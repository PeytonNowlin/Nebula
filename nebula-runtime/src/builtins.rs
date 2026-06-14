use nebula_ast::Span;
use nebula_builtins::{manifest, BuiltinCheckerKind};
use nebula_ir::{IrExpr, IrExprKind};

use crate::{Runtime, RuntimeError, Value};

/// Substring matched by [`missing_handler_error`] and asserted absent in sync tests.
pub(crate) const MISSING_HANDLER_MARKER: &str = "listed in builtins.toml but has no runtime handler";

/// Manifest builtin names that lack a runtime handler.
pub fn missing_runtime_handlers() -> Vec<String> {
    manifest()
        .names()
        .filter(|name| !has_runtime_handler(name))
        .map(str::to_string)
        .collect()
}

fn has_runtime_handler(name: &str) -> bool {
    let Some(def) = manifest().get(name) else {
        return false;
    };
    match def.checker {
        BuiltinCheckerKind::Simple => simple_builtin_handled(name),
        BuiltinCheckerKind::Len
        | BuiltinCheckerKind::Push
        | BuiltinCheckerKind::At
        | BuiltinCheckerKind::Get
        | BuiltinCheckerKind::Has
        | BuiltinCheckerKind::Insert
        | BuiltinCheckerKind::Split
        | BuiltinCheckerKind::Join => true,
    }
}

fn simple_builtin_handled(name: &str) -> bool {
    matches!(
        name,
        "print"
            | "str_to_int"
            | "int_to_str"
            | "str_to_float"
            | "float_to_str"
            | "int_to_float"
            | "float_to_int"
            | "substr"
            | "contains"
            | "index_of"
            | "starts_with"
            | "ends_with"
            | "to_upper"
            | "to_lower"
            | "trim"
            | "replace"
            | "abs"
            | "min"
            | "max"
    )
}

fn missing_handler_error(name: &str) -> RuntimeError {
    RuntimeError::Error {
        message: format!("builtin `{name}` is {MISSING_HANDLER_MARKER}"),
    }
}

impl Runtime {
    pub(super) fn eval_builtin(
        &mut self,
        name: &str,
        args: &[IrExpr],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let Some(def) = manifest().get(name) else {
            return Err(RuntimeError::Error {
                message: format!("unknown builtin `{name}`"),
            });
        };

        let value = match def.checker {
            BuiltinCheckerKind::Simple => self.eval_simple_builtin(name, args)?,
            BuiltinCheckerKind::Len => self.eval_len(args, span)?,
            BuiltinCheckerKind::Push => self.eval_push(args, span)?,
            BuiltinCheckerKind::At => self.eval_at(args, span)?,
            BuiltinCheckerKind::Get => self.eval_get(args, span)?,
            BuiltinCheckerKind::Has => self.eval_has(args, span)?,
            BuiltinCheckerKind::Insert => self.eval_insert(args, span)?,
            BuiltinCheckerKind::Split => self.eval_split(args)?,
            BuiltinCheckerKind::Join => self.eval_join(args)?,
        };
        self.finish_value(value)
    }

    fn eval_simple_builtin(
        &mut self,
        name: &str,
        args: &[IrExpr],
    ) -> Result<Value, RuntimeError> {
        match name {
            "print" => self.eval_print(args),
            "str_to_int" => self.eval_str_to_int(args),
            "int_to_str" => self.eval_int_to_str(args),
            "str_to_float" => self.eval_str_to_float(args),
            "float_to_str" => self.eval_float_to_str(args),
            "int_to_float" => self.eval_int_to_float(args),
            "float_to_int" => self.eval_float_to_int(args),
            "substr" => self.eval_substr(args),
            "contains" => self.eval_contains(args),
            "index_of" => self.eval_index_of(args),
            "starts_with" => self.eval_starts_with(args),
            "ends_with" => self.eval_ends_with(args),
            "to_upper" => self.eval_to_upper(args),
            "to_lower" => self.eval_to_lower(args),
            "trim" => self.eval_trim(args),
            "replace" => self.eval_replace(args),
            "abs" => self.eval_abs(args),
            "min" => self.eval_min(args),
            "max" => self.eval_max(args),
            _ => Err(missing_handler_error(name)),
        }
    }

