use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use nebula_ast::{BinaryOp, UnaryOp};
use nebula_builtins::manifest;
use nebula_ir::{IrExpr, IrExprKind, IrFunction, IrProgram, IrStmt};
use nebula_load::LoadedProgram;

use crate::error::EmitError;
use crate::layout::{
    common_base, python_import_from, relative_py_path,
    sorted_modules,
};

pub struct EmitOptions {
    pub out_dir: PathBuf,
    pub entry_path: PathBuf,
    pub probe_manifest: Option<PathBuf>,
    pub telemetry_path: Option<PathBuf>,
}

pub struct EmitResult {
    pub modules_emitted: usize,
    pub entry_module: PathBuf,
}

pub fn emit_workspace(
    loaded: &LoadedProgram,
    ir: &IrProgram,
    opts: &EmitOptions,
) -> Result<EmitResult, EmitError> {
    fs::create_dir_all(&opts.out_dir).map_err(|err| EmitError::Error {
        message: format!("failed to create output dir: {err}"),
    })?;
    copy_runtime_shim(&opts.out_dir)?;

    let module_paths: Vec<PathBuf> = sorted_modules(&loaded.modules);
    let base = common_base(&module_paths);
    let entry_canonical = fs::canonicalize(&opts.entry_path).map_err(|err| EmitError::Error {
        message: format!("failed to canonicalize entry path: {err}"),
    })?;

    let mut package_dirs = HashSet::new();
    let mut emitted = 0usize;
    for module_path in &module_paths {
        let py_relative = relative_py_path(module_path, &base);
        let py_path = opts.out_dir.join(&py_relative);
        if let Some(parent) = py_path.parent() {
            fs::create_dir_all(parent).map_err(|err| EmitError::Error {
                message: format!("failed to create module dir: {err}"),
            })?;
        }

        let source = emit_module(
            loaded,
            ir,
            module_path,
            module_path == &entry_canonical,
            &base,
            opts,
        )?;
        fs::write(&py_path, source).map_err(|err| EmitError::Error {
            message: format!("failed to write `{}`: {err}", py_path.display()),
        })?;
        if let Some(parent) = py_path.parent() {
            let rel_parent = parent.strip_prefix(&opts.out_dir).unwrap_or(parent);
            for ancestor in rel_parent.ancestors() {
                if ancestor.as_os_str().is_empty() || ancestor == Path::new(".") {
                    continue;
                }
                package_dirs.insert(opts.out_dir.join(ancestor));
            }
        }
        emitted += 1;
    }

    for package_dir in package_dirs {
        let init_path = package_dir.join("__init__.py");
        if !init_path.exists() {
            fs::write(&init_path, "").map_err(|err| EmitError::Error {
                message: format!("failed to write `{}`: {err}", init_path.display()),
            })?;
        }
    }

    let entry_py = opts.out_dir.join(relative_py_path(&entry_canonical, &base));
    Ok(EmitResult {
        modules_emitted: emitted,
        entry_module: entry_py,
    })
}

fn copy_runtime_shim(out_dir: &Path) -> Result<(), EmitError> {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../python/nebula_runtime");
    let dest = out_dir.join("nebula_runtime");
    copy_dir_recursive(&source, &dest).map_err(|err| EmitError::RuntimeCopy {
        message: err.to_string(),
    })
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &target)?;
        } else {
            fs::copy(&path, &target)?;
        }
    }
    Ok(())
}

struct ModuleEmitter<'a> {
    loaded: &'a LoadedProgram,
    ir: &'a IrProgram,
    module_path: &'a Path,
    _is_entry: bool,
    base: &'a Path,
    opts: &'a EmitOptions,
    out: String,
    indent: usize,
    current_sector: Option<String>,
}

impl<'a> ModuleEmitter<'a> {
    fn emit_module(
        loaded: &'a LoadedProgram,
        ir: &'a IrProgram,
        module_path: &'a Path,
        is_entry: bool,
        base: &'a Path,
        opts: &'a EmitOptions,
    ) -> Result<String, EmitError> {
        let mut emitter = Self {
            loaded,
            ir,
            module_path,
            _is_entry: is_entry,
            base,
            opts,
            out: String::new(),
            indent: 0,
            current_sector: None,
        };
        emitter.write_header();
        emitter.write_imports();
        emitter.write_sectors()?;
        if is_entry {
            emitter.write_main()?;
            emitter.write_entrypoint();
        }
        Ok(emitter.out)
    }

    fn write_line(&mut self, line: &str) {
        if line.is_empty() {
            self.out.push('\n');
            return;
        }
        self.out.push_str(&"    ".repeat(self.indent));
        self.out.push_str(line);
        self.out.push('\n');
    }

