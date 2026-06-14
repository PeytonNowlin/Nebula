mod builtins;
mod diagnostic_extract;
mod probe;
mod probe_bundle;
mod probe_manifest;

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use miette::Diagnostic;
use nebula_ast::{BinaryOp, UnaryOp};
use nebula_builtins::is_builtin;
use nebula_ast::Span;
use nebula_ir::{IrExpr, IrExprKind, IrProgram, IrStmt};
use serde::Serialize;
use thiserror::Error;

pub use builtins::missing_runtime_handlers;
pub use probe::{ProbeHost, ProbeInvocation, RegistryProbeHost};
pub use probe_manifest::{
    list_probe_manifest, read_probe_manifest, validate_manifest, DeclaredProbe, McpServerReport,
    ProbeBinding, ProbeListReport, ProbeManifest,
};

#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    None,
    Some(Box<Value>),
    List(Vec<Value>),
    Map(HashMap<String, Value>),
    Struct {
        name: String,
        fields: HashMap<String, Value>,
    },
}

#[derive(Debug, Error, Diagnostic)]
pub enum RuntimeError {
    #[error("NEB-R002 [runtime_error] {message}")]
    #[diagnostic(code(nebula::runtime_error))]
    Error { message: String },

    #[error("NEB-P001 [probe_error] undefined probe `{name}`")]
    #[diagnostic(code(nebula::undefined_probe))]
    UndefinedProbe { name: String },

    #[error("NEB-P002 [probe_error] probe `{name}` is not implemented by the host")]
    #[diagnostic(code(nebula::probe_not_implemented))]
    ProbeNotImplemented { name: String },

    #[error("NEB-P003 [probe_error] probe `{name}` failed: {message}")]
    #[diagnostic(code(nebula::probe_failed))]
    ProbeFailed { name: String, message: String },

    #[error("NEB-P004 [probe_error] MCP transport error: {message}")]
    #[diagnostic(code(nebula::mcp_transport))]
    McpTransport { message: String },

    #[error("NEB-R003 [runtime_error] undefined variable `{name}`")]
    #[diagnostic(code(nebula::undefined_var))]
    UndefinedVar { name: String, span: Span },

    #[error("NEB-R004 [runtime_error] division by zero")]
    #[diagnostic(code(nebula::divide_by_zero))]
    DivideByZero { span: Span },

    #[error("NEB-R005 [runtime_error] list index {index} out of bounds (len {len})")]
    #[diagnostic(code(nebula::index_out_of_bounds))]
    IndexOutOfBounds {
        index: i64,
        len: usize,
        span: Span,
    },

    #[error("NEB-R006 [runtime_error] key `{key}` not found in map")]
    #[diagnostic(code(nebula::key_not_found))]
    KeyNotFound { key: String, span: Span },

    #[error("NEB-R007 [runtime_error] integer overflow in `{op}`")]
    #[diagnostic(code(nebula::integer_overflow))]
    IntegerOverflow { op: String, span: Span },
}

impl nebula_ast::NebError for RuntimeError {
    fn neb_code(&self) -> &'static str {
        match self {
            RuntimeError::Error { .. } => "NEB-R002",
            RuntimeError::UndefinedProbe { .. } => "NEB-P001",
            RuntimeError::ProbeNotImplemented { .. } => "NEB-P002",
            RuntimeError::ProbeFailed { .. } => "NEB-P003",
            RuntimeError::McpTransport { .. } => "NEB-P004",
            RuntimeError::UndefinedVar { .. } => "NEB-R003",
            RuntimeError::DivideByZero { .. } => "NEB-R004",
            RuntimeError::IndexOutOfBounds { .. } => "NEB-R005",
            RuntimeError::KeyNotFound { .. } => "NEB-R006",
            RuntimeError::IntegerOverflow { .. } => "NEB-R007",
        }
    }

    fn neb_message(&self) -> String {
        match self {
            RuntimeError::Error { message, .. } => message.clone(),
            RuntimeError::UndefinedProbe { name, .. } => {
                format!("undefined probe `{name}`")
            }
            RuntimeError::ProbeNotImplemented { name, .. } => {
                format!("probe `{name}` is not implemented by the host")
            }
            RuntimeError::ProbeFailed { name, message, .. } => {
                format!("probe `{name}` failed: {message}")
            }
            RuntimeError::McpTransport { message, .. } => {
                format!("MCP transport error: {message}")
            }
            RuntimeError::UndefinedVar { name, .. } => format!("undefined variable `{name}`"),
            RuntimeError::DivideByZero { .. } => "division by zero".to_string(),
            RuntimeError::IndexOutOfBounds { index, len, .. } => {
                format!("list index {index} out of bounds (len {len})")
            }
            RuntimeError::KeyNotFound { key, .. } => format!("key `{key}` not found in map"),
            RuntimeError::IntegerOverflow { op, .. } => {
                format!("integer overflow in `{op}`")
            }
        }
    }

    fn neb_span(&self) -> Option<nebula_ast::Span> {
        match self {
            RuntimeError::UndefinedVar { span, .. }
            | RuntimeError::DivideByZero { span }
            | RuntimeError::IndexOutOfBounds { span, .. }
            | RuntimeError::KeyNotFound { span, .. }
            | RuntimeError::IntegerOverflow { span, .. } => Some(span.clone()),
            _ => None,
        }
    }
}

