mod builtins;
mod diagnostic_extract;
mod limits;
mod probe;
mod probe_bundle;
mod probe_manifest;
mod secrets;
mod telemetry_format;
pub mod value_json;

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::time::Instant;

use miette::Diagnostic;
use nebula_ast::Span;
use nebula_ast::{BinaryOp, UnaryOp};
use nebula_builtins::is_builtin;
use nebula_ir::{IrExpr, IrExprKind, IrProgram, IrStmt};
use thiserror::Error;

pub use builtins::missing_runtime_handlers;
pub use limits::ResourceLimits;
pub use probe::{ProbeHost, ProbeInvocation, ProbeJsonlEvent, RegistryProbeHost};
pub use probe_manifest::{
    list_probe_manifest, prepare_probe_manifest, read_probe_manifest, validate_manifest,
    DeclaredProbe, McpServerReport, ProbeBinding, ProbeListReport, ProbeManifest,
};
pub use secrets::{resolve_secrets, SecretBinding, SecretsStore};
pub use telemetry_format::ProbeCallRecord;

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
    Error { message: String, span: Span },

    #[error("NEB-P001 [probe_error] undefined probe `{name}`")]
    #[diagnostic(code(nebula::undefined_probe))]
    UndefinedProbe { name: String, span: Span },

    #[error("NEB-P002 [probe_error] probe `{name}` is not implemented by the host")]
    #[diagnostic(code(nebula::probe_not_implemented))]
    ProbeNotImplemented { name: String, span: Span },

    #[error("NEB-P003 [probe_error] probe `{name}` failed: {message}")]
    #[diagnostic(code(nebula::probe_failed))]
    ProbeFailed {
        name: String,
        message: String,
        span: Span,
    },

    #[error("NEB-P004 [probe_error] MCP transport error: {message}")]
    #[diagnostic(code(nebula::mcp_transport))]
    McpTransport { message: String, span: Span },

    #[error("NEB-R003 [runtime_error] undefined variable `{name}`")]
    #[diagnostic(code(nebula::undefined_var))]
    UndefinedVar { name: String, span: Span },

    #[error("NEB-R004 [runtime_error] division by zero")]
    #[diagnostic(code(nebula::divide_by_zero))]
    DivideByZero { span: Span },

    #[error("NEB-R005 [runtime_error] list index {index} out of bounds (len {len})")]
    #[diagnostic(code(nebula::index_out_of_bounds))]
    IndexOutOfBounds { index: i64, len: usize, span: Span },

    #[error("NEB-R006 [runtime_error] key `{key}` not found in map")]
    #[diagnostic(code(nebula::key_not_found))]
    KeyNotFound { key: String, span: Span },

    #[error("NEB-R007 [runtime_error] integer overflow in `{op}`")]
    #[diagnostic(code(nebula::integer_overflow))]
    IntegerOverflow { op: String, span: Span },

    #[error("NEB-R008 [runtime_error] execution exceeded time limit of {limit_ms}ms")]
    #[diagnostic(code(nebula::execution_timeout))]
    ExecutionTimeout { limit_ms: u64, span: Span },

    #[error("NEB-R009 [runtime_error] while-loop iteration limit of {limit} exceeded")]
    #[diagnostic(code(nebula::loop_iteration_limit))]
    LoopIterationLimit { limit: u64, span: Span },

    #[error("NEB-R010 [runtime_error] memory limit of {limit_bytes} bytes exceeded (used {used_bytes} bytes)")]
    #[diagnostic(code(nebula::memory_limit_exceeded))]
    MemoryLimitExceeded {
        limit_bytes: usize,
        used_bytes: usize,
        span: Span,
    },
}

