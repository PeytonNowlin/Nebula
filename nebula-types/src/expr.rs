use nebula_ast::{BinaryOp, Expr, Type, UnaryOp};

use crate::checker::Checker;
use crate::error::TypeError;
use crate::scope::Scope;
use crate::util::types_equal;

impl Checker {
    pub(super) fn check_expr(
        &mut self,
        expr: &Expr,
        scope: &mut Scope,
        errors: &mut Vec<TypeError>,
    ) -> Type {
        self.check_expr_inner(expr, scope, errors, None)
    }

    pub(super) fn check_expr_inner(
        &self,
        expr: &Expr,
        scope: &mut Scope,
        errors: &mut Vec<TypeError>,
        expected: Option<&Type>,
    ) -> Type {
        match expr {
            Expr::Int(_) => Type::Int,
            Expr::Float(_) => Type::Float,
            Expr::Str(_) => Type::Str,
            Expr::Bool(_) => Type::Bool,
            Expr::None => Type::NoneValue,
            Expr::Some(inner) => {
                let inner_expected = match expected {
                    Some(Type::Option(inner)) => Some(inner.as_ref()),
                    _ => None,
                };
                let inner_ty = self.check_expr_inner(&inner.node, scope, errors, inner_expected);
                Type::Option(Box::new(inner_ty))
            }
            Expr::Ident(name) => match scope.get(&name.node) {
                Some((ty, _)) => ty.clone(),
                None => {
                    errors.push(TypeError::UndefinedIdent {
                        name: name.node.clone(),
                        span: name.span.clone(),
                    });
                    Type::Void
                }
            },
            Expr::Unary {
                op: UnaryOp::Not,
                operand,
            } => {
                let ty = self.check_expr_inner(&operand.node, scope, errors, None);
                if ty != Type::Bool {
                    errors.push(TypeError::Mismatch {
                        expected: "Bool".into(),
                        found: ty.display(),
                        span: operand.span.clone(),
                    });
                }
                Type::Bool
            }
            Expr::Binary { left, op, right } => {
                let lty = self.check_expr_inner(&left.node, scope, errors, None);
                let rty = self.check_expr_inner(&right.node, scope, errors, None);
                match op {
                    BinaryOp::Plus => {
                        if lty == Type::Str && rty == Type::Str {
                            Type::Str
                        } else if lty == Type::Int && rty == Type::Int {
                            Type::Int
                        } else if lty == Type::Float && rty == Type::Float {
                            Type::Float
                        } else {
                            errors.push(TypeError::Mismatch {
                                expected: "matching Int, Float, or Str operands".into(),
                                found: format!("{} and {}", lty.display(), rty.display()),
                                span: left.span.clone(),
                            });
                            Type::Int
                        }
                    }
                    BinaryOp::Minus | BinaryOp::Times | BinaryOp::Div | BinaryOp::Mod => {
                        if lty == Type::Int && rty == Type::Int {
                            Type::Int
                        } else if lty == Type::Float && rty == Type::Float {
                            Type::Float
                        } else {
                            errors.push(TypeError::Mismatch {
                                expected: "matching Int or Float operands".into(),
                                found: format!("{} and {}", lty.display(), rty.display()),
                                span: left.span.clone(),
                            });
                            Type::Int
                        }
                    }
                    BinaryOp::Eq | BinaryOp::Ne => {
                        if !types_equal(&lty, &rty) {
                            errors.push(TypeError::Mismatch {
                                expected: lty.display(),
                                found: rty.display(),
                                span: right.span.clone(),
                            });
                        }
                        Type::Bool
                    }
                    BinaryOp::Lt | BinaryOp::Gt | BinaryOp::Le | BinaryOp::Ge => {
                        let ordered = (lty == Type::Int && rty == Type::Int)
                            || (lty == Type::Float && rty == Type::Float);
                        if !ordered {
                            errors.push(TypeError::Mismatch {
                                expected: "matching Int or Float operands".into(),
                                found: format!("{} and {}", lty.display(), rty.display()),
                                span: left.span.clone(),
                            });
                        }
                        Type::Bool
                    }
                    BinaryOp::And | BinaryOp::Or => {
                        if lty != Type::Bool || rty != Type::Bool {
                            errors.push(TypeError::Mismatch {
                                expected: "Bool operands".into(),
                                found: format!("{} and {}", lty.display(), rty.display()),
                                span: left.span.clone(),
                            });
                        }
                        Type::Bool
                    }
                }
            }
            Expr::ProbeCall { name, args } => {
                if let Some(resolved) = self.resolve_probe(&name.node) {
                    if let Some(probe) = self.probes.get(&resolved).cloned() {
                        return self.check_probe_call(
                            &probe,
                            args,
                            name.span.clone(),
                            scope,
                            errors,
                        );
                    }
                }
                errors.push(TypeError::UndefinedProbe {
                    name: name.node.clone(),
                    span: name.span.clone(),
                });
                Type::Void
            }
            Expr::Call { callee, args } => {
                let name = &callee.node;
                if let Some(ret) = self.check_builtin_call(name, args, scope, errors) {
                    return ret;
                }
                let resolved = self.resolve_fn(name);
                if let Some(key) = resolved {
                    if let Some(fn_info) = self.functions.get(&key).cloned() {
                        if fn_info.params.len() != args.len() {
                            errors.push(TypeError::Mismatch {
                                expected: format!("{} arguments", fn_info.params.len()),
                                found: format!("{} arguments", args.len()),
                                span: callee.span.clone(),
                            });
                        }
                        for (param, arg) in fn_info.params.iter().zip(args.iter()) {
                            let arg_ty =
                                self.check_expr_inner(&arg.node, scope, errors, Some(&param.1));
                            if !types_equal(&param.1, &arg_ty) {
                                errors.push(TypeError::Mismatch {
                                    expected: param.1.display(),
                                    found: arg_ty.display(),
                                    span: arg.span.clone(),
                                });
                            }
                        }
                        return self.resolve_type(&fn_info.return_type);
                    }
                }
                errors.push(TypeError::UndefinedFn {
                    name: name.clone(),
                    span: callee.span.clone(),
                });
                Type::Void
            }
            Expr::List(items) => {
                if items.is_empty() {
                    if let Some(Type::List(inner)) = expected {
                        return Type::List(Box::new((**inner).clone()));
                    }
                    return Type::List(Box::new(Type::Int));
                }
                let item_expected = match expected {
                    Some(Type::List(inner)) => Some(inner.as_ref()),
                    _ => None,
                };
                let first = self.check_expr_inner(&items[0].node, scope, errors, item_expected);
                for item in &items[1..] {
                    let ty = self.check_expr_inner(&item.node, scope, errors, item_expected);
                    if !types_equal(&first, &ty) {
                        errors.push(TypeError::Mismatch {
                            expected: first.display(),
                            found: ty.display(),
                            span: item.span.clone(),
                        });
                    }
                }
                Type::List(Box::new(first))
            }
            Expr::Map(entries) => {
                if entries.is_empty() {
                    if let Some(Type::Map(key, value)) = expected {
                        return Type::Map(Box::new((**key).clone()), Box::new((**value).clone()));
                    }
                    return Type::Map(Box::new(Type::Str), Box::new(Type::Int));
                }
                let (key_expected, val_expected) = match expected {
                    Some(Type::Map(k, v)) => (Some(k.as_ref()), Some(v.as_ref())),
                    _ => (None, None),
                };
                let key_ty =
                    self.check_expr_inner(&entries[0].node.key.node, scope, errors, key_expected);
                let val_ty =
                    self.check_expr_inner(&entries[0].node.value.node, scope, errors, val_expected);
                for entry in &entries[1..] {
                    let k =
                        self.check_expr_inner(&entry.node.key.node, scope, errors, Some(&key_ty));
                    if !types_equal(&key_ty, &k) {
                        errors.push(TypeError::Mismatch {
                            expected: key_ty.display(),
                            found: k.display(),
                            span: entry.node.key.span.clone(),
                        });
                    }
                    let v =
                        self.check_expr_inner(&entry.node.value.node, scope, errors, Some(&val_ty));
                    if !types_equal(&val_ty, &v) {
                        errors.push(TypeError::Mismatch {
                            expected: val_ty.display(),
                            found: v.display(),
                            span: entry.node.value.span.clone(),
                        });
                    }
                }
                Type::Map(Box::new(key_ty), Box::new(val_ty))
            }
            Expr::StructLit { name, fields } => {
                let resolved = self.resolve_struct(&name.node);
                if let Some(key) = resolved.and_then(|k| self.structs.get(&k).cloned()) {
                    let info = key;
                    for field in fields {
                        if let Some(field_ty) = info.fields.get(&field.node.name.node) {
                            let actual = self.check_expr_inner(
                                &field.node.value.node,
                                scope,
                                errors,
                                Some(field_ty),
                            );
                            if !types_equal(field_ty, &actual) {
                                errors.push(TypeError::Mismatch {
                                    expected: field_ty.display(),
                                    found: actual.display(),
                                    span: field.node.value.span.clone(),
                                });
                            }
                        } else {
                            errors.push(TypeError::UnknownField {
                                struct_name: name.node.clone(),
                                field: field.node.name.node.clone(),
                                span: field.node.name.span.clone(),
                            });
                        }
                    }
                    Type::Named(info.qualified_name.clone())
                } else {
                    errors.push(TypeError::UndefinedStruct {
                        name: name.node.clone(),
                        span: name.span.clone(),
                    });
                    Type::Void
                }
            }
            Expr::FieldAccess { object, field } => {
                if let Expr::Ident(name) = &object.node {
                    if scope.get(&name.node).is_none() {
                        if let Some(key) = self.resolve_struct(&name.node) {
                            if let Some(info) = self.structs.get(&key) {
                                if let Some(ty) = info.fields.get(&field.node) {
                                    return ty.clone();
                                }
                                errors.push(TypeError::UnknownField {
                                    struct_name: name.node.clone(),
                                    field: field.node.clone(),
                                    span: field.span.clone(),
                                });
                                return Type::Void;
                            }
                        }
                    }
                }

                let obj_ty = self.check_expr_inner(&object.node, scope, errors, None);
                if let Type::Named(ref struct_name) = obj_ty {
                    if let Some(info) = self.struct_info_for_named(struct_name) {
                        if let Some(ty) = info.fields.get(&field.node) {
                            return ty.clone();
                        }
                        errors.push(TypeError::UnknownField {
                            struct_name: info.qualified_name.clone(),
                            field: field.node.clone(),
                            span: field.span.clone(),
                        });
                        return Type::Void;
                    }
                }

                errors.push(TypeError::Mismatch {
                    expected: "struct value".into(),
                    found: obj_ty.display(),
                    span: object.span.clone(),
                });
                Type::Void
            }
        }
    }
}
