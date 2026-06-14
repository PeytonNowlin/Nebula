use nebula_ast::*;
use nebula_syntax::parse;

pub fn format(source: &str) -> Result<String, nebula_syntax::ParseError> {
    let program = parse(source)?;
    Ok(format_program(&program))
}

fn format_program(program: &Program) -> String {
    let mut out = String::new();
    for (i, item) in program.items.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        format_top_level(&mut out, &item.node, 0);
    }
    out.push('\n');
    out
}

fn indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("  ");
    }
}

fn format_top_level(out: &mut String, item: &TopLevel, level: usize) {
    match item {
        TopLevel::Import(s) => {
            indent(out, level);
            out.push_str("import ");
            format_string(out, &s.node);
            out.push('\n');
        }
        TopLevel::Sector(sector) => {
            indent(out, level);
            out.push_str("sector ");
            out.push_str(&sector.node.name.node);
            out.push_str(" {\n");
            for sitem in &sector.node.items {
                format_sector_item(out, sitem, level + 1);
            }
            indent(out, level);
            out.push_str("}\n");
        }
        TopLevel::Mission(mission) => {
            indent(out, level);
            out.push_str("mission ");
            out.push_str(&mission.node.name.node);
            out.push_str(" {\n");
            for mitem in &mission.node.items {
                format_mission_item(out, mitem, level + 1);
            }
            indent(out, level);
            out.push_str("}\n");
        }
    }
}

fn format_sector_item(out: &mut String, item: &SectorItem, level: usize) {
    match item {
        SectorItem::Fn(f) => format_fn(out, &f.node, level),
        SectorItem::Struct(s) => format_struct(out, &s.node, level),
        SectorItem::Probe(p) => format_probe(out, &p.node, level),
    }
}

fn format_mission_item(out: &mut String, item: &MissionItem, level: usize) {
    match item {
        MissionItem::Stmt(s) => format_stmt(out, &s.node, level),
        MissionItem::Probe(p) => format_probe(out, &p.node, level),
    }
}

fn format_fn(out: &mut String, f: &FnDecl, level: usize) {
    indent(out, level);
    out.push_str("fn ");
    out.push_str(&f.name.node);
    out.push('(');
    format_params(out, &f.params);
    out.push_str(") -> ");
    format_type(out, &f.return_type.node);
    out.push_str(" {\n");
    for stmt in &f.body {
        format_stmt(out, &stmt.node, level + 1);
    }
    indent(out, level);
    out.push_str("}\n");
}

fn format_struct(out: &mut String, s: &StructDecl, level: usize) {
    indent(out, level);
    out.push_str("struct ");
    out.push_str(&s.name.node);
    out.push_str(" {\n");
    for field in &s.fields {
        indent(out, level + 1);
        out.push_str(&field.node.name.node);
        out.push_str(": ");
        format_type(out, &field.node.ty.node);
        out.push_str(";\n");
    }
    indent(out, level);
    out.push_str("}\n");
}

fn format_probe(out: &mut String, p: &ProbeDecl, level: usize) {
    indent(out, level);
    out.push_str("probe ");
    out.push_str(&p.name.node);
    out.push('(');
    format_params(out, &p.params);
    out.push_str(") -> ");
    format_type(out, &p.return_type.node);
    out.push_str(";\n");
}

fn format_params(out: &mut String, params: &[Spanned<Param>]) {
    for (i, p) in params.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&p.node.name.node);
        out.push_str(": ");
        format_type(out, &p.node.ty.node);
    }
}