    /// Evaluate `args[index]` and require it to be a `Str`.
    fn str_arg(&mut self, args: &[IrExpr], index: usize, fname: &str) -> Result<String, RuntimeError> {
        let expr = args.get(index).ok_or_else(|| RuntimeError::Error {
            message: format!("{fname} requires {} arguments", index + 1),
        })?;
        match self.eval_expr(expr)? {
            Value::Str(s) => Ok(s),
            _ => Err(RuntimeError::Error {
                message: format!("{fname} requires Str arguments"),
            }),
        }
    }

    /// Evaluate `args[index]` and require it to be an `Int`.
    fn int_arg(&mut self, args: &[IrExpr], index: usize, fname: &str) -> Result<i64, RuntimeError> {
        let expr = args.get(index).ok_or_else(|| RuntimeError::Error {
            message: format!("{fname} requires {} arguments", index + 1),
        })?;
        match self.eval_expr(expr)? {
            Value::Int(n) => Ok(n),
            _ => Err(RuntimeError::Error {
                message: format!("{fname} requires an Int argument"),
            }),
        }
    }

    /// substr(s, start, end): code-point slice with start/end clamped to
    /// [0, len] and start <= end. Negative bounds clamp to 0, mirroring the
    /// lenient Python `chars[start:end]` lowering.
    fn eval_substr(&mut self, args: &[IrExpr]) -> Result<Value, RuntimeError> {
        let s = self.str_arg(args, 0, "substr")?;
        let start = self.int_arg(args, 1, "substr")?;
        let end = self.int_arg(args, 2, "substr")?;
        let chars: Vec<char> = s.chars().collect();
        let n = chars.len() as i64;
        let start = start.clamp(0, n);
        let end = end.clamp(start, n);
        Ok(Value::Str(
            chars[start as usize..end as usize].iter().collect(),
        ))
    }

    fn eval_contains(&mut self, args: &[IrExpr]) -> Result<Value, RuntimeError> {
        let s = self.str_arg(args, 0, "contains")?;
        let needle = self.str_arg(args, 1, "contains")?;
        Ok(Value::Bool(s.contains(&needle)))
    }

    /// index_of(s, needle): 0-based code-point index of the first occurrence,
    /// or -1 when absent.
    fn eval_index_of(&mut self, args: &[IrExpr]) -> Result<Value, RuntimeError> {
        let s = self.str_arg(args, 0, "index_of")?;
        let needle = self.str_arg(args, 1, "index_of")?;
        let idx = match s.find(&needle) {
            Some(byte) => s[..byte].chars().count() as i64,
            None => -1,
        };
        Ok(Value::Int(idx))
    }

    fn eval_starts_with(&mut self, args: &[IrExpr]) -> Result<Value, RuntimeError> {
        let s = self.str_arg(args, 0, "starts_with")?;
        let prefix = self.str_arg(args, 1, "starts_with")?;
        Ok(Value::Bool(s.starts_with(&prefix)))
    }

    fn eval_ends_with(&mut self, args: &[IrExpr]) -> Result<Value, RuntimeError> {
        let s = self.str_arg(args, 0, "ends_with")?;
        let suffix = self.str_arg(args, 1, "ends_with")?;
        Ok(Value::Bool(s.ends_with(&suffix)))
    }

    fn eval_to_upper(&mut self, args: &[IrExpr]) -> Result<Value, RuntimeError> {
        let s = self.str_arg(args, 0, "to_upper")?;
        Ok(Value::Str(s.to_uppercase()))
    }

    fn eval_to_lower(&mut self, args: &[IrExpr]) -> Result<Value, RuntimeError> {
        let s = self.str_arg(args, 0, "to_lower")?;
        Ok(Value::Str(s.to_lowercase()))
    }

    fn eval_trim(&mut self, args: &[IrExpr]) -> Result<Value, RuntimeError> {
        let s = self.str_arg(args, 0, "trim")?;
        Ok(Value::Str(s.trim().to_string()))
    }

    /// abs(n): magnitude. `abs(i64::MIN)` overflows, so it is checked (NEB-R007)
    /// to match the Python backend's i64 bounds check.
    fn eval_abs(&mut self, args: &[IrExpr]) -> Result<Value, RuntimeError> {
        let n = self.int_arg(args, 0, "abs")?;
        let span = args
            .first()
            .map(|expr| expr.span.clone())
            .unwrap_or_default();
        n.checked_abs()
            .map(Value::Int)
            .ok_or(RuntimeError::IntegerOverflow {
                op: "abs".into(),
                span,
            })
    }

