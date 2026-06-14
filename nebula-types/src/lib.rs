use std::collections::HashMap;

use miette::Diagnostic;
use nebula_ast::*;
use thiserror::Error;

#[derive(Debug, Clone, Error, Diagnostic)]
pub enum TypeError {
    #[error("NEB-T001 [type_error] undefined identifier `{name}`")]
    #[diagnostic(code(nebula::undefined_ident))]
    UndefinedIdent { name: String, span: Span },

    #[error("NEB-T002 [type_error] type mismatch: expected {expected}, found {found}")]
    #[diagnostic(code(nebula::type_mismatch))]
    Mismatch {
        expected: String,
        found: String,
        span: Span,
    },

    #[error("NEB-T003 [type_error] cannot assign to immutable binding `{name}`")]
    #[diagnostic(code(nebula::immutable_assign))]
    ImmutableAssign { name: String, span: Span },

    #[error("NEB-T004 [type_error] undefined function `{name}`")]
    #[diagnostic(code(nebula::undefined_fn))]
    UndefinedFn { name: String, span: Span },

    #[error("NEB-T005 [type_error] undefined struct `{name}`")]
    #[diagnostic(code(nebula::undefined_struct))]
    UndefinedStruct { name: String, span: Span },

    #[error("NEB-T006 [type_error] undefined probe `{name}`")]
    #[diagnostic(code(nebula::undefined_probe))]
    UndefinedProbe { name: String, span: Span },

    #[error("NEB-T007 [type_error] missing mission entry point `main`")]
    #[diagnostic(code(nebula::missing_main))]
    MissingMain { span: Span },

    #[error("NEB-T008 [type_error] unknown field `{field}` on struct `{struct_name}`")]
    #[diagnostic(code(nebula::unknown_field))]
    UnknownField {
        struct_name: String,
        field: String,
        span: Span,
    },
}

#[derive(Debug, Clone)]
pub struct TypedProgram {
    pub program: Program,
    pub functions: HashMap<String, FnInfo>,
    pub structs: HashMap<String, StructInfo>,
    pub probes: HashMap<String, ProbeInfo>,
    pub has_main: bool,
}

#[derive(Debug, Clone)]
pub struct FnInfo {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub return_type: Type,
}

#[derive(Debug, Clone)]
pub struct StructInfo {
    pub name: String,
    pub fields: HashMap<String, Type>,
}

#[derive(Debug, Clone)]
pub struct ProbeInfo {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub return_type: Type,
}

