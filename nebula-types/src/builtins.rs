use std::collections::HashMap;

use nebula_ast::{Expr, Spanned, Type};
use nebula_builtins::{manifest, BuiltinCheckerKind};

use crate::checker::Checker;
use crate::error::TypeError;
use crate::program::FnInfo;
use crate::scope::Scope;
use crate::util::types_equal;

pub(crate) fn builtin_functions() -> HashMap<String, FnInfo> {
    let mut functions = HashMap::new();
    for (name, params, return_type) in manifest().simple_signatures() {
        functions.insert(
            name.to_string(),
            FnInfo {
                sector: String::new(),
                name: name.to_string(),
                qualified_name: name.to_string(),
                params,
                return_type,
            },
        );
    }
    functions
}

impl Checker {
    pub(super) fn check_builtin_call(
        &self,
        name: &str,
        args: &[Spanned<Expr>],
        scope: &mut Scope,
        errors: &mut Vec<TypeError>,
    ) -> Option<Type> {
        let builtin = manifest().get(name)?;
        match builtin.checker {
            BuiltinCheckerKind::Simple => None,
            BuiltinCheckerKind::Len => {
                if args.len() != 1 {
                    errors.push(TypeError::Mismatch {
                        expected: "1 argument".into(),
                        found: format!("{} arguments", args.len()),
                        span: args.first().map(|a| a.span.clone()).unwrap_or(0..0),
                    });
                    return Some(Type::Int);
                }

                let arg_ty = self.check_expr_inner(&args[0].node, scope, errors, None);
                match arg_ty {
                    Type::List(_) | Type::Map(_, _) | Type::Str => {}
                    _ => {
                        errors.push(TypeError::Mismatch {
                            expected: "List<T>, Map<K, V>, or Str".into(),
                            found: arg_ty.display(),
                            span: args[0].span.clone(),
                        });
                    }
                }
                Some(Type::Int)
            }
            BuiltinCheckerKind::Push => {
                if args.len() != 2 {
                    errors.push(TypeError::Mismatch {
                        expected: "2 arguments".into(),
                        found: format!("{} arguments", args.len()),
                        span: args.first().map(|a| a.span.clone()).unwrap_or(0..0),
                    });
                    return Some(Type::Void);
                }

                if !matches!(args[0].node, Expr::Ident(_)) {
                    errors.push(TypeError::Mismatch {
                        expected: "list variable as first argument".into(),
                        found: "expression".into(),
                        span: args[0].span.clone(),
                    });
                }

                let list_ty = self.check_expr_inner(&args[0].node, scope, errors, None);
                let value_expected = match &list_ty {
                    Type::List(inner) => Some(inner.as_ref()),
                    _ => None,
                };
                let value_ty =
                    self.check_expr_inner(&args[1].node, scope, errors, value_expected);

                match list_ty {
                    Type::List(inner) => {
                        if !types_equal(&inner, &value_ty) {
                            errors.push(TypeError::Mismatch {
                                expected: inner.display(),
                                found: value_ty.display(),
                                span: args[1].span.clone(),
                            });
                        }
                    }
                    _ => {
                        errors.push(TypeError::Mismatch {
                            expected: "List<T>".into(),
                            found: list_ty.display(),
                            span: args[0].span.clone(),
                        });
                    }
                }

                Some(Type::Void)
            }
            BuiltinCheckerKind::At => {
                if args.len() != 2 {
                    errors.push(TypeError::Mismatch {
                        expected: "2 arguments".into(),
                        found: format!("{} arguments", args.len()),
                        span: args.first().map(|a| a.span.clone()).unwrap_or(0..0),
                    });
                    return Some(Type::Void);
                }
                let list_ty = self.check_expr_inner(&args[0].node, scope, errors, None);
                let idx_ty = self.check_expr_inner(&args[1].node, scope, errors, Some(&Type::Int));
                if idx_ty != Type::Int {
                    errors.push(TypeError::Mismatch {
                        expected: "Int".into(),
                        found: idx_ty.display(),
                        span: args[1].span.clone(),
                    });
                }
                match list_ty {
                    Type::List(inner) => Some(*inner),
                    _ => {
                        errors.push(TypeError::Mismatch {
                            expected: "List<T>".into(),
                            found: list_ty.display(),
                            span: args[0].span.clone(),
                        });
                        Some(Type::Void)
                    }
                }
            }
            BuiltinCheckerKind::Get | BuiltinCheckerKind::Has => {
                if args.len() != 2 {
                    errors.push(TypeError::Mismatch {
                        expected: "2 arguments".into(),
                        found: format!("{} arguments", args.len()),
                        span: args.first().map(|a| a.span.clone()).unwrap_or(0..0),
                    });
                    return Some(if builtin.checker == BuiltinCheckerKind::Has {
                        Type::Bool
                    } else {
                        Type::Void
                    });
                }
                let map_ty = self.check_expr_inner(&args[0].node, scope, errors, None);
                match map_ty {
                    Type::Map(key, value) => {
                        let key_ty =
                            self.check_expr_inner(&args[1].node, scope, errors, Some(&key));
                        if !types_equal(&key, &key_ty) {
                            errors.push(TypeError::Mismatch {
                                expected: key.display(),
                                found: key_ty.display(),
                                span: args[1].span.clone(),
                            });
                        }
                        Some(if builtin.checker == BuiltinCheckerKind::Has {
                            Type::Bool
                        } else {
                            *value
                        })
                    }
                    _ => {
                        self.check_expr_inner(&args[1].node, scope, errors, None);
                        errors.push(TypeError::Mismatch {
                            expected: "Map<K, V>".into(),
                            found: map_ty.display(),
                            span: args[0].span.clone(),
                        });
                        Some(if builtin.checker == BuiltinCheckerKind::Has {
                            Type::Bool
                        } else {
                            Type::Void
                        })
                    }
                }
            }
        }
    }
}