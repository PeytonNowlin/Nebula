use std::collections::HashMap;

use nebula_ast::*;

use crate::builtins::builtin_functions;
use crate::error::TypeError;
use crate::program::{FnInfo, ProbeInfo, StructInfo};
use crate::resolve::{qualify, resolve_symbol};
use crate::scope::Scope;
use crate::util::types_equal;

pub(crate) struct Checker {
    pub(super) functions: HashMap<String, FnInfo>,
    pub(super) structs: HashMap<String, StructInfo>,
    pub(super) probes: HashMap<String, ProbeInfo>,
    sectors: HashMap<String, Span>,
    pub(super) has_main: bool,
    current_sector: Option<String>,
}

impl Checker {
    pub(crate) fn new() -> Self {
        Self {
            functions: builtin_functions(),
            structs: HashMap::new(),
            probes: HashMap::new(),
            sectors: HashMap::new(),
            has_main: false,
            current_sector: None,
        }
    }

    fn register_function(
        &mut self,
        info: FnInfo,
        span: Span,
        errors: &mut Vec<TypeError>,
    ) {
        if self.functions.contains_key(&info.qualified_name) {
            errors.push(TypeError::DuplicateSymbol {
                kind: "function".into(),
                name: info.qualified_name,
                span,
            });
            return;
        }
        self.functions.insert(info.qualified_name.clone(), info);
    }

    fn register_probe(
        &mut self,
        info: ProbeInfo,
        span: Span,
        errors: &mut Vec<TypeError>,
    ) {
        if self.probes.contains_key(&info.qualified_name) {
            errors.push(TypeError::DuplicateSymbol {
                kind: "probe".into(),
                name: info.qualified_name,
                span,
            });
            return;
        }
        self.probes.insert(info.qualified_name.clone(), info);
    }

    fn register_struct(
        &mut self,
        info: StructInfo,
        span: Span,
        errors: &mut Vec<TypeError>,
    ) {
        if self.structs.contains_key(&info.qualified_name) {
            errors.push(TypeError::DuplicateSymbol {
                kind: "struct".into(),
                name: info.qualified_name,
                span,
            });
            return;
        }
        self.structs.insert(info.qualified_name.clone(), info);
    }

    fn with_sector<R>(&mut self, sector: &str, f: impl FnOnce(&mut Self) -> R) -> R {
        let previous = self.current_sector.replace(sector.to_string());
        let result = f(self);
        self.current_sector = previous;
        result
    }

    pub(super) fn resolve_fn(&self, name: &str) -> Option<String> {
        resolve_symbol(name, self.current_sector.as_deref(), &self.functions)
    }

    pub(super) fn resolve_struct(&self, name: &str) -> Option<String> {
        resolve_symbol(name, self.current_sector.as_deref(), &self.structs)
    }

    fn resolve_probe(&self, name: &str) -> Option<String> {
        resolve_symbol(name, self.current_sector.as_deref(), &self.probes)
    }

    fn resolve_type_name(&self, name: &str) -> String {
        resolve_symbol(name, self.current_sector.as_deref(), &self.structs)
            .unwrap_or_else(|| name.to_string())
    }

    pub(super) fn struct_info_for_named(&self, name: &str) -> Option<&StructInfo> {
        if let Some(info) = self.structs.get(name) {
            return Some(info);
        }
        if let Some(key) = self.resolve_struct(name) {
            return self.structs.get(&key);
        }
        let mut matches = self
            .structs
            .values()
            .filter(|info| info.name == name)
            .collect::<Vec<_>>();
        if matches.len() == 1 {
            return Some(matches.remove(0));
        }
        None
    }

    pub(super) fn resolve_type(&self, ty: &Type) -> Type {
        match ty {
            Type::Named(name) => Type::Named(self.resolve_type_name(name)),
            Type::List(inner) => Type::List(Box::new(self.resolve_type(inner))),
            Type::Map(k, v) => Type::Map(
                Box::new(self.resolve_type(k)),
                Box::new(self.resolve_type(v)),
            ),
            Type::Option(inner) => Type::Option(Box::new(self.resolve_type(inner))),
            Type::Fn(params, ret) => Type::Fn(
                params.iter().map(|p| self.resolve_type(p)).collect(),
                Box::new(self.resolve_type(ret)),
            ),
            Type::NoneValue => Type::NoneValue,
            _ => ty.clone(),
        }
    }