#[derive(Serialize)]
struct TelemetryEvent {
    step: String,
    detail: String,
}

pub struct Runtime {
    env: HashMap<String, Value>,
    functions: HashMap<String, nebula_ir::IrFunction>,
    probes: HashMap<String, nebula_ir::ProbeInfo>,
    probe_host: RegistryProbeHost,
    current_sector: Option<String>,
    telemetry_path: Option<String>,
    telemetry_enabled: bool,
    capture_print: bool,
    printed: Vec<String>,
}

impl Runtime {
    pub fn new(program: &IrProgram) -> Self {
        let mut functions = HashMap::new();
        for (sector_name, sector) in &program.sectors {
            for func in sector.functions.values() {
                functions.insert(func.qualified_name.clone(), func.clone());
                let _ = sector_name;
            }
        }

        Self {
            env: HashMap::new(),
            functions,
            probes: program.probes.clone(),
            probe_host: RegistryProbeHost::with_defaults(),
            current_sector: None,
            telemetry_path: None,
            telemetry_enabled: false,
            capture_print: false,
            printed: Vec::new(),
        }
    }

    /// Capture `print` output in [`Runtime::take_printed`] instead of writing to stdout.
    pub fn with_capture_print(mut self, capture: bool) -> Self {
        self.capture_print = capture;
        self
    }

    /// Drain lines emitted by `print` when print capture is enabled.
    pub fn take_printed(&mut self) -> Vec<String> {
        std::mem::take(&mut self.printed)
    }

    pub fn with_probe_host(mut self, host: RegistryProbeHost) -> Self {
        self.probe_host = host;
        self
    }

    pub fn with_probe_manifest(mut self, path: &Path) -> Result<Self, RuntimeError> {
        self.probe_host.load_manifest(path)?;
        Ok(self)
    }

    pub fn with_telemetry(mut self, path: String) -> Self {
        self.telemetry_path = Some(path);
        self
    }

    /// Coverage hook used by sync tests to verify manifest builtins dispatch.
    #[doc(hidden)]
    pub fn eval_builtin_for_coverage(&mut self, name: &str) -> Result<Value, RuntimeError> {
        self.eval_builtin(name, &[], 0..0)
    }

    pub fn run(&mut self, program: &IrProgram) -> Result<Value, RuntimeError> {
        let mut result = Value::None;
        for stmt in &program.mission.stmts {
            if let Some(v) = self.exec_stmt(stmt)? {
                result = v;
            }
        }
        Ok(result)
    }