    fn eval_min(&mut self, args: &[IrExpr]) -> Result<Value, RuntimeError> {
        let a = self.int_arg(args, 0, "min")?;
        let b = self.int_arg(args, 1, "min")?;
        Ok(Value::Int(a.min(b)))
    }

    fn eval_max(&mut self, args: &[IrExpr]) -> Result<Value, RuntimeError> {
        let a = self.int_arg(args, 0, "max")?;
        let b = self.int_arg(args, 1, "max")?;
        Ok(Value::Int(a.max(b)))
    }

    /// split(s, sep) -> List<Str>. Empty separator is a runtime error so the
    /// interpreter and Python (which raises on `"".split("")`) stay in lockstep.
    fn eval_split(&mut self, args: &[IrExpr]) -> Result<Value, RuntimeError> {
        let s = self.str_arg(args, 0, "split")?;
        let sep = self.str_arg(args, 1, "split")?;
        if sep.is_empty() {
            return Err(RuntimeError::Error {
                message: "split separator must be non-empty".into(),
            });
        }
        Ok(Value::List(
            s.split(&sep).map(|part| Value::Str(part.to_string())).collect(),
        ))
    }

    /// join(parts: List<Str>, sep) -> Str.
    fn eval_join(&mut self, args: &[IrExpr]) -> Result<Value, RuntimeError> {
        let parts = match self.eval_expr(args.first().ok_or_else(|| RuntimeError::Error {
            message: "join requires 2 arguments".into(),
        })?)? {
            Value::List(items) => items,
            _ => {
                return Err(RuntimeError::Error {
                    message: "join requires a list as first argument".into(),
                })
            }
        };
        let sep = self.str_arg(args, 1, "join")?;
        let mut strs = Vec::with_capacity(parts.len());
        for item in parts {
            match item {
                Value::Str(s) => strs.push(s),
                _ => {
                    return Err(RuntimeError::Error {
                        message: "join requires a List<Str>".into(),
                    })
                }
            }
        }
        Ok(Value::Str(strs.join(&sep)))
    }

    fn eval_replace(&mut self, args: &[IrExpr]) -> Result<Value, RuntimeError> {
        let s = self.str_arg(args, 0, "replace")?;
        let from = self.str_arg(args, 1, "replace")?;
        let to = self.str_arg(args, 2, "replace")?;
        Ok(Value::Str(s.replace(&from, &to)))
    }

    fn eval_print(&mut self, args: &[IrExpr]) -> Result<Value, RuntimeError> {
        if let Some(arg) = args.first() {
            let v = self.eval_expr(arg)?;
            let line = super::value_to_string(&v)?;
            if self.capture_print {
                self.printed.push(line);
            } else {
                println!("{line}");
            }
        }
        Ok(Value::None)
    }

    fn eval_len(&mut self, args: &[IrExpr], _span: Span) -> Result<Value, RuntimeError> {
        let v = self.eval_expr(args.first().ok_or(RuntimeError::Error {
            message: "len requires argument".into(),
        })?)?;
        match v {
            Value::List(items) => Ok(Value::Int(items.len() as i64)),
            Value::Map(map) => Ok(Value::Int(map.len() as i64)),
            // Count Unicode scalar values, not bytes, to match the Python
            // backend's `len()` (NEB string length is in code points).
            Value::Str(s) => Ok(Value::Int(s.chars().count() as i64)),
            _ => Err(RuntimeError::Error {
                message: "len requires list, map, or string".into(),
            }),
        }
    }

    fn eval_push(&mut self, args: &[IrExpr], span: Span) -> Result<Value, RuntimeError> {
        if args.len() != 2 {
            return Err(RuntimeError::Error {
                message: "push requires exactly 2 arguments".into(),
            });
        }

        let list_name = match args.first().map(|arg| &arg.node) {
            Some(IrExprKind::Var(name)) => name.clone(),
            _ => {
                return Err(RuntimeError::Error {
                    message: "push requires a list variable as first argument".into(),
                });
            }
        };

        let value = self.eval_expr(
            args.get(1).ok_or(RuntimeError::Error {
                message: "push requires a value as second argument".into(),
            })?,
        )?;

        let before = self.env_footprint(&list_name);
        match self.env.get_mut(&list_name) {
            Some(Value::List(items)) => {
                if let Some(existing) = items.first() {
                    if !values_same_type(existing, &value) {
                        return Err(RuntimeError::Error {
                            message: format!("push value type mismatch for list `{list_name}`"),
                        });
                    }
                }
                items.push(value);
                self.record_env_footprint_change(&list_name, before)?;
                Ok(Value::None)
            }
            Some(_) => Err(RuntimeError::Error {
                message: format!("`{list_name}` is not a list"),
            }),
            None => Err(RuntimeError::UndefinedVar {
                name: list_name,
                span: span.clone(),
            }),
        }
    }