    fn write_header(&mut self) {
        self.write_line("from __future__ import annotations");
        self.write_line("");
        self.write_line("import sys");
        self.write_line("from pathlib import Path");
        self.write_line("");
        self.write_line("_NEBULA_ROOT = Path(__file__).resolve().parent");
        self.write_line("while not (_NEBULA_ROOT / \"nebula_runtime\").exists() and _NEBULA_ROOT.parent != _NEBULA_ROOT:");
        self.indent = 1;
        self.write_line("_NEBULA_ROOT = _NEBULA_ROOT.parent");
        self.indent = 0;
        self.write_line("if str(_NEBULA_ROOT) not in sys.path:");
        self.indent = 1;
        self.write_line("sys.path.insert(0, str(_NEBULA_ROOT))");
        self.indent = 0;
        self.write_line("");
        self.write_line("from nebula_runtime.builtins import *  # noqa: F401,F403");
        self.write_line("from nebula_runtime.probes import RegistryProbeHost");
        self.write_line("from nebula_runtime.runtime import (");
        self.indent = 1;
        self.write_line("PROBE_HOST,");
        self.write_line("run_main,");
        self.write_line("set_telemetry_enabled,");
        self.write_line("telemetry_enabled,");
        self.indent = 0;
        self.write_line(")");
        self.write_line(
            "from nebula_runtime.telemetry import log_telemetry, telemetry_binding_value",
        );
        self.write_line("from nebula_runtime.truthy import nebula_truthy");
        self.write_line("from nebula_runtime.values import StructValue, nebula_field, nebula_key");
        self.write_line("");
        self.write_line("_NEBULA_TELEMETRY_PATH = None");
        self.write_line("");
    }

    fn sectors_in_module(&self) -> Vec<String> {
        self.ir
            .sectors
            .keys()
            .filter(|name| {
                self.loaded
                    .symbol_sources
                    .get(*name)
                    .map(PathBuf::as_path)
                    == Some(self.module_path)
            })
            .cloned()
            .collect()
    }

    fn imported_module_paths(&self) -> Vec<PathBuf> {
        self.loaded
            .import_graph
            .get(self.module_path)
            .cloned()
            .unwrap_or_default()
    }

    fn write_imports(&mut self) {
        for imported in self.imported_module_paths() {
            let module_name = python_import_from(&imported, self.module_path, self.base);
            if module_name.is_empty() {
                continue;
            }
            let sector_names: Vec<String> = self
                .ir
                .sectors
                .keys()
                .filter(|name| {
                    self.loaded
                        .symbol_sources
                        .get(*name)
                        .map(|path| path.as_path() == imported.as_path())
                        .unwrap_or(false)
                })
                .cloned()
                .collect();
            for sector in sector_names {
                self.write_line(&format!("from {module_name} import {sector}"));
            }
        }
        self.write_line("");
    }

    fn write_sectors(&mut self) -> Result<(), EmitError> {
        for sector_name in self.sectors_in_module() {
            let sector = self
                .ir
                .sectors
                .get(&sector_name)
                .ok_or_else(|| EmitError::Error {
                    message: format!("missing IR sector `{sector_name}`"),
                })?;

            self.write_line(&format!("class {sector_name}:"));
            self.indent += 1;

            let functions: Vec<_> = sector
                .functions
                .iter()
                .filter(|(qualified, _)| self.symbol_source(qualified) == Some(self.module_path))
                .map(|(_, function)| function)
                .collect();

            if functions.is_empty() {
                self.write_line("pass");
            } else {
                for function in functions {
                    self.write_function(function)?;
                }
            }

            self.indent -= 1;
            self.write_line("");
        }
        Ok(())
    }

    fn write_function(&mut self, function: &IrFunction) -> Result<(), EmitError> {
        let params = function.params.join(", ");
        self.write_line("@staticmethod");
        self.write_line(&format!("def {}({params}):", function.name));
        self.indent += 1;
        let prev = self.current_sector.replace(function.sector.clone());
        for stmt in &function.body {
            self.write_stmt(stmt)?;
        }
        self.current_sector = prev;
        self.indent -= 1;
        Ok(())
    }

    fn write_main(&mut self) -> Result<(), EmitError> {
        self.write_line("def main():");
        self.indent += 1;
        self.write_line("global _NEBULA_TELEMETRY_PATH");
        if let Some(path) = &self.opts.telemetry_path {
            self.write_line(&format!(
                "_NEBULA_TELEMETRY_PATH = {}",
                python_string(&path.to_string_lossy())
            ));
        }
        self.current_sector = None;
        for stmt in &self.ir.mission.stmts {
            self.write_stmt(stmt)?;
        }
        self.write_line("return None");
        self.indent -= 1;
        self.write_line("");
        Ok(())
    }