    fn exec_stmt(&mut self, stmt: &IrStmt) -> Result<Option<Value>, RuntimeError> {
        match stmt {
            IrStmt::Let { name, value, .. } => {
                let v = self.eval_expr(value)?;
                self.env.insert(name.clone(), v);
                self.log_telemetry("let", name);
                Ok(None)
            }
            IrStmt::Set { name, value } => {
                if !self.env.contains_key(name) {
                    return Err(RuntimeError::UndefinedVar {
                        name: name.clone(),
                        span: value.span.clone(),
                    });
                }
                let v = self.eval_expr(value)?;
                self.env.insert(name.clone(), v);
                self.log_telemetry("set", name);
                Ok(None)
            }
            IrStmt::If {
                condition,
                then_body,
                else_body,
            } => {
                let cond = self.eval_expr(condition)?;
                let branch = if is_truthy(&cond) {
                    then_body
                } else {
                    else_body.as_ref().map(|b| b.as_slice()).unwrap_or(&[])
                };
                for s in branch {
                    if let Some(v) = self.exec_stmt(s)? {
                        return Ok(Some(v));
                    }
                }
                Ok(None)
            }
            IrStmt::While { condition, body } => {
                while is_truthy(&self.eval_expr(condition)?) {
                    for s in body {
                        if let Some(v) = self.exec_stmt(s)? {
                            return Ok(Some(v));
                        }
                    }
                }
                Ok(None)
            }
            IrStmt::Return(expr) => Ok(Some(self.eval_expr(expr)?)),
            IrStmt::Expr(expr) => {
                self.eval_expr(expr)?;
                Ok(None)
            }
            IrStmt::ProbeCall { name, args } => {
                self.invoke_probe(name, args)?;
                Ok(None)
            }
            IrStmt::Telemetry { body } => {
                let prev = self.telemetry_enabled;
                self.telemetry_enabled = true;
                for s in body {
                    if let Some(v) = self.exec_stmt(s)? {
                        self.telemetry_enabled = prev;
                        return Ok(Some(v));
                    }
                }
                self.telemetry_enabled = prev;
                Ok(None)
            }
        }
    }

    fn eval_expr(&mut self, expr: &IrExpr) -> Result<Value, RuntimeError> {
        match &expr.node {
            IrExprKind::Int(n) => Ok(Value::Int(*n)),
            IrExprKind::Float(n) => Ok(Value::Float(*n)),
            IrExprKind::Str(s) => Ok(Value::Str(s.clone())),
            IrExprKind::Bool(b) => Ok(Value::Bool(*b)),
            IrExprKind::None => Ok(Value::None),
            IrExprKind::Some(inner) => Ok(Value::Some(Box::new(self.eval_expr(inner)?))),
            IrExprKind::Var(name) => self.env.get(name).cloned().ok_or(RuntimeError::UndefinedVar {
                name: name.clone(),
                span: expr.span.clone(),
            }),
            IrExprKind::Unary { op, operand } => {
                let v = self.eval_expr(operand)?;
                match op {
                    UnaryOp::Not => Ok(Value::Bool(!is_truthy(&v))),
                }
            }
            IrExprKind::Binary { left, op, right } => {
                let l = self.eval_expr(left)?;
                let r = self.eval_expr(right)?;
                eval_binary(*op, l, r, expr.span.clone())
            }
            IrExprKind::Call { name, args } => {
                if is_builtin(name) {
                    return self.eval_builtin(name, args, expr.span.clone());
                }

                let resolved = self.resolve_function(name);
                let func = self
                    .functions
                    .get(&resolved)
                    .cloned()
                    .ok_or(RuntimeError::Error {
                        message: format!("undefined function `{name}`"),
                    })?;

                if func.params.len() != args.len() {
                    return Err(RuntimeError::Error {
                        message: format!("wrong argument count for `{name}`"),
                    });
                }

                let mut arg_values = Vec::new();
                for arg in args {
                    arg_values.push(self.eval_expr(arg)?);
                }

                let saved_env = std::mem::take(&mut self.env);
                let saved_sector = self.current_sector.replace(func.sector.clone());
                for (param, value) in func.params.iter().zip(arg_values) {
                    self.env.insert(param.clone(), value);
                }

                let mut result = Value::None;
                for stmt in &func.body {
                    if let Some(v) = self.exec_stmt(stmt)? {
                        result = v;
                        break;
                    }
                }

                self.env = saved_env;
                self.current_sector = saved_sector;
                Ok(result)
            }
            IrExprKind::List(items) => {
                let mut vals = Vec::new();
                for item in items {
                    vals.push(self.eval_expr(item)?);
                }
                Ok(Value::List(vals))
            }
            IrExprKind::Map(entries) => {
                let mut map = HashMap::new();
                for (k, v) in entries {
                    let key = value_to_string(&self.eval_expr(k)?)?;
                    map.insert(key, self.eval_expr(v)?);
                }
                Ok(Value::Map(map))
            }
            IrExprKind::Struct { name, fields } => {
                let mut map = HashMap::new();
                for (k, v) in fields {
                    map.insert(k.clone(), self.eval_expr(v)?);
                }
                Ok(Value::Struct {
                    name: name.clone(),
                    fields: map,
                })
            }
            IrExprKind::FieldAccess { object, field } => {
                let val = self.eval_expr(object)?;
                match val {
                    Value::Struct { fields, .. } => fields
                        .get(field)
                        .cloned()
                        .ok_or(RuntimeError::Error {
                            message: format!("unknown field `{field}`"),
                        }),
                    _ => Err(RuntimeError::Error {
                        message: "field access on non-struct".into(),
                    }),
                }
            }
            IrExprKind::ProbeCall { name, args } => self.invoke_probe(name, args),
        }
    }

