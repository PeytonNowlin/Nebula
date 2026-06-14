use std::collections::HashMap;

use nebula_ast::{Expr, Spanned, Type};
use nebula_builtins::{manifest, BuiltinCheckerKind};

use crate::checker::Checker;
use crate::error::TypeError;
use crate::program::FnInfo;
use crate::scope::Scope;
use crate::util::types_equal;

/// How one polymorphic builtin argument is validated.
#[derive(Debug, Clone, Copy)]
enum ArgRule {
    /// `len`: `List<_>`, `Map<_, _>`, or `Str`.
    LenTarget,
    /// `push` list operand: must be a list variable.
    ListMutTarget,
    /// `at` list operand: any `List<T>` expression.
    ListTarget,
    /// `get` / `has` / `insert` map operand: any `Map<K, V>` expression.
    MapTarget,
    /// Index operand: must be `Int`.
    Int,
    /// Element operand: must match `T` from a prior list rule.
    ListElem,
    /// Key operand: must match `K` from a prior map rule.
    MapKey,
    /// Value operand: must match `V` from a prior map rule.
    MapValue,
}

/// How the builtin's return type is determined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReturnRule {
    FixedInt,
    FixedVoid,
    FixedBool,
    ListElem,
    MapValue,
}

/// Declarative checker surface for polymorphic builtins.
#[derive(Debug, Clone, Copy)]
struct BuiltinSig {
    arity: usize,
    args: &'static [ArgRule],
    returns: ReturnRule,
}

const LEN_SIG: BuiltinSig = BuiltinSig {
    arity: 1,
    args: &[ArgRule::LenTarget],
    returns: ReturnRule::FixedInt,
};

const PUSH_SIG: BuiltinSig = BuiltinSig {
    arity: 2,
    args: &[ArgRule::ListMutTarget, ArgRule::ListElem],
    returns: ReturnRule::FixedVoid,
};

const AT_SIG: BuiltinSig = BuiltinSig {
    arity: 2,
    args: &[ArgRule::ListTarget, ArgRule::Int],
    returns: ReturnRule::ListElem,
};

const INSERT_SIG: BuiltinSig = BuiltinSig {
    arity: 3,
    args: &[ArgRule::MapTarget, ArgRule::MapKey, ArgRule::MapValue],
    returns: ReturnRule::FixedVoid,
};

const GET_SIG: BuiltinSig = BuiltinSig {
    arity: 2,
    args: &[ArgRule::MapTarget, ArgRule::MapKey],
    returns: ReturnRule::MapValue,
};

const HAS_SIG: BuiltinSig = BuiltinSig {
    arity: 2,
    args: &[ArgRule::MapTarget, ArgRule::MapKey],
    returns: ReturnRule::FixedBool,
};