fn format_stmt(out: &mut String, stmt: &Stmt, level: usize) {
    indent(out, level);
    match stmt {
        Stmt::Let {
            mutable,
            name,
            ty,
            value,
        } => {
            out.push_str("let ");
            if *mutable {
                out.push_str("mut ");
            }
            out.push_str(&name.node);
            out.push_str(": ");
            format_type(out, &ty.node);
            out.push_str(" = ");
            format_expr(out, &value.node);
            out.push_str(";\n");
        }
        Stmt::Set { name, value } => {
            out.push_str("set ");
            out.push_str(&name.node);
            out.push_str(" = ");
            format_expr(out, &value.node);
            out.push_str(";\n");
        }
        Stmt::If {
            condition,
            then_block,
            else_block,
        } => {
            out.push_str("if ");
            format_expr(out, &condition.node);
            out.push_str(" then\n");
            for s in then_block {
                format_stmt(out, &s.node, level + 1);
            }
            if let Some(else_stmts) = else_block {
                indent(out, level);
                out.push_str("else\n");
                for s in else_stmts {
                    format_stmt(out, &s.node, level + 1);
                }
            }
            indent(out, level);
            out.push_str("end\n");
        }
        Stmt::While { condition, body } => {
            out.push_str("while ");
            format_expr(out, &condition.node);
            out.push_str(" do\n");
            for s in body {
                format_stmt(out, &s.node, level + 1);
            }
            indent(out, level);
            out.push_str("end\n");
        }
        Stmt::Emit(expr) => {
            out.push_str("emit ");
            format_expr(out, &expr.node);
            out.push_str(";\n");
        }
        Stmt::Return(expr) => {
            out.push_str("return ");
            format_expr(out, &expr.node);
            out.push_str(";\n");
        }
        Stmt::Expr(expr) => {
            format_expr(out, &expr.node);
            out.push_str(";\n");
        }
        Stmt::Call { name, args } => {
            out.push_str("call ");
            out.push_str(&name.node);
            out.push('(');
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&arg.node.name.node);
                out.push_str(": ");
                format_expr(out, &arg.node.value.node);
            }
            out.push_str(");\n");
        }
        Stmt::Telemetry { body } => {
            out.push_str("telemetry\n");
            for s in body {
                format_stmt(out, &s.node, level + 1);
            }
            indent(out, level);
            out.push_str("end\n");
        }
    }
}

fn format_type(out: &mut String, ty: &Type) {
    match ty {
        Type::Int => out.push_str("Int"),
        Type::Float => out.push_str("Float"),
        Type::Bool => out.push_str("Bool"),
        Type::Str => out.push_str("Str"),
        Type::Void => out.push_str("Void"),
        Type::List(inner) => {
            out.push_str("List<");
            format_type(out, inner);
            out.push('>');
        }
        Type::Map(k, v) => {
            out.push_str("Map<");
            format_type(out, k);
            out.push_str(", ");
            format_type(out, v);
            out.push('>');
        }
        Type::Option(inner) => {
            out.push_str("Option<");
            format_type(out, inner);
            out.push('>');
        }
        Type::NoneValue => out.push_str("None"),
        Type::Fn(params, ret) => {
            out.push_str("fn(");
            for (i, p) in params.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                format_type(out, p);
            }
            out.push_str(") -> ");
            format_type(out, ret);
        }
        Type::Named(name) => out.push_str(name),
    }
}

fn format_expr(out: &mut String, expr: &Expr) {
    match expr {
        Expr::Int(n) => out.push_str(&n.to_string()),
        Expr::Float(n) => out.push_str(&n.to_string()),
        Expr::Str(s) => format_string(out, s),
        Expr::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Expr::None => out.push_str("None"),
        Expr::Some(inner) => {
            out.push_str("Some(");
            format_expr(out, &inner.node);
            out.push(')');
        }
        Expr::Ident(name) => out.push_str(&name.node),
        Expr::Unary { op: _, operand } => {
            out.push_str("not ");
            format_expr(out, &operand.node);
        }
        Expr::Binary { left, op, right } => {
            format_expr(out, &left.node);
            out.push(' ');
            out.push_str(binary_op(*op));
            out.push(' ');
            format_expr(out, &right.node);
        }
        Expr::Call { callee, args } => {
            out.push_str(&callee.node);
            out.push('(');
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                format_expr(out, &arg.node);
            }
            out.push(')');
        }
        Expr::List(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                format_expr(out, &item.node);
            }
            out.push(']');
        }
        Expr::Map(entries) => {
            out.push('{');
            for (i, entry) in entries.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                format_expr(out, &entry.node.key.node);
                out.push_str(": ");
                format_expr(out, &entry.node.value.node);
            }
            out.push('}');
        }
        Expr::StructLit { name, fields } => {
            out.push_str(&name.node);
            out.push_str(" { ");
            for (i, field) in fields.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&field.node.name.node);
                out.push_str(": ");
                format_expr(out, &field.node.value.node);
            }
            out.push_str(" }");
        }
        Expr::FieldAccess { object, field } => {
            out.push_str(&object.node);
            out.push('.');
            out.push_str(&field.node);
        }
    }
}

fn binary_op(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Plus => "plus",
        BinaryOp::Minus => "minus",
        BinaryOp::Times => "times",
        BinaryOp::Div => "div",
        BinaryOp::Mod => "mod",
        BinaryOp::Eq => "eq",
        BinaryOp::Ne => "ne",
        BinaryOp::Lt => "lt",
        BinaryOp::Gt => "gt",
        BinaryOp::Le => "le",
        BinaryOp::Ge => "ge",
        BinaryOp::And => "and",
        BinaryOp::Or => "or",
    }
}

fn format_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
}