    fn write_entrypoint(&mut self) {
        self.write_line("if __name__ == \"__main__\":");
        self.indent += 1;
        let mut kwargs = Vec::new();
        if let Some(path) = &self.opts.probe_manifest {
            kwargs.push(format!(
                "probe_manifest={}",
                python_string(&path.to_string_lossy())
            ));
        }
        if let Some(path) = &self.opts.telemetry_path {
            kwargs.push(format!(
                "telemetry_path={}",
                python_string(&path.to_string_lossy())
            ));
        }
        if kwargs.is_empty() {
            self.write_line("run_main(main)");
        } else {
            self.write_line(&format!("run_main(main, {})", kwargs.join(", ")));
        }
        self.indent -= 1;
    }

    fn write_stmt(&mut self, stmt: &IrStmt) -> Result<(), EmitError> {
        match stmt {
            IrStmt::Let { name, value, .. } => {
                self.write_line(&format!("{name} = {}", self.emit_expr(value)?));
                self.write_binding_telemetry("let", name);
            }
            IrStmt::Set { name, value } => {
                self.write_line(&format!("{name} = {}", self.emit_expr(value)?));
                self.write_binding_telemetry("set", name);
            }
            IrStmt::If {
                condition,
                then_body,
                else_body,
            } => {
                self.write_line(&format!("if nebula_truthy({}):", self.emit_expr(condition)?));
                self.indent += 1;
                for s in then_body {
                    self.write_stmt(s)?;
                }
                self.indent -= 1;
                if let Some(else_stmts) = else_body {
                    self.write_line("else:");
                    self.indent += 1;
                    for s in else_stmts {
                        self.write_stmt(s)?;
                    }
                    self.indent -= 1;
                }
            }
            IrStmt::While { condition, body } => {
                self.write_line(&format!(
                    "while nebula_truthy({}):",
                    self.emit_expr(condition)?
                ));
                self.indent += 1;
                for s in body {
                    self.write_stmt(s)?;
                }
                self.indent -= 1;
            }
            IrStmt::Return(expr) => {
                self.write_line(&format!("return {}", self.emit_expr(expr)?));
            }
            IrStmt::Expr(expr) => {
                self.write_line(&format!("{}  # expr stmt", self.emit_expr(expr)?));
            }
            IrStmt::ProbeCall { name, args } => {
                let resolved = self.resolve_probe_name(name);
                let mut pairs = Vec::new();
                for (key, value) in args {
                    pairs.push(format!("{}: {}", python_string(key), self.emit_expr(value)?));
                }
                self.write_line(&format!(
                    "PROBE_HOST.call({}, {{{}}})",
                    python_string(&resolved),
                    pairs.join(", ")
                ));
            }
            IrStmt::Telemetry { body } => {
                self.write_line("_prev_telemetry = telemetry_enabled()");
                self.write_line("set_telemetry_enabled(True)");
                self.write_line("try:");
                self.indent += 1;
                for s in body {
                    self.write_stmt(s)?;
                }
                self.indent -= 1;
                self.write_line("finally:");
                self.indent += 1;
                self.write_line("set_telemetry_enabled(_prev_telemetry)");
                self.indent -= 1;
            }
        }
        Ok(())
    }

    fn write_binding_telemetry(&mut self, step: &str, name: &str) {
        self.write_line(&format!(
            "log_telemetry(_NEBULA_TELEMETRY_PATH, telemetry_enabled(), {}, {}, value=telemetry_binding_value({}))",
            python_string(step),
            python_string(name),
            name
        ));
    }