    fn invoke_probe(
        &mut self,
        name: &str,
        args: &HashMap<String, IrExpr>,
    ) -> Result<Value, RuntimeError> {
        let resolved = self.resolve_probe(name);
        if !self.probes.contains_key(&resolved) {
            return Err(RuntimeError::UndefinedProbe {
                name: name.to_string(),
            });
        }
        let mut evaluated = HashMap::new();
        for (k, v) in args {
            evaluated.insert(k.clone(), self.eval_expr(v)?);
        }
        self.log_telemetry("probe", &resolved);
        self.probe_host.invoke(&ProbeInvocation {
            name: &resolved,
            args: evaluated,
        })
    }

    fn resolve_function(&self, name: &str) -> String {
        if self.functions.contains_key(name) {
            return name.to_string();
        }
        if !name.contains('.') {
            if let Some(sector) = &self.current_sector {
                let qualified = format!("{sector}.{name}");
                if self.functions.contains_key(&qualified) {
                    return qualified;
                }
            }
        }
        name.to_string()
    }

    fn resolve_probe(&self, name: &str) -> String {
        if self.probes.contains_key(name) {
            return name.to_string();
        }
        if !name.contains('.') {
            if let Some(sector) = &self.current_sector {
                let qualified = format!("{sector}.{name}");
                if self.probes.contains_key(&qualified) {
                    return qualified;
                }
            }
        }
        name.to_string()
    }

    fn log_telemetry(&self, step: &str, detail: &str) {
        if !self.telemetry_enabled {
            return;
        }
        if let Some(path) = &self.telemetry_path {
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
                let event = TelemetryEvent {
                    step: step.into(),
                    detail: detail.into(),
                };
                if let Ok(line) = serde_json::to_string(&event) {
                    let _ = writeln!(file, "{line}");
                }
            }
        }
    }
}