impl RuntimeError {
    pub(crate) fn with_diagnostic_span(self, span: Span) -> Self {
        if span.is_empty() {
            return self;
        }
        match self {
            Self::Error { message, .. } => Self::Error { message, span },
            Self::UndefinedProbe { name, .. } => Self::UndefinedProbe { name, span },
            Self::ProbeNotImplemented { name, .. } => Self::ProbeNotImplemented { name, span },
            Self::ProbeFailed { name, message, .. } => Self::ProbeFailed {
                name,
                message,
                span,
            },
            Self::McpTransport { message, .. } => Self::McpTransport { message, span },
            Self::ExecutionTimeout { limit_ms, .. } => Self::ExecutionTimeout { limit_ms, span },
            Self::MemoryLimitExceeded {
                limit_bytes,
                used_bytes,
                ..
            } => Self::MemoryLimitExceeded {
                limit_bytes,
                used_bytes,
                span,
            },
            other => other,
        }
    }
}

fn meaningful_span(span: &Span) -> Option<Span> {
    if span.is_empty() {
        None
    } else {
        Some(span.clone())
    }
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
            RuntimeError::ExecutionTimeout { .. } => "NEB-R008",
            RuntimeError::LoopIterationLimit { .. } => "NEB-R009",
            RuntimeError::MemoryLimitExceeded { .. } => "NEB-R010",
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
            RuntimeError::ExecutionTimeout { limit_ms, .. } => {
                format!("execution exceeded time limit of {limit_ms}ms")
            }
            RuntimeError::LoopIterationLimit { limit, .. } => {
                format!("while-loop iteration limit of {limit} exceeded")
            }
            RuntimeError::MemoryLimitExceeded {
                limit_bytes,
                used_bytes,
                ..
            } => format!("memory limit of {limit_bytes} bytes exceeded (used {used_bytes} bytes)"),
        }
    }

    fn neb_span(&self) -> Option<nebula_ast::Span> {
        match self {
            RuntimeError::Error { span, .. }
            | RuntimeError::UndefinedProbe { span, .. }
            | RuntimeError::ProbeNotImplemented { span, .. }
            | RuntimeError::ProbeFailed { span, .. }
            | RuntimeError::McpTransport { span, .. }
            | RuntimeError::UndefinedVar { span, .. }
            | RuntimeError::DivideByZero { span }
            | RuntimeError::IndexOutOfBounds { span, .. }
            | RuntimeError::KeyNotFound { span, .. }
            | RuntimeError::IntegerOverflow { span, .. }
            | RuntimeError::ExecutionTimeout { span, .. }
            | RuntimeError::LoopIterationLimit { span, .. }
            | RuntimeError::MemoryLimitExceeded { span, .. } => meaningful_span(span),
        }
    }
}

use telemetry_format::TelemetryEvent;

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
    probes_called: Vec<telemetry_format::ProbeCallRecord>,
    pub(crate) limits: limits::ResourceLimits,
    pub(crate) started_at: Option<Instant>,
    pub(crate) loop_iterations: u64,
    pub(crate) memory_bytes: usize,
    diagnostic_span: Span,
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
            probes_called: Vec::new(),
            limits: limits::ResourceLimits::unlimited(),
            started_at: None,
            loop_iterations: 0,
            memory_bytes: 0,
            diagnostic_span: 0..0,
        }
    }

    pub(crate) fn runtime_error(&self, message: impl Into<String>) -> RuntimeError {
        RuntimeError::Error {
            message: message.into(),
            span: self.diagnostic_span.clone(),
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

    /// Drain jsonl probe events captured during execution.
    pub fn take_probe_events(&mut self) -> Vec<ProbeJsonlEvent> {
        self.probe_host.take_probe_events()
    }

    /// Drain all probe invocations captured during execution.
    pub fn take_probes_called(&mut self) -> Vec<telemetry_format::ProbeCallRecord> {
        std::mem::take(&mut self.probes_called)
    }

    pub fn with_probe_host(mut self, host: RegistryProbeHost) -> Self {
        self.probe_host = host;
        self
    }

    pub fn with_probe_manifest(
        mut self,
        path: &Path,
        secrets_overlay: Option<&SecretsStore>,
    ) -> Result<Self, RuntimeError> {
        self.probe_host.load_manifest(path, secrets_overlay)?;
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
        self.begin_run_budget();
        let mut result = Value::None;
        for stmt in &program.mission.stmts {
            if let Some(v) = self.exec_stmt(stmt)? {
                result = v;
            }
        }
        Ok(result)
    }

    fn exec_stmt(&mut self, stmt: &IrStmt) -> Result<Option<Value>, RuntimeError> {
        self.diagnostic_span = stmt_diagnostic_span(stmt);
        self.check_timeout()?;
        match stmt {
            IrStmt::Let { name, value, .. } => {
                let v = self.eval_expr(value)?;
                self.charge_env_binding(name, v)?;
                if let Some(bound) = self.env.get(name) {
                    self.log_telemetry_binding("let", name, bound);
                }
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
                self.charge_env_binding(name, v)?;
                if let Some(bound) = self.env.get(name) {
                    self.log_telemetry_binding("set", name, bound);
                }
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
                while {
                    self.bump_loop_iteration(condition.span.clone())?;
                    is_truthy(&self.eval_expr(condition)?)
                } {
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
                let span = probe_call_span(args);
                self.invoke_probe(name, args, span)?;
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
        self.diagnostic_span = expr.span.clone();
        self.check_timeout()?;
        match &expr.node {
            IrExprKind::Int(n) => Ok(Value::Int(*n)),
            IrExprKind::Float(n) => Ok(Value::Float(*n)),
            IrExprKind::Str(s) => self.finish_value(Value::Str(s.clone())),
            IrExprKind::Bool(b) => Ok(Value::Bool(*b)),
            IrExprKind::None => Ok(Value::None),
            IrExprKind::Some(inner) => {
                let inner = self.eval_expr(inner)?;
                self.finish_value(Value::Some(Box::new(inner)))
            }
            IrExprKind::Var(name) => {
                self.env
                    .get(name)
                    .cloned()
                    .ok_or(RuntimeError::UndefinedVar {
                        name: name.clone(),
                        span: expr.span.clone(),
                    })
            }
            IrExprKind::Unary { op, operand } => {
                let v = self.eval_expr(operand)?;
                match op {
                    UnaryOp::Not => Ok(Value::Bool(!is_truthy(&v))),
                }
            }
            IrExprKind::Binary { left, op, right } => {
                let l = self.eval_expr(left)?;
                let r = self.eval_expr(right)?;
                self.finish_value(eval_binary(*op, l, r, expr.span.clone())?)
            }
            IrExprKind::Call { name, args } => {
                if is_builtin(name) {
                    return self.eval_builtin(name, args, expr.span.clone());
                }

                let resolved = self.resolve_function(name);
                let func =
                    self.functions.get(&resolved).cloned().ok_or_else(|| {
                        self.runtime_error(format!("undefined function `{name}`"))
                    })?;

                if func.params.len() != args.len() {
                    return Err(self.runtime_error(format!("wrong argument count for `{name}`")));
                }

                let mut arg_values = Vec::new();
                for arg in args {
                    arg_values.push(self.eval_expr(arg)?);
                }

                let (saved_env, saved_memory) = self.take_env_budget();
                let saved_sector = self.current_sector.replace(func.sector.clone());
                for (param, value) in func.params.iter().zip(arg_values) {
                    self.charge_env_binding(param, value)?;
                }

                let mut result = Value::None;
                for stmt in &func.body {
                    if let Some(v) = self.exec_stmt(stmt)? {
                        result = v;
                        break;
                    }
                }

                self.restore_env_budget(saved_env, saved_memory);
                self.current_sector = saved_sector;
                self.finish_value(result)
            }
            IrExprKind::List(items) => {
                let mut vals = Vec::new();
                for item in items {
                    vals.push(self.eval_expr(item)?);
                }
                self.finish_value(Value::List(vals))
            }
            IrExprKind::Map(entries) => {
                let mut map = HashMap::new();
                for (k, v) in entries {
                    let key = value_to_string(&self.eval_expr(k)?)?;
                    map.insert(key, self.eval_expr(v)?);
                }
                self.finish_value(Value::Map(map))
            }
            IrExprKind::Struct { name, fields } => {
                let mut map = HashMap::new();
                for (k, v) in fields {
                    map.insert(k.clone(), self.eval_expr(v)?);
                }
                self.finish_value(Value::Struct {
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
                        .ok_or_else(|| self.runtime_error(format!("unknown field `{field}`"))),
                    _ => Err(self.runtime_error("field access on non-struct")),
                }
            }
            IrExprKind::ProbeCall { name, args } => {
                let span = expr.span.clone();
                let value = self.invoke_probe(name, args, span)?;
                self.finish_value(value)
            }
        }
    }

    fn invoke_probe(
        &mut self,
        name: &str,
        args: &HashMap<String, IrExpr>,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let resolved = self.resolve_probe(name);
        if !self.probes.contains_key(&resolved) {
            return Err(RuntimeError::UndefinedProbe {
                name: name.to_string(),
                span,
            });
        }
        let mut evaluated = HashMap::new();
        for (k, v) in args {
            evaluated.insert(k.clone(), self.eval_expr(v)?);
        }
        self.check_timeout()?;
        let value = self
            .probe_host
            .invoke(&ProbeInvocation {
                name: &resolved,
                args: evaluated.clone(),
            })
            .map_err(|err| err.with_diagnostic_span(span.clone()))?;
        self.probes_called.push(telemetry_format::probe_call_record(
            &resolved, &evaluated, &value,
        ));
        self.log_telemetry_probe(&resolved, &evaluated, &value);
        self.check_timeout()?;
        Ok(value)
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

    fn log_telemetry_event(&self, event: TelemetryEvent) {
        if !self.telemetry_enabled {
            return;
        }
        if let Some(path) = &self.telemetry_path {
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
                if let Ok(line) = serde_json::to_string(&event) {
                    let _ = writeln!(file, "{line}");
                }
            }
        }
    }

    fn log_telemetry_binding(&self, step: &str, name: &str, value: &Value) {
        self.log_telemetry_event(TelemetryEvent {
            step: step.into(),
            detail: name.into(),
            value: Some(telemetry_format::binding_value(value)),
            args: None,
            result: None,
        });
    }

    fn log_telemetry_probe(&self, name: &str, args: &HashMap<String, Value>, result: &Value) {
        let short = name.rsplit('.').next().unwrap_or(name);
        let redact_args = short == "secret_get";
        self.log_telemetry_event(TelemetryEvent {
            step: "probe".into(),
            detail: name.into(),
            value: None,
            args: Some(telemetry_format::probe_args(args, redact_args)),
            result: Some(telemetry_format::probe_result_summary(name, result)),
        });
    }
}

fn eval_binary(op: BinaryOp, l: Value, r: Value, span: Span) -> Result<Value, RuntimeError> {
    match op {
        BinaryOp::Plus => match (l, r) {
            // Integer arithmetic is checked: overflow is a deterministic error
            // (NEB-R007), never a silent wrap, so the interpreter and the
            // arbitrary-precision Python backend cannot diverge.
            (Value::Int(a), Value::Int(b)) => {
                a.checked_add(b)
                    .map(Value::Int)
                    .ok_or(RuntimeError::IntegerOverflow {
                        op: "plus".into(),
                        span: span.clone(),
                    })
            }
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
            (Value::Str(mut a), Value::Str(b)) => {
                a.push_str(&b);
                Ok(Value::Str(a))
            }
            _ => Err(RuntimeError::Error {
                message: "invalid plus operands".into(),
                span: span.clone(),
            }),
        },
        BinaryOp::Minus => match (l, r) {
            (Value::Int(a), Value::Int(b)) => {
                a.checked_sub(b)
                    .map(Value::Int)
                    .ok_or(RuntimeError::IntegerOverflow {
                        op: "minus".into(),
                        span: span.clone(),
                    })
            }
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
            _ => Err(RuntimeError::Error {
                message: "invalid minus operands".into(),
                span: span.clone(),
            }),
        },
        BinaryOp::Times => match (l, r) {
            (Value::Int(a), Value::Int(b)) => {
                a.checked_mul(b)
                    .map(Value::Int)
                    .ok_or(RuntimeError::IntegerOverflow {
                        op: "times".into(),
                        span: span.clone(),
                    })
            }
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
            _ => Err(RuntimeError::Error {
                message: "invalid times operands".into(),
                span: span.clone(),
            }),
        },
        BinaryOp::Div => match (l, r) {
            (Value::Int(_), Value::Int(0)) => {
                Err(RuntimeError::DivideByZero { span: span.clone() })
            }
            // Integer div truncates toward zero (C-style); the Python backend
            // mirrors this in `nebula_div`. `checked_div` also traps the lone
            // overflowing case, i64::MIN / -1.
            (Value::Int(a), Value::Int(b)) => {
                a.checked_div(b)
                    .map(Value::Int)
                    .ok_or(RuntimeError::IntegerOverflow {
                        op: "div".into(),
                        span: span.clone(),
                    })
            }
            (Value::Float(_), Value::Float(0.0)) => {
                Err(RuntimeError::DivideByZero { span: span.clone() })
            }
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
            _ => Err(RuntimeError::Error {
                message: "invalid div operands".into(),
                span: span.clone(),
            }),
        },
        BinaryOp::Mod => match (l, r) {
            (Value::Int(_), Value::Int(0)) => {
                Err(RuntimeError::DivideByZero { span: span.clone() })
            }
            (Value::Int(a), Value::Int(b)) => {
                a.checked_rem(b)
                    .map(Value::Int)
                    .ok_or(RuntimeError::IntegerOverflow {
                        op: "mod".into(),
                        span: span.clone(),
                    })
            }
            (Value::Float(_), Value::Float(0.0)) => Err(RuntimeError::DivideByZero { span }),
            // f64 `%` keeps the sign of the dividend, matching Python math.fmod.
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a % b)),
            _ => Err(RuntimeError::Error {
                message: "invalid mod operands".into(),
                span: span.clone(),
            }),
        },
        BinaryOp::Eq => Ok(Value::Bool(values_equal(&l, &r))),
        BinaryOp::Ne => Ok(Value::Bool(!values_equal(&l, &r))),
        BinaryOp::Lt => match (l, r) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a < b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a < b)),
            _ => Err(RuntimeError::Error {
                message: "invalid lt operands".into(),
                span: span.clone(),
            }),
        },
        BinaryOp::Gt => match (l, r) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a > b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a > b)),
            _ => Err(RuntimeError::Error {
                message: "invalid gt operands".into(),
                span: span.clone(),
            }),
        },
        BinaryOp::Le => match (l, r) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a <= b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a <= b)),
            _ => Err(RuntimeError::Error {
                message: "invalid le operands".into(),
                span: span.clone(),
            }),
        },
        BinaryOp::Ge => match (l, r) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a >= b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a >= b)),
            _ => Err(RuntimeError::Error {
                message: "invalid ge operands".into(),
                span: span.clone(),
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
            span: 0..0,
        }),
    }
}

fn stmt_diagnostic_span(stmt: &IrStmt) -> Span {
    match stmt {
        IrStmt::Let { value, .. } | IrStmt::Set { value, .. } => value.span.clone(),
        IrStmt::Return(expr) | IrStmt::Expr(expr) => expr.span.clone(),
        IrStmt::If { condition, .. } | IrStmt::While { condition, .. } => condition.span.clone(),
        IrStmt::ProbeCall { args, .. } => probe_call_span(args),
        IrStmt::Telemetry { body } => body
            .first()
            .map(stmt_diagnostic_span)
            .unwrap_or_else(|| 0..0),
    }
}

fn probe_call_span(args: &HashMap<String, IrExpr>) -> Span {
    args.values()
        .map(|expr| expr.span.clone())
        .min_by_key(|span| span.start)
        .unwrap_or(0..0)
}