    fn emit_expr(&self, expr: &IrExpr) -> Result<String, EmitError> {
        Ok(match &expr.node {
            IrExprKind::Int(n) => n.to_string(),
            IrExprKind::Float(n) => python_float(*n),
            IrExprKind::Str(s) => python_string(s),
            IrExprKind::Bool(b) => if *b { "True" } else { "False" }.to_string(),
            IrExprKind::None => "None".to_string(),
            IrExprKind::Some(inner) => self.emit_expr(inner)?,
            IrExprKind::Var(name) => name.clone(),
            IrExprKind::Unary { op, operand } => match op {
                UnaryOp::Not => format!("(not nebula_truthy({}))", self.emit_expr(operand)?),
            },
            IrExprKind::Binary { left, op, right } => {
                let l = self.emit_expr(left)?;
                let r = self.emit_expr(right)?;
                match op {
                    // Route arithmetic through shim helpers so integer overflow
                    // traps as NEB-R007 (matching the interpreter) instead of
                    // silently growing into a Python bignum.
                    BinaryOp::Plus => format!("nebula_add({l}, {r})"),
                    BinaryOp::Minus => format!("nebula_sub({l}, {r})"),
                    BinaryOp::Times => format!("nebula_mul({l}, {r})"),
                    BinaryOp::Div => format!("nebula_div({l}, {r})"),
                    BinaryOp::Mod => format!("nebula_mod({l}, {r})"),
                    BinaryOp::Eq => format!("({l} == {r})"),
                    BinaryOp::Ne => format!("({l} != {r})"),
                    BinaryOp::Lt => format!("({l} < {r})"),
                    BinaryOp::Gt => format!("({l} > {r})"),
                    BinaryOp::Le => format!("({l} <= {r})"),
                    BinaryOp::Ge => format!("({l} >= {r})"),
                    BinaryOp::And => format!("(nebula_truthy({l}) and nebula_truthy({r}))"),
                    BinaryOp::Or => format!("(nebula_truthy({l}) or nebula_truthy({r}))"),
                }
            }
            IrExprKind::Call { name, args } => {
                let callee = self.resolve_call_name(name);
                let rendered_args: Result<Vec<_>, _> =
                    args.iter().map(|arg| self.emit_expr(arg)).collect();
                let rendered_args = rendered_args?;
                if let Some(builtin) = manifest().get(name) {
                    format!("{}({})", builtin.python_name, rendered_args.join(", "))
                } else {
                    format!("{callee}({})", rendered_args.join(", "))
                }
            }
            IrExprKind::List(items) => {
                let parts: Result<Vec<_>, _> = items.iter().map(|i| self.emit_expr(i)).collect();
                format!("[{}]", parts?.join(", "))
            }
            IrExprKind::Map(entries) => {
                let mut parts = Vec::new();
                for (key, value) in entries {
                    parts.push(format!(
                        "nebula_key({}): {}",
                        self.emit_expr(key)?,
                        self.emit_expr(value)?
                    ));
                }
                format!("{{{}}}", parts.join(", "))
            }
            IrExprKind::Struct { name, fields } => {
                let mut field_parts = Vec::new();
                for (field, value) in fields {
                    field_parts.push(format!(
                        "{}: {}",
                        python_string(field),
                        self.emit_expr(value)?
                    ));
                }
                format!(
                    "StructValue({}, {{{}}})",
                    python_string(name),
                    field_parts.join(", ")
                )
            }
            IrExprKind::FieldAccess { object, field } => {
                format!(
                    "nebula_field({}, {})",
                    self.emit_expr(object)?,
                    python_string(field)
                )
            }
            IrExprKind::ProbeCall { name, args } => {
                let resolved = self.resolve_probe_name(name);
                let mut pairs = Vec::new();
                for (key, value) in args {
                    pairs.push(format!("{}: {}", python_string(key), self.emit_expr(value)?));
                }
                format!(
                    "PROBE_HOST.call({}, {{{}}})",
                    python_string(&resolved),
                    pairs.join(", ")
                )
            }
        })
    }

    fn resolve_call_name(&self, name: &str) -> String {
        if name.contains('.') {
            return name.to_string();
        }
        if let Some(sector) = &self.current_sector {
            return format!("{sector}.{name}");
        }
        name.to_string()
    }

    fn resolve_probe_name(&self, name: &str) -> String {
        if self.ir.probes.contains_key(name) {
            return name.to_string();
        }
        if let Some(sector) = &self.current_sector {
            let qualified = format!("{sector}.{name}");
            if self.ir.probes.contains_key(&qualified) {
                return qualified;
            }
        }
        name.to_string()
    }

    fn symbol_source(&self, qualified: &str) -> Option<&Path> {
        self.loaded
            .symbol_sources
            .get(qualified)
            .map(PathBuf::as_path)
    }
}

fn emit_module(
    loaded: &LoadedProgram,
    ir: &IrProgram,
    module_path: &Path,
    is_entry: bool,
    base: &Path,
    opts: &EmitOptions,
) -> Result<String, EmitError> {
    ModuleEmitter::emit_module(loaded, ir, module_path, is_entry, base, opts)
}



/// Render a float literal so Python parses it as a `float`, never an `int`.
/// `7.0_f64.to_string()` is `"7"`, which would become a Python int and corrupt
/// downstream numeric ops (e.g. `div` would truncate).
fn python_float(n: f64) -> String {
    if n.is_nan() {
        return "float('nan')".to_string();
    }
    if n.is_infinite() {
        return if n < 0.0 {
            "float('-inf')".to_string()
        } else {
            "float('inf')".to_string()
        };
    }
    let s = format!("{n}");
    if s.contains(['.', 'e', 'E']) {
        s
    } else {
        format!("{s}.0")
    }
}

fn python_string(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");
    format!("\"{escaped}\"")
}