    fn eval_at(&mut self, args: &[IrExpr], span: Span) -> Result<Value, RuntimeError> {
        if args.len() != 2 {
            return Err(RuntimeError::Error {
                message: "at requires exactly 2 arguments".into(),
            });
        }
        let list = self.eval_expr(&args[0])?;
        let index = match self.eval_expr(&args[1])? {
            Value::Int(i) => i,
            _ => {
                return Err(RuntimeError::Error {
                    message: "at index must be an Int".into(),
                })
            }
        };
        match list {
            Value::List(items) => {
                if index < 0 || index as usize >= items.len() {
                    return Err(RuntimeError::IndexOutOfBounds {
                        index,
                        len: items.len(),
                        span: span.clone(),
                    });
                }
                Ok(items[index as usize].clone())
            }
            _ => Err(RuntimeError::Error {
                message: "at requires a list as first argument".into(),
            }),
        }
    }

    fn eval_get(&mut self, args: &[IrExpr], span: Span) -> Result<Value, RuntimeError> {
        if args.len() != 2 {
            return Err(RuntimeError::Error {
                message: "get requires exactly 2 arguments".into(),
            });
        }
        let map = self.eval_expr(&args[0])?;
        let key = super::value_to_string(&self.eval_expr(&args[1])?)?;
        match map {
            Value::Map(entries) => entries
                .get(&key)
                .cloned()
                .ok_or(RuntimeError::KeyNotFound {
                    key,
                    span: span.clone(),
                }),
            _ => Err(RuntimeError::Error {
                message: "get requires a map as first argument".into(),
            }),
        }
    }

    fn eval_has(&mut self, args: &[IrExpr], _span: Span) -> Result<Value, RuntimeError> {
        if args.len() != 2 {
            return Err(RuntimeError::Error {
                message: "has requires exactly 2 arguments".into(),
            });
        }
        let map = self.eval_expr(&args[0])?;
        let key = super::value_to_string(&self.eval_expr(&args[1])?)?;
        match map {
            Value::Map(entries) => Ok(Value::Bool(entries.contains_key(&key))),
            _ => Err(RuntimeError::Error {
                message: "has requires a map as first argument".into(),
            }),
        }
    }

    fn eval_insert(&mut self, args: &[IrExpr], span: Span) -> Result<Value, RuntimeError> {
        if args.len() != 3 {
            return Err(RuntimeError::Error {
                message: "insert requires exactly 3 arguments".into(),
            });
        }
        // First argument must be a map variable so it is mutated in place,
        // mirroring `push` on lists.
        let map_name = match args.first().map(|arg| &arg.node) {
            Some(IrExprKind::Var(name)) => name.clone(),
            _ => {
                return Err(RuntimeError::Error {
                    message: "insert requires a map variable as first argument".into(),
                });
            }
        };
        let key = super::value_to_string(&self.eval_expr(&args[1])?)?;
        let value = self.eval_expr(&args[2])?;
        let before = self.env_footprint(&map_name);
        match self.env.get_mut(&map_name) {
            Some(Value::Map(entries)) => {
                entries.insert(key, value);
                self.record_env_footprint_change(&map_name, before)?;
                Ok(Value::None)
            }
            Some(_) => Err(RuntimeError::Error {
                message: format!("`{map_name}` is not a map"),
            }),
            None => Err(RuntimeError::UndefinedVar {
                name: map_name,
                span: span.clone(),
            }),
        }
    }