fn eval_binary(op: BinaryOp, l: Value, r: Value, span: Span) -> Result<Value, RuntimeError> {
    match op {
        BinaryOp::Plus => match (l, r) {
            // Integer arithmetic is checked: overflow is a deterministic error
            // (NEB-R007), never a silent wrap, so the interpreter and the
            // arbitrary-precision Python backend cannot diverge.
            (Value::Int(a), Value::Int(b)) => a
                .checked_add(b)
                .map(Value::Int)
                .ok_or(RuntimeError::IntegerOverflow {
                    op: "plus".into(),
                    span: span.clone(),
                }),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
            (Value::Str(mut a), Value::Str(b)) => {
                a.push_str(&b);
                Ok(Value::Str(a))
            }
            _ => Err(RuntimeError::Error {
                message: "invalid plus operands".into(),
            }),
        },
        BinaryOp::Minus => match (l, r) {
            (Value::Int(a), Value::Int(b)) => a
                .checked_sub(b)
                .map(Value::Int)
                .ok_or(RuntimeError::IntegerOverflow {
                    op: "minus".into(),
                    span: span.clone(),
                }),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
            _ => Err(RuntimeError::Error {
                message: "invalid minus operands".into(),
            }),
        },
        BinaryOp::Times => match (l, r) {
            (Value::Int(a), Value::Int(b)) => a
                .checked_mul(b)
                .map(Value::Int)
                .ok_or(RuntimeError::IntegerOverflow {
                    op: "times".into(),
                    span: span.clone(),
                }),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
            _ => Err(RuntimeError::Error {
                message: "invalid times operands".into(),
            }),
        },
        BinaryOp::Div => match (l, r) {
            (Value::Int(_), Value::Int(0)) => Err(RuntimeError::DivideByZero { span: span.clone() }),
            // Integer div truncates toward zero (C-style); the Python backend
            // mirrors this in `nebula_div`. `checked_div` also traps the lone
            // overflowing case, i64::MIN / -1.
            (Value::Int(a), Value::Int(b)) => a
                .checked_div(b)
                .map(Value::Int)
                .ok_or(RuntimeError::IntegerOverflow {
                    op: "div".into(),
                    span: span.clone(),
                }),
            (Value::Float(_), Value::Float(0.0)) => Err(RuntimeError::DivideByZero { span: span.clone() }),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
            _ => Err(RuntimeError::Error {
                message: "invalid div operands".into(),
            }),
        },
        BinaryOp::Mod => match (l, r) {
            (Value::Int(_), Value::Int(0)) => Err(RuntimeError::DivideByZero { span: span.clone() }),
            (Value::Int(a), Value::Int(b)) => a
                .checked_rem(b)
                .map(Value::Int)
                .ok_or(RuntimeError::IntegerOverflow {
                    op: "mod".into(),
                    span: span.clone(),
                }),
            (Value::Float(_), Value::Float(0.0)) => Err(RuntimeError::DivideByZero { span }),
            // f64 `%` keeps the sign of the dividend, matching Python math.fmod.
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a % b)),
            _ => Err(RuntimeError::Error {
                message: "invalid mod operands".into(),
            }),
        },
        BinaryOp::Eq => Ok(Value::Bool(values_equal(&l, &r))),
        BinaryOp::Ne => Ok(Value::Bool(!values_equal(&l, &r))),
        BinaryOp::Lt => match (l, r) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a < b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a < b)),
            _ => Err(RuntimeError::Error {
                message: "invalid lt operands".into(),
            }),
        },
        BinaryOp::Gt => match (l, r) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a > b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a > b)),
            _ => Err(RuntimeError::Error {
                message: "invalid gt operands".into(),
            }),
        },
        BinaryOp::Le => match (l, r) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a <= b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a <= b)),
            _ => Err(RuntimeError::Error {
                message: "invalid le operands".into(),
            }),
        },
        BinaryOp::Ge => match (l, r) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a >= b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a >= b)),
            _ => Err(RuntimeError::Error {
                message: "invalid ge operands".into(),
            }),
        },
        BinaryOp::And => Ok(Value::Bool(is_truthy(&l) && is_truthy(&r))),
        BinaryOp::Or => Ok(Value::Bool(is_truthy(&l) || is_truthy(&r))),
    }
}

fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Int(n) => *n != 0,
        Value::None => false,
        Value::Str(s) => !s.is_empty(),
        Value::List(items) => !items.is_empty(),
        _ => true,
    }
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => x == y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::None, Value::None) => true,
        (Value::Some(x), Value::Some(y)) => values_equal(x, y),
        (Value::List(x), Value::List(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(l, r)| values_equal(l, r))
        }
        (Value::Map(x), Value::Map(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| values_equal(v, w)))
        }
        (
            Value::Struct {
                name: n1,
                fields: f1,
            },
            Value::Struct {
                name: n2,
                fields: f2,
            },
        ) => {
            n1 == n2
                && f1.len() == f2.len()
                && f1
                    .iter()
                    .all(|(k, v)| f2.get(k).is_some_and(|w| values_equal(v, w)))
        }
        _ => false,
    }
}

fn value_to_string(v: &Value) -> Result<String, RuntimeError> {
    match v {
        Value::Str(s) => Ok(s.clone()),
        Value::Int(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Float(n) => Ok(builtins::format_float(*n)),
        _ => Err(RuntimeError::Error {
            message: "cannot convert value to string".into(),
        }),
    }
}