pub fn typecheck(program: &Program) -> Result<TypedProgram, Vec<TypeError>> {
    let mut checker = Checker::new();
    let mut errors = Vec::new();

    for item in &program.items {
        checker.collect_top_level(&item.node, &mut errors);
    }

    if !checker.has_main {
        errors.push(TypeError::MissingMain { span: 0..0 });
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    for item in &program.items {
        checker.check_top_level(&item.node, &mut errors);
    }

    if errors.is_empty() {
        Ok(TypedProgram {
            program: program.clone(),
            functions: checker.functions,
            structs: checker.structs,
            probes: checker.probes,
            has_main: checker.has_main,
        })
    } else {
        Err(errors)
    }
}

struct Checker {
    functions: HashMap<String, FnInfo>,
    structs: HashMap<String, StructInfo>,
    probes: HashMap<String, ProbeInfo>,
    has_main: bool,
}

impl Checker {
    fn new() -> Self {
        let mut functions = HashMap::new();
        functions.insert(
            "print".into(),
            FnInfo {
                name: "print".into(),
                params: vec![("value".into(), Type::Str)],
                return_type: Type::Void,
            },
        );
        functions.insert(
            "len".into(),
            FnInfo {
                name: "len".into(),
                params: vec![("value".into(), Type::List(Box::new(Type::Int)))],
                return_type: Type::Int,
            },
        );
        functions.insert(
            "str_to_int".into(),
            FnInfo {
                name: "str_to_int".into(),
                params: vec![("s".into(), Type::Str)],
                return_type: Type::Int,
            },
        );
        functions.insert(
            "int_to_str".into(),
            FnInfo {
                name: "int_to_str".into(),
                params: vec![("n".into(), Type::Int)],
                return_type: Type::Str,
            },
        );

        Self {
            functions,
            structs: HashMap::new(),
            probes: HashMap::new(),
            has_main: false,
        }
    }

    fn collect_top_level(&mut self, item: &TopLevel, _errors: &mut Vec<TypeError>) {
        match item {
            TopLevel::Sector(sector) => {
                for sitem in &sector.node.items {
                    match sitem {
                        SectorItem::Fn(f) => self.collect_fn(&f.node),
                        SectorItem::Struct(s) => self.collect_struct(&s.node),
                        SectorItem::Probe(p) => self.collect_probe(&p.node),
                    }
                }
            }
            TopLevel::Mission(mission) => {
                if mission.node.name.node == "main" {
                    self.has_main = true;
                }
                for mitem in &mission.node.items {
                    if let MissionItem::Probe(p) = mitem {
                        self.collect_probe(&p.node);
                    }
                }
            }
            TopLevel::Import(_) => {}
        }
    }

    fn collect_fn(&mut self, f: &FnDecl) {
        let name = f.name.node.clone();
        let params = f
            .params
            .iter()
            .map(|p| (p.node.name.node.clone(), p.node.ty.node.clone()))
            .collect();
        self.functions.insert(
            name.clone(),
            FnInfo {
                name,
                params,
                return_type: f.return_type.node.clone(),
            },
        );
    }

    fn collect_struct(&mut self, s: &StructDecl) {
        let name = s.name.node.clone();
        let mut fields = HashMap::new();
        for f in &s.fields {
            fields.insert(f.node.name.node.clone(), f.node.ty.node.clone());
        }
        self.structs.insert(name.clone(), StructInfo { name, fields });
    }

    fn collect_probe(&mut self, p: &ProbeDecl) {
        let name = p.name.node.clone();
        let params = p
            .params
            .iter()
            .map(|param| (param.node.name.node.clone(), param.node.ty.node.clone()))
            .collect();
        self.probes.insert(
            name.clone(),
            ProbeInfo {
                name,
                params,
                return_type: p.return_type.node.clone(),
            },
        );
    }

    fn check_top_level(&mut self, item: &TopLevel, errors: &mut Vec<TypeError>) {
        match item {
            TopLevel::Sector(sector) => {
                for sitem in &sector.node.items {
                    if let SectorItem::Fn(f) = sitem {
                        self.check_fn(&f.node, errors);
                    }
                }
            }
            TopLevel::Mission(mission) => {
                let mut scope = Scope::new();
                for mitem in &mission.node.items {
                    match mitem {
                        MissionItem::Stmt(stmt) => {
                            self.check_stmt(&stmt.node, &mut scope, &Type::Void, errors);
                        }
                        MissionItem::Probe(_) => {}
                    }
                }
            }
            TopLevel::Import(_) => {}
        }
    }

    fn check_fn(&mut self, f: &FnDecl, errors: &mut Vec<TypeError>) {
        let mut scope = Scope::new();
        for p in &f.params {
            scope.define(
                p.node.name.node.clone(),
                p.node.ty.node.clone(),
                false,
            );
        }
        for stmt in &f.body {
            self.check_stmt(&stmt.node, &mut scope, &f.return_type.node, errors);
        }
    }

    fn check_stmt(
        &mut self,
        stmt: &Stmt,
        scope: &mut Scope,
        expected_return: &Type,
        errors: &mut Vec<TypeError>,
    ) {
        match stmt {
            Stmt::Let {
                mutable,
                name,
                ty,
                value,
            } => {
                let value_ty = self.check_expr(&value.node, scope, errors);
                if !types_equal(&ty.node, &value_ty) {
                    errors.push(TypeError::Mismatch {
                        expected: ty.node.display(),
                        found: value_ty.display(),
                        span: value.span.clone(),
                    });
                }
                scope.define(name.node.clone(), ty.node.clone(), *mutable);
            }
            Stmt::Set { name, value } => {
                let binding = scope.get(&name.node).map(|(ty, mutable)| (ty.clone(), *mutable));
                match binding {
                    Some((bty, true)) => {
                        let value_ty = self.check_expr(&value.node, scope, errors);
                        if !types_equal(&bty, &value_ty) {
                            errors.push(TypeError::Mismatch {
                                expected: bty.display(),
                                found: value_ty.display(),
                                span: value.span.clone(),
                            });
                        }
                    }
                    Some((_, false)) => {
                        errors.push(TypeError::ImmutableAssign {
                            name: name.node.clone(),
                            span: name.span.clone(),
                        });
                    }
                    None => {
                        errors.push(TypeError::UndefinedIdent {
                            name: name.node.clone(),
                            span: name.span.clone(),
                        });
                    }
                }
            }
            Stmt::If {
                condition,
                then_block,
                else_block,
            } => {
                let cond_ty = self.check_expr(&condition.node, scope, errors);
                if cond_ty != Type::Bool {
                    errors.push(TypeError::Mismatch {
                        expected: "Bool".into(),
                        found: cond_ty.display(),
                        span: condition.span.clone(),
                    });
                }
                for s in then_block {
                    self.check_stmt(&s.node, scope, expected_return, errors);
                }
                if let Some(else_stmts) = else_block {
                    for s in else_stmts {
                        self.check_stmt(&s.node, scope, expected_return, errors);
                    }
                }
            }
            Stmt::While { condition, body } => {
                let cond_ty = self.check_expr(&condition.node, scope, errors);
                if cond_ty != Type::Bool {
                    errors.push(TypeError::Mismatch {
                        expected: "Bool".into(),
                        found: cond_ty.display(),
                        span: condition.span.clone(),
                    });
                }
                for s in body {
                    self.check_stmt(&s.node, scope, expected_return, errors);
                }
            }
            Stmt::Emit(expr) | Stmt::Return(expr) => {
                let ty = self.check_expr(&expr.node, scope, errors);
                if !types_equal(expected_return, &ty) {
                    errors.push(TypeError::Mismatch {
                        expected: expected_return.display(),
                        found: ty.display(),
                        span: expr.span.clone(),
                    });
                }
            }
            Stmt::Expr(expr) => {
                self.check_expr(&expr.node, scope, errors);
            }
            Stmt::Call { name, args } => {
                if let Some(probe) = self.probes.get(&name.node).cloned() {
                    self.check_probe_call(&probe, args, name.span.clone(), scope, errors);
                } else {
                    errors.push(TypeError::UndefinedProbe {
                        name: name.node.clone(),
                        span: name.span.clone(),
                    });
                }
            }
            Stmt::Telemetry { body } => {
                for s in body {
                    self.check_stmt(&s.node, scope, expected_return, errors);
                }
            }
        }
    }

    fn check_probe_call(
        &self,
        probe: &ProbeInfo,
        args: &[Spanned<NamedArg>],
        span: Span,
        scope: &mut Scope,
        errors: &mut Vec<TypeError>,
    ) {
        for (pname, pty) in &probe.params {
            let found = args.iter().find(|a| a.node.name.node == *pname);
            match found {
                Some(arg) => {
                    let actual = self.check_expr_inner(&arg.node.value.node, scope, errors);
                    if !types_equal(pty, &actual) {
                        errors.push(TypeError::Mismatch {
                            expected: pty.display(),
                            found: actual.display(),
                            span: arg.node.value.span.clone(),
                        });
                    }
                }
                None => errors.push(TypeError::Mismatch {
                    expected: format!("argument `{pname}`"),
                    found: "missing".into(),
                    span: span.clone(),
                }),
            }
        }
    }

    fn check_expr(&mut self, expr: &Expr, scope: &mut Scope, errors: &mut Vec<TypeError>) -> Type {
        self.check_expr_inner(expr, scope, errors)
    }

    fn check_expr_inner(&self, expr: &Expr, scope: &mut Scope, errors: &mut Vec<TypeError>) -> Type {
        match expr {
            Expr::Int(_) => Type::Int,
            Expr::Float(_) => Type::Float,
            Expr::Str(_) => Type::Str,
            Expr::Bool(_) => Type::Bool,
            Expr::None => Type::Option(Box::new(Type::Int)),
            Expr::Some(inner) => {
                let inner_ty = self.check_expr_inner(&inner.node, scope, errors);
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
            Expr::Unary { op: UnaryOp::Not, operand } => {
                let ty = self.check_expr_inner(&operand.node, scope, errors);
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
                let lty = self.check_expr_inner(&left.node, scope, errors);
                let rty = self.check_expr_inner(&right.node, scope, errors);
                match op {
                    BinaryOp::Plus | BinaryOp::Minus | BinaryOp::Times | BinaryOp::Div | BinaryOp::Mod => {
                        if lty != Type::Int || rty != Type::Int {
                            errors.push(TypeError::Mismatch {
                                expected: "Int operands".into(),
                                found: format!("{} and {}", lty.display(), rty.display()),
                                span: left.span.clone(),
                            });
                        }
                        if matches!(op, BinaryOp::Plus) && lty == Type::Str && rty == Type::Str {
                            return Type::Str;
                        }
                        Type::Int
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
                        if lty != Type::Int || rty != Type::Int {
                            errors.push(TypeError::Mismatch {
                                expected: "Int operands".into(),
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
            Expr::Call { callee, args } => {
                let name = &callee.node;
                if let Some(fn_info) = self.functions.get(name).cloned() {
                    if fn_info.params.len() != args.len() {
                        errors.push(TypeError::Mismatch {
                            expected: format!("{} arguments", fn_info.params.len()),
                            found: format!("{} arguments", args.len()),
                            span: callee.span.clone(),
                        });
                    }
                    for (i, (expected, arg)) in fn_info.params.iter().zip(args.iter()).enumerate() {
                        let arg_ty = self.check_expr_inner(&arg.node, scope, errors);
                        if !types_equal(&expected.1, &arg_ty) {
                            errors.push(TypeError::Mismatch {
                                expected: expected.1.display(),
                                found: arg_ty.display(),
                                span: arg.span.clone(),
                            });
                        }
                        let _ = i;
                    }
                    fn_info.return_type
                } else {
                    errors.push(TypeError::UndefinedFn {
                        name: name.clone(),
                        span: callee.span.clone(),
                    });
                    Type::Void
                }
            }
            Expr::List(items) => {
                if items.is_empty() {
                    return Type::List(Box::new(Type::Int));
                }
                let first = self.check_expr_inner(&items[0].node, scope, errors);
                for item in &items[1..] {
                    let ty = self.check_expr_inner(&item.node, scope, errors);
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
                    return Type::Map(Box::new(Type::Str), Box::new(Type::Int));
                }
                let key_ty = self.check_expr_inner(&entries[0].node.key.node, scope, errors);
                let val_ty = self.check_expr_inner(&entries[0].node.value.node, scope, errors);
                Type::Map(Box::new(key_ty), Box::new(val_ty))
            }
            Expr::StructLit { name, fields } => {
                if let Some(info) = self.structs.get(&name.node).cloned() {
                    for field in fields {
                        if let Some(expected) = info.fields.get(&field.node.name.node) {
                            let actual = self.check_expr_inner(&field.node.value.node, scope, errors);
                            if !types_equal(expected, &actual) {
                                errors.push(TypeError::Mismatch {
                                    expected: expected.display(),
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
                    Type::Named(name.node.clone())
                } else {
                    errors.push(TypeError::UndefinedStruct {
                        name: name.node.clone(),
                        span: name.span.clone(),
                    });
                    Type::Void
                }
            }
            Expr::FieldAccess { object, field } => {
                let struct_name = &object.node;
                if let Some(info) = self.structs.get(struct_name) {
                    if let Some(ty) = info.fields.get(&field.node) {
                        ty.clone()
                    } else {
                        errors.push(TypeError::UnknownField {
                            struct_name: struct_name.clone(),
                            field: field.node.clone(),
                            span: field.span.clone(),
                        });
                        Type::Void
                    }
                } else {
                    errors.push(TypeError::UndefinedStruct {
                        name: struct_name.clone(),
                        span: object.span.clone(),
                    });
                    Type::Void
                }
            }
        }
    }
}

struct Scope {
    bindings: HashMap<String, (Type, bool)>,
}

impl Scope {
    fn new() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }

    fn define(&mut self, name: String, ty: Type, mutable: bool) {
        self.bindings.insert(name, (ty, mutable));
    }

    fn get(&self, name: &str) -> Option<&(Type, bool)> {
        self.bindings.get(name)
    }
}

fn types_equal(a: &Type, b: &Type) -> bool {
    match (a, b) {
        (Type::Int, Type::Int)
        | (Type::Float, Type::Float)
        | (Type::Bool, Type::Bool)
        | (Type::Str, Type::Str)
        | (Type::Void, Type::Void) => true,
        (Type::List(a), Type::List(b)) => types_equal(a, b),
        (Type::Map(ak, av), Type::Map(bk, bv)) => types_equal(ak, bk) && types_equal(av, bv),
        (Type::Option(a), Type::Option(b)) => types_equal(a, b),
        (Type::Named(a), Type::Named(b)) => a == b,
        (Type::Fn(ap, ar), Type::Fn(bp, br)) => {
            ap.len() == bp.len()
                && ap.iter().zip(bp.iter()).all(|(x, y)| types_equal(x, y))
                && types_equal(ar, br)
        }
        _ => false,
    }
}