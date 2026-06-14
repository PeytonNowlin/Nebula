use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;

use miette::Diagnostic;
use nebula_ast::{BinaryOp, UnaryOp};
use nebula_ir::{IrExpr, IrProgram, IrStmt};
use serde::Serialize;
use thiserror::Error;

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

    #[error("NEB-R003 [runtime_error] undefined variable `{name}`")]
    #[diagnostic(code(nebula::undefined_var))]
    UndefinedVar { name: String },
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
    current_sector: Option<String>,
    telemetry_path: Option<String>,
    telemetry_enabled: bool,
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
            current_sector: None,
            telemetry_path: None,
            telemetry_enabled: false,
        }
    }

    pub fn with_telemetry(mut self, path: String) -> Self {
        self.telemetry_path = Some(path);
        self
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
                    return Err(RuntimeError::UndefinedVar { name: name.clone() });
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
                let resolved = self.resolve_probe(name);
                if !self.probes.contains_key(&resolved) {
                    return Err(RuntimeError::UndefinedProbe { name: name.clone() });
                }
                let name = resolved;
                let mut evaluated = HashMap::new();
                for (k, v) in args {
                    evaluated.insert(k.clone(), self.eval_expr(v)?);
                }
                self.log_telemetry("probe", &name);
                println!("[probe:{name}] {evaluated:?}");
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
        match expr {
            IrExpr::Int(n) => Ok(Value::Int(*n)),
            IrExpr::Float(n) => Ok(Value::Float(*n)),
            IrExpr::Str(s) => Ok(Value::Str(s.clone())),
            IrExpr::Bool(b) => Ok(Value::Bool(*b)),
            IrExpr::None => Ok(Value::None),
            IrExpr::Some(inner) => Ok(Value::Some(Box::new(self.eval_expr(inner)?))),
            IrExpr::Var(name) => self
                .env
                .get(name)
                .cloned()
                .ok_or(RuntimeError::UndefinedVar { name: name.clone() }),
            IrExpr::Unary { op, operand } => {
                let v = self.eval_expr(operand)?;
                match op {
                    UnaryOp::Not => Ok(Value::Bool(!is_truthy(&v))),
                }
            }
            IrExpr::Binary { left, op, right } => {
                let l = self.eval_expr(left)?;
                let r = self.eval_expr(right)?;
                eval_binary(*op, l, r)
            }
            IrExpr::Call { name, args } => {
                if is_builtin(name) {
                    return eval_builtin(name, args, self);
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
            IrExpr::List(items) => {
                let mut vals = Vec::new();
                for item in items {
                    vals.push(self.eval_expr(item)?);
                }
                Ok(Value::List(vals))
            }
            IrExpr::Map(entries) => {
                let mut map = HashMap::new();
                for (k, v) in entries {
                    let key = value_to_string(&self.eval_expr(k)?)?;
                    map.insert(key, self.eval_expr(v)?);
                }
                Ok(Value::Map(map))
            }
            IrExpr::Struct { name, fields } => {
                let mut map = HashMap::new();
                for (k, v) in fields {
                    map.insert(k.clone(), self.eval_expr(v)?);
                }
                Ok(Value::Struct {
                    name: name.clone(),
                    fields: map,
                })
            }
            IrExpr::FieldAccess { object, field } => {
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
        }
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

fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        "print" | "len" | "push" | "str_to_int" | "int_to_str"
    )
}

fn eval_builtin(
    name: &str,
    args: &[IrExpr],
    rt: &mut Runtime,
) -> Result<Value, RuntimeError> {
    match name {
        "print" => {
            if let Some(arg) = args.first() {
                let v = rt.eval_expr(arg)?;
                println!("{}", value_to_string(&v)?);
            }
            Ok(Value::None)
        }
        "len" => {
            let v = rt.eval_expr(args.first().ok_or(RuntimeError::Error {
                message: "len requires argument".into(),
            })?)?;
            match v {
                Value::List(items) => Ok(Value::Int(items.len() as i64)),
                Value::Str(s) => Ok(Value::Int(s.len() as i64)),
                _ => Err(RuntimeError::Error {
                    message: "len requires list or string".into(),
                }),
            }
        }
        "push" => {
            if args.len() != 2 {
                return Err(RuntimeError::Error {
                    message: "push requires exactly 2 arguments".into(),
                });
            }

            let list_name = match args.first() {
                Some(IrExpr::Var(name)) => name.clone(),
                _ => {
                    return Err(RuntimeError::Error {
                        message: "push requires a list variable as first argument".into(),
                    });
                }
            };

            let value = rt.eval_expr(
                args.get(1).ok_or(RuntimeError::Error {
                    message: "push requires a value as second argument".into(),
                })?,
            )?;

            match rt.env.get_mut(&list_name) {
                Some(Value::List(items)) => {
                    if let Some(existing) = items.first() {
                        if !values_same_type(existing, &value) {
                            return Err(RuntimeError::Error {
                                message: format!(
                                    "push value type mismatch for list `{list_name}`"
                                ),
                            });
                        }
                    }
                    items.push(value);
                    Ok(Value::None)
                }
                Some(_) => Err(RuntimeError::Error {
                    message: format!("`{list_name}` is not a list"),
                }),
                None => Err(RuntimeError::UndefinedVar { name: list_name }),
            }
        }
        "str_to_int" => {
            let v = rt.eval_expr(args.first().ok_or(RuntimeError::Error {
                message: "str_to_int requires argument".into(),
            })?)?;
            match v {
                Value::Str(s) => s.parse::<i64>().map(Value::Int).map_err(|_| RuntimeError::Error {
                    message: format!("invalid int: {s}"),
                }),
                _ => Err(RuntimeError::Error {
                    message: "str_to_int requires string".into(),
                }),
            }
        }
        "int_to_str" => {
            let v = rt.eval_expr(args.first().ok_or(RuntimeError::Error {
                message: "int_to_str requires argument".into(),
            })?)?;
            match v {
                Value::Int(n) => Ok(Value::Str(n.to_string())),
                _ => Err(RuntimeError::Error {
                    message: "int_to_str requires int".into(),
                }),
            }
        }
        _ => Err(RuntimeError::Error {
            message: format!("unknown builtin `{name}`"),
        }),
    }
}

fn eval_binary(op: BinaryOp, l: Value, r: Value) -> Result<Value, RuntimeError> {
    match op {
        BinaryOp::Plus => match (l, r) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
            (Value::Str(mut a), Value::Str(b)) => {
                a.push_str(&b);
                Ok(Value::Str(a))
            }
            _ => Err(RuntimeError::Error {
                message: "invalid plus operands".into(),
            }),
        },
        BinaryOp::Minus => match (l, r) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a - b)),
            _ => Err(RuntimeError::Error {
                message: "invalid minus operands".into(),
            }),
        },
        BinaryOp::Times => match (l, r) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a * b)),
            _ => Err(RuntimeError::Error {
                message: "invalid times operands".into(),
            }),
        },
        BinaryOp::Div => match (l, r) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a / b)),
            _ => Err(RuntimeError::Error {
                message: "invalid div operands".into(),
            }),
        },
        BinaryOp::Mod => match (l, r) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a % b)),
            _ => Err(RuntimeError::Error {
                message: "invalid mod operands".into(),
            }),
        },
        BinaryOp::Eq => Ok(Value::Bool(values_equal(&l, &r))),
        BinaryOp::Ne => Ok(Value::Bool(!values_equal(&l, &r))),
        BinaryOp::Lt => match (l, r) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a < b)),
            _ => Err(RuntimeError::Error {
                message: "invalid lt operands".into(),
            }),
        },
        BinaryOp::Gt => match (l, r) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a > b)),
            _ => Err(RuntimeError::Error {
                message: "invalid gt operands".into(),
            }),
        },
        BinaryOp::Le => match (l, r) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a <= b)),
            _ => Err(RuntimeError::Error {
                message: "invalid le operands".into(),
            }),
        },
        BinaryOp::Ge => match (l, r) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a >= b)),
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

fn values_same_type(a: &Value, b: &Value) -> bool {
    matches!(
        (a, b),
        (Value::Int(_), Value::Int(_))
            | (Value::Float(_), Value::Float(_))
            | (Value::Bool(_), Value::Bool(_))
            | (Value::Str(_), Value::Str(_))
            | (Value::None, Value::None)
            | (Value::List(_), Value::List(_))
            | (Value::Map(_), Value::Map(_))
            | (Value::Some(_), Value::Some(_))
            | (Value::Struct { .. }, Value::Struct { .. })
    )
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => x == y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::None, Value::None) => true,
        _ => false,
    }
}

fn value_to_string(v: &Value) -> Result<String, RuntimeError> {
    match v {
        Value::Str(s) => Ok(s.clone()),
        Value::Int(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Float(n) => Ok(n.to_string()),
        _ => Err(RuntimeError::Error {
            message: "cannot convert value to string".into(),
        }),
    }
}