    pub(crate) fn collect_top_level(&mut self, item: &TopLevel, errors: &mut Vec<TypeError>) {
        match item {
            TopLevel::Sector(sector) => {
                let sector_name = sector.node.name.node.clone();
                if self.sectors.contains_key(&sector_name) {
                    errors.push(TypeError::DuplicateSymbol {
                        kind: "sector".into(),
                        name: sector_name.clone(),
                        span: sector.node.name.span.clone(),
                    });
                } else {
                    self.sectors
                        .insert(sector_name.clone(), sector.node.name.span.clone());
                }
                for sitem in &sector.node.items {
                    match sitem {
                        SectorItem::Fn(f) => self.collect_fn(&f.node, &sector_name, errors),
                        SectorItem::Struct(s) => self.collect_struct(&s.node, &sector_name, errors),
                        SectorItem::Probe(p) => {
                            self.collect_probe(&p.node, Some(&sector_name), errors)
                        }
                    }
                }
            }
            TopLevel::Mission(mission) => {
                if mission.node.name.node == "main" {
                    self.has_main = true;
                }
                for mitem in &mission.node.items {
                    if let MissionItem::Probe(p) = mitem {
                        self.collect_probe(&p.node, None, errors);
                    }
                }
            }
            TopLevel::Import(_) => {}
        }
    }

    fn collect_fn(&mut self, f: &FnDecl, sector: &str, errors: &mut Vec<TypeError>) {
        let name = f.name.node.clone();
        let qualified_name = qualify(sector, &name);
        self.with_sector(sector, |checker| {
            let params = f
                .params
                .iter()
                .map(|p| {
                    (
                        p.node.name.node.clone(),
                        checker.resolve_type(&p.node.ty.node),
                    )
                })
                .collect();
            checker.register_function(
                FnInfo {
                    sector: sector.to_string(),
                    name,
                    qualified_name,
                    params,
                    return_type: checker.resolve_type(&f.return_type.node),
                },
                f.name.span.clone(),
                errors,
            );
        });
    }

    fn collect_struct(&mut self, s: &StructDecl, sector: &str, errors: &mut Vec<TypeError>) {
        let name = s.name.node.clone();
        let qualified_name = qualify(sector, &name);
        let mut fields = HashMap::new();
        for f in &s.fields {
            fields.insert(
                f.node.name.node.clone(),
                self.with_sector(sector, |checker| checker.resolve_type(&f.node.ty.node)),
            );
        }
        self.register_struct(
            StructInfo {
                sector: sector.to_string(),
                name,
                qualified_name,
                fields,
            },
            s.name.span.clone(),
            errors,
        );
    }

    fn collect_probe(
        &mut self,
        p: &ProbeDecl,
        sector: Option<&str>,
        errors: &mut Vec<TypeError>,
    ) {
        let name = p.name.node.clone();
        let qualified_name = sector
            .map(|s| qualify(s, &name))
            .unwrap_or_else(|| name.clone());
        let params = p
            .params
            .iter()
            .map(|param| (param.node.name.node.clone(), param.node.ty.node.clone()))
            .collect();
        self.register_probe(
            ProbeInfo {
                sector: sector.map(str::to_string),
                name,
                qualified_name,
                params,
                return_type: p.return_type.node.clone(),
            },
            p.name.span.clone(),
            errors,
        );
    }

    pub(crate) fn check_top_level(&mut self, item: &TopLevel, errors: &mut Vec<TypeError>) {
        match item {
            TopLevel::Sector(sector) => {
                let sector_name = sector.node.name.node.clone();
                for sitem in &sector.node.items {
                    if let SectorItem::Fn(f) = sitem {
                        self.with_sector(&sector_name, |checker| {
                            checker.check_fn(&f.node, errors);
                        });
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
                self.resolve_type(&p.node.ty.node),
                false,
            );
        }
        let expected_return = self.resolve_type(&f.return_type.node);
        for stmt in &f.body {
            self.check_stmt(&stmt.node, &mut scope, &expected_return, errors);
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
                let resolved_ty = self.resolve_type(&ty.node);
                let value_ty =
                    self.check_expr_inner(&value.node, scope, errors, Some(&resolved_ty));
                if !types_equal(&resolved_ty, &value_ty) {
                    errors.push(TypeError::Mismatch {
                        expected: resolved_ty.display(),
                        found: value_ty.display(),
                        span: value.span.clone(),
                    });
                }
                scope.define(name.node.clone(), resolved_ty, *mutable);
            }
            Stmt::Set { name, value } => {
                let binding = scope.get(&name.node).map(|(ty, mutable)| (ty.clone(), *mutable));
                match binding {
                    Some((bty, true)) => {
                        let value_ty = self.check_expr_inner(&value.node, scope, errors, Some(&bty));
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
                let ty = self.check_expr_inner(&expr.node, scope, errors, Some(expected_return));
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
                if let Some(resolved) = self.resolve_probe(&name.node) {
                    if let Some(probe) = self.probes.get(&resolved).cloned() {
                        self.check_probe_call(&probe, args, name.span.clone(), scope, errors);
                    }
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
                    let actual =
                        self.check_expr_inner(&arg.node.value.node, scope, errors, Some(pty));
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
}