    fn eval_str_to_int(&mut self, args: &[IrExpr]) -> Result<Value, RuntimeError> {
        let v = self.eval_expr(args.first().ok_or(RuntimeError::Error {
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

    fn eval_int_to_str(&mut self, args: &[IrExpr]) -> Result<Value, RuntimeError> {
        let v = self.eval_expr(args.first().ok_or(RuntimeError::Error {
            message: "int_to_str requires argument".into(),
        })?)?;
        match v {
            Value::Int(n) => Ok(Value::Str(n.to_string())),
            _ => Err(RuntimeError::Error {
                message: "int_to_str requires int".into(),
            }),
        }
    }

    fn eval_float_to_str(&mut self, args: &[IrExpr]) -> Result<Value, RuntimeError> {
        let v = self.eval_expr(args.first().ok_or(RuntimeError::Error {
            message: "float_to_str requires argument".into(),
        })?)?;
        match v {
            Value::Float(n) => Ok(Value::Str(format_float(n))),
            _ => Err(RuntimeError::Error {
                message: "float_to_str requires float".into(),
            }),
        }
    }

    fn eval_str_to_float(&mut self, args: &[IrExpr]) -> Result<Value, RuntimeError> {
        let v = self.eval_expr(args.first().ok_or(RuntimeError::Error {
            message: "str_to_float requires argument".into(),
        })?)?;
        match v {
            Value::Str(s) => s.trim().parse::<f64>().map(Value::Float).map_err(|_| {
                RuntimeError::Error {
                    message: format!("invalid float: {s}"),
                }
            }),
            _ => Err(RuntimeError::Error {
                message: "str_to_float requires string".into(),
            }),
        }
    }

    fn eval_int_to_float(&mut self, args: &[IrExpr]) -> Result<Value, RuntimeError> {
        let v = self.eval_expr(args.first().ok_or(RuntimeError::Error {
            message: "int_to_float requires argument".into(),
        })?)?;
        match v {
            Value::Int(n) => Ok(Value::Float(n as f64)),
            _ => Err(RuntimeError::Error {
                message: "int_to_float requires int".into(),
            }),
        }
    }

    fn eval_float_to_int(&mut self, args: &[IrExpr]) -> Result<Value, RuntimeError> {
        let v = self.eval_expr(args.first().ok_or(RuntimeError::Error {
            message: "float_to_int requires argument".into(),
        })?)?;
        match v {
            // Truncate toward zero, matching Python int(float).
            Value::Float(n) => Ok(Value::Int(n.trunc() as i64)),
            _ => Err(RuntimeError::Error {
                message: "float_to_int requires float".into(),
            }),
        }
    }
}

/// Format an f64 the same way Python's `str(float)` does for the common cases:
/// integral values keep a trailing `.0`, and non-finite values use lowercase
/// `nan`/`inf`/`-inf`. Extreme magnitudes that Python renders in exponent form
/// are the one documented divergence.
pub(super) fn format_float(n: f64) -> String {
    if n.is_nan() {
        return "nan".into();
    }
    if n.is_infinite() {
        return if n < 0.0 { "-inf".into() } else { "inf".into() };
    }
    let s = format!("{n}");
    if s.contains(['.', 'e', 'E']) {
        s
    } else {
        format!("{s}.0")
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use nebula_builtins::manifest;
    use nebula_ir::{IrMission, IrProgram};

    use super::*;
    use crate::Runtime;

    fn empty_program() -> IrProgram {
        IrProgram {
            sectors: HashMap::new(),
            mission: IrMission {
                name: "main".into(),
                stmts: Vec::new(),
            },
            probes: HashMap::new(),
        }
    }

    #[test]
    fn simple_handlers_cover_manifest_simple_signatures() {
        use std::collections::HashSet;

        let manifest_simple: HashSet<_> = manifest()
            .simple_signatures()
            .into_iter()
            .map(|(name, _, _)| name)
            .collect();
        let handled: HashSet<_> = manifest()
            .names()
            .filter(|name| {
                manifest()
                    .get(name)
                    .is_some_and(|def| def.checker == BuiltinCheckerKind::Simple)
            })
            .filter(|name| simple_builtin_handled(name))
            .collect();
        assert_eq!(manifest_simple, handled);
    }

    #[test]
    fn dispatch_does_not_hit_missing_handler_arm() {
        let mut rt = Runtime::new(&empty_program());
        for name in manifest().names() {
            let result = rt.eval_builtin(name, &[], 0..0);
            if let Err(RuntimeError::Error { message }) = result {
                assert!(
                    !message.contains(MISSING_HANDLER_MARKER),
                    "builtin `{name}` missing runtime handler"
                );
            }
        }
    }
}