fn sig_for(kind: BuiltinCheckerKind) -> &'static BuiltinSig {
    match kind {
        BuiltinCheckerKind::Len => &LEN_SIG,
        BuiltinCheckerKind::Push => &PUSH_SIG,
        BuiltinCheckerKind::At => &AT_SIG,
        BuiltinCheckerKind::Insert => &INSERT_SIG,
        BuiltinCheckerKind::Get => &GET_SIG,
        BuiltinCheckerKind::Has => &HAS_SIG,
        BuiltinCheckerKind::Simple => {
            panic!("simple builtins are checked via manifest signatures")
        }
    }
}

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
        if builtin.checker == BuiltinCheckerKind::Simple {
            return None;
        }
        Some(self.check_special_builtin(sig_for(builtin.checker), args, scope, errors))
    }

    fn check_special_builtin(
        &self,
        sig: &'static BuiltinSig,
        args: &[Spanned<Expr>],
        scope: &mut Scope,
        errors: &mut Vec<TypeError>,
    ) -> Type {
        if !report_arity(sig.arity, args, errors) {
            return fallback_return(sig.returns);
        }

        let mut list_elem: Option<Type> = None;
        let mut map_key: Option<Type> = None;
        let mut map_value: Option<Type> = None;

        for (arg, rule) in args.iter().zip(sig.args.iter()) {
            match rule {
                ArgRule::LenTarget => {
                    let ty = self.check_expr_inner(&arg.node, scope, errors, None);
                    if !matches!(ty, Type::List(_) | Type::Map(_, _) | Type::Str) {
                        errors.push(TypeError::Mismatch {
                            expected: "List<T>, Map<K, V>, or Str".into(),
                            found: ty.display(),
                            span: arg.span.clone(),
                        });
                    }
                }
                ArgRule::ListMutTarget => {
                    if !matches!(arg.node, Expr::Ident(_)) {
                        errors.push(TypeError::Mismatch {
                            expected: "list variable as first argument".into(),
                            found: "expression".into(),
                            span: arg.span.clone(),
                        });
                    }
                    list_elem = self.check_list_target(arg, scope, errors);
                }
                ArgRule::ListTarget => {
                    list_elem = self.check_list_target(arg, scope, errors);
                }
                ArgRule::MapTarget => {
                    let (key, value) = self.check_map_target(arg, scope, errors);
                    map_key = key;
                    map_value = value;
                }
                ArgRule::Int => {
                    let ty = self.check_expr_inner(&arg.node, scope, errors, Some(&Type::Int));
                    if ty != Type::Int {
                        errors.push(TypeError::Mismatch {
                            expected: "Int".into(),
                            found: ty.display(),
                            span: arg.span.clone(),
                        });
                    }
                }
                ArgRule::ListElem => {
                    self.check_against_expected(arg, list_elem.as_ref(), scope, errors);
                }
                ArgRule::MapKey => {
                    self.check_against_expected(arg, map_key.as_ref(), scope, errors);
                }
                ArgRule::MapValue => {
                    self.check_against_expected(arg, map_value.as_ref(), scope, errors);
                }
            }
        }

        resolve_return(sig.returns, list_elem, map_value)
    }

    fn check_list_target(
        &self,
        arg: &Spanned<Expr>,
        scope: &mut Scope,
        errors: &mut Vec<TypeError>,
    ) -> Option<Type> {
        let list_ty = self.check_expr_inner(&arg.node, scope, errors, None);
        match list_ty {
            Type::List(inner) => Some(*inner),
            _ => {
                errors.push(TypeError::Mismatch {
                    expected: "List<T>".into(),
                    found: list_ty.display(),
                    span: arg.span.clone(),
                });
                None
            }
        }
    }

    fn check_map_target(
        &self,
        arg: &Spanned<Expr>,
        scope: &mut Scope,
        errors: &mut Vec<TypeError>,
    ) -> (Option<Type>, Option<Type>) {
        let map_ty = self.check_expr_inner(&arg.node, scope, errors, None);
        match map_ty {
            Type::Map(key, value) => (Some(*key), Some(*value)),
            _ => {
                errors.push(TypeError::Mismatch {
                    expected: "Map<K, V>".into(),
                    found: map_ty.display(),
                    span: arg.span.clone(),
                });
                (None, None)
            }
        }
    }

    fn check_against_expected(
        &self,
        arg: &Spanned<Expr>,
        expected: Option<&Type>,
        scope: &mut Scope,
        errors: &mut Vec<TypeError>,
    ) {
        let found = self.check_expr_inner(&arg.node, scope, errors, expected);
        if let Some(expected) = expected {
            if !types_equal(expected, &found) {
                errors.push(TypeError::Mismatch {
                    expected: expected.display(),
                    found: found.display(),
                    span: arg.span.clone(),
                });
            }
        }
    }
}

fn report_arity(expected: usize, args: &[Spanned<Expr>], errors: &mut Vec<TypeError>) -> bool {
    if args.len() == expected {
        return true;
    }
    errors.push(TypeError::Mismatch {
        expected: format!(
            "{expected} argument{}",
            if expected == 1 { "" } else { "s" }
        ),
        found: format!("{} arguments", args.len()),
        span: args
            .first()
            .map(|arg| arg.span.clone())
            .unwrap_or_else(|| 0..0),
    });
    false
}

fn fallback_return(rule: ReturnRule) -> Type {
    match rule {
        ReturnRule::FixedInt => Type::Int,
        ReturnRule::FixedVoid | ReturnRule::ListElem | ReturnRule::MapValue => Type::Void,
        ReturnRule::FixedBool => Type::Bool,
    }
}

fn resolve_return(
    rule: ReturnRule,
    list_elem: Option<Type>,
    map_value: Option<Type>,
) -> Type {
    match rule {
        ReturnRule::FixedInt => Type::Int,
        ReturnRule::FixedVoid => Type::Void,
        ReturnRule::FixedBool => Type::Bool,
        ReturnRule::ListElem => list_elem.unwrap_or(Type::Void),
        ReturnRule::MapValue => map_value.unwrap_or(Type::Void),
    }
}