use std::collections::HashMap;

use miette::Diagnostic;
use nebula_ast::*;
use nebula_types::TypedProgram;
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
#[error("NEB-R001 [ir_error] {message}")]
#[diagnostic(code(nebula::ir_error))]
pub struct IrError {
    pub message: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct IrProgram {
    pub sectors: HashMap<String, IrSector>,
    pub mission: IrMission,
    pub probes: HashMap<String, ProbeInfo>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct IrSector {
    pub functions: HashMap<String, IrFunction>,
    pub structs: HashMap<String, StructInfo>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct IrFunction {
    pub sector: String,
    pub name: String,
    pub qualified_name: String,
    pub params: Vec<String>,
    pub body: Vec<IrStmt>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct IrMission {
    pub name: String,
    pub stmts: Vec<IrStmt>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProbeInfo {
    pub name: String,
    pub params: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct StructInfo {
    pub fields: HashMap<String, Type>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub enum IrStmt {
    Let {
        name: String,
        mutable: bool,
        value: IrExpr,
    },
    Set {
        name: String,
        value: IrExpr,
    },
    If {
        condition: IrExpr,
        then_body: Vec<IrStmt>,
        else_body: Option<Vec<IrStmt>>,
    },
    While {
        condition: IrExpr,
        body: Vec<IrStmt>,
    },
    Return(IrExpr),
    Expr(IrExpr),
    ProbeCall {
        name: String,
        args: HashMap<String, IrExpr>,
    },
    Telemetry {
        body: Vec<IrStmt>,
    },
}

#[derive(Debug, Clone, serde::Serialize)]
pub enum IrExpr {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    None,
    Some(Box<IrExpr>),
    Var(String),
    Unary {
        op: UnaryOp,
        operand: Box<IrExpr>,
    },
    Binary {
        left: Box<IrExpr>,
        op: BinaryOp,
        right: Box<IrExpr>,
    },
    Call {
        name: String,
        args: Vec<IrExpr>,
    },
    List(Vec<IrExpr>),
    Map(Vec<(IrExpr, IrExpr)>),
    Struct {
        name: String,
        fields: HashMap<String, IrExpr>,
    },
    FieldAccess {
        object: Box<IrExpr>,
        field: String,
    },
}

pub fn lower(program: &TypedProgram) -> Result<IrProgram, IrError> {
    let mut sectors: HashMap<String, IrSector> = HashMap::new();
    let mut mission = None;
    let mut probes = HashMap::new();

    for probe in program.probes.values() {
        probes.insert(
            probe.qualified_name.clone(),
            ProbeInfo {
                name: probe.qualified_name.clone(),
                params: probe.params.iter().map(|(n, _)| n.clone()).collect(),
            },
        );
    }

    for item in &program.program.items {
        match &item.node {
            TopLevel::Sector(sector) => {
                let name = sector.node.name.node.clone();
                let mut functions = HashMap::new();
                let mut structs = HashMap::new();
                for sitem in &sector.node.items {
                    match sitem {
                        SectorItem::Fn(f) => {
                            let qualified = format!("{name}.{}", f.node.name.node);
                            functions.insert(qualified, lower_fn(&name, &f.node));
                        }
                        SectorItem::Struct(s) => {
                            let mut fields = HashMap::new();
                            for field in &s.node.fields {
                                fields.insert(
                                    field.node.name.node.clone(),
                                    field.node.ty.node.clone(),
                                );
                            }
                            structs.insert(
                                s.node.name.node.clone(),
                                StructInfo { fields },
                            );
                        }
                        SectorItem::Probe(_) => {}
                    }
                }
                sectors.insert(name, IrSector { functions, structs });
            }
            TopLevel::Mission(m) => {
                if m.node.name.node == "main" {
                    let mut stmts = Vec::new();
                    for mitem in &m.node.items {
                        match mitem {
                            MissionItem::Stmt(s) => stmts.push(lower_stmt(&s.node)),
                            MissionItem::Probe(_) => {}
                        }
                    }
                    mission = Some(IrMission {
                        name: "main".into(),
                        stmts,
                    });
                }
            }
            TopLevel::Import(_) => {}
        }
    }

    Ok(IrProgram {
        sectors,
        mission: mission.unwrap_or(IrMission {
            name: "main".into(),
            stmts: vec![],
        }),
        probes,
    })
}

fn lower_fn(sector: &str, f: &FnDecl) -> IrFunction {
    let name = f.name.node.clone();
    let qualified_name = format!("{sector}.{name}");
    IrFunction {
        sector: sector.to_string(),
        name,
        qualified_name,
        params: f.params.iter().map(|p| p.node.name.node.clone()).collect(),
        body: f.body.iter().map(|s| lower_stmt(&s.node)).collect(),
    }
}

fn lower_stmt(stmt: &Stmt) -> IrStmt {
    match stmt {
        Stmt::Let {
            mutable,
            name,
            value,
            ..
        } => IrStmt::Let {
            name: name.node.clone(),
            mutable: *mutable,
            value: lower_expr(&value.node),
        },
        Stmt::Set { name, value } => IrStmt::Set {
            name: name.node.clone(),
            value: lower_expr(&value.node),
        },
        Stmt::If {
            condition,
            then_block,
            else_block,
        } => IrStmt::If {
            condition: lower_expr(&condition.node),
            then_body: then_block.iter().map(|s| lower_stmt(&s.node)).collect(),
            else_body: else_block
                .as_ref()
                .map(|b| b.iter().map(|s| lower_stmt(&s.node)).collect()),
        },
        Stmt::While { condition, body } => IrStmt::While {
            condition: lower_expr(&condition.node),
            body: body.iter().map(|s| lower_stmt(&s.node)).collect(),
        },
        Stmt::Emit(expr) | Stmt::Return(expr) => IrStmt::Return(lower_expr(&expr.node)),
        Stmt::Expr(expr) => IrStmt::Expr(lower_expr(&expr.node)),
        Stmt::Call { name, args } => {
            let mut arg_map = HashMap::new();
            for arg in args {
                arg_map.insert(
                    arg.node.name.node.clone(),
                    lower_expr(&arg.node.value.node),
                );
            }
            IrStmt::ProbeCall {
                name: name.node.clone(),
                args: arg_map,
            }
        }
        Stmt::Telemetry { body } => IrStmt::Telemetry {
            body: body.iter().map(|s| lower_stmt(&s.node)).collect(),
        },
    }
}

fn lower_expr(expr: &Expr) -> IrExpr {
    match expr {
        Expr::Int(n) => IrExpr::Int(*n),
        Expr::Float(n) => IrExpr::Float(*n),
        Expr::Str(s) => IrExpr::Str(s.clone()),
        Expr::Bool(b) => IrExpr::Bool(*b),
        Expr::None => IrExpr::None,
        Expr::Some(inner) => IrExpr::Some(Box::new(lower_expr(&inner.node))),
        Expr::Ident(name) => IrExpr::Var(name.node.clone()),
        Expr::Unary { op, operand } => IrExpr::Unary {
            op: *op,
            operand: Box::new(lower_expr(&operand.node)),
        },
        Expr::Binary { left, op, right } => IrExpr::Binary {
            left: Box::new(lower_expr(&left.node)),
            op: *op,
            right: Box::new(lower_expr(&right.node)),
        },
        Expr::Call { callee, args } => IrExpr::Call {
            name: callee.node.clone(),
            args: args.iter().map(|a| lower_expr(&a.node)).collect(),
        },
        Expr::List(items) => IrExpr::List(items.iter().map(|i| lower_expr(&i.node)).collect()),
        Expr::Map(entries) => IrExpr::Map(
            entries
                .iter()
                .map(|e| (lower_expr(&e.node.key.node), lower_expr(&e.node.value.node)))
                .collect(),
        ),
        Expr::StructLit { name, fields } => {
            let mut map = HashMap::new();
            for f in fields {
                map.insert(f.node.name.node.clone(), lower_expr(&f.node.value.node));
            }
            IrExpr::Struct {
                name: name.node.clone(),
                fields: map,
            }
        }
        Expr::FieldAccess { object, field } => IrExpr::FieldAccess {
            object: Box::new(lower_expr(&object.node)),
            field: field.node.clone(),
        },
    }
}