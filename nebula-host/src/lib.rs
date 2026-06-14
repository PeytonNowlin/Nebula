//! In-process embedding API for agent runtimes.
//!
//! ```no_run
//! use nebula_host::Host;
//!
//! let host = Host::new();
//! let result = host.check_source(r#"mission main {}"#);
//! assert!(result.ok);
//!
//! let run = host.run_source(r#"mission main { print("hi"); }"#);
//! assert!(run.ok);
//! assert_eq!(run.printed, vec!["hi"]);
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use miette::{IntoDiagnostic, Report};
use nebula_ast::Program;
use nebula_diagnostics::diagnostics_from_report_with_source;
use nebula_ir::lower;
use nebula_load::load_workspace;
use nebula_runtime::{list_probe_manifest, Runtime};
use nebula_syntax::parse;
use nebula_types::{report_with_source, typecheck};

pub use nebula_ast::Program as AstProgram;
pub use nebula_diagnostics::DiagnosticJson;
pub use nebula_ir::IrProgram;
pub use nebula_runtime::Value as HostValue;
pub use nebula_runtime::{DeclaredProbe, McpServerReport, ProbeListReport};

const SOURCE_ENTRY: &str = "<source>";

/// Reusable host session. Clone to share probe/telemetry configuration across calls.
#[derive(Debug, Clone, Default)]
pub struct Host {
    config: HostConfig,
}

/// Configuration shared across [`Host`] calls in an agent loop.
#[derive(Debug, Clone, Default)]
pub struct HostConfig {
    /// When set, `check_file` / `run_file` load this probe manifest.
    pub probe_manifest: Option<PathBuf>,
    /// When set, `run_*` writes telemetry JSONL to this path.
    pub telemetry_path: Option<PathBuf>,
    /// Label used in diagnostics for [`Host::check_source`] / [`Host::run_source`].
    pub source_entry_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckResult {
    pub ok: bool,
    pub diagnostics: Vec<DiagnosticJson>,
}

#[derive(Debug, Clone)]
pub struct RunResult {
    pub ok: bool,
    pub diagnostics: Vec<DiagnosticJson>,
    pub printed: Vec<String>,
    pub return_value: Option<HostValue>,
}

/// Successful in-process execution output.
#[derive(Debug, Clone)]
pub struct RunOutput {
    pub printed: Vec<String>,
    pub return_value: Option<HostValue>,
}

struct CompiledSource {
    entry: String,
    source: String,
    program: Program,
}

impl Host {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_config(config: HostConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &HostConfig {
        &self.config
    }

    pub fn check_source(&self, source: &str) -> CheckResult {
        let entry = self
            .config
            .source_entry_label
            .as_deref()
            .unwrap_or(SOURCE_ENTRY);
        match compile_source(source, entry) {
            Ok(compiled) => match typecheck(&compiled.program) {
                Ok(_) => CheckResult::ok(),
                Err(errors) => CheckResult::fail(to_diagnostics(
                    &compiled.entry,
                    &compiled.source,
                    report_with_source(&compiled.entry, &compiled.source, errors),
                )),
            },
            Err(diagnostics) => CheckResult::fail(diagnostics),
        }
    }

    pub fn check_file(&self, path: impl AsRef<Path>) -> CheckResult {
        match self.try_check_file(path) {
            Ok(()) => CheckResult::ok(),
            Err(report) => CheckResult::fail(report_to_diagnostics(report)),
        }
    }

    /// Typecheck a file on disk, resolving imports. Returns miette reports for CLI display.
    pub fn try_check_file(&self, path: impl AsRef<Path>) -> miette::Result<()> {
        let compiled = compile_file_report(path.as_ref())?;
        typecheck(&compiled.program).map_err(|errors| {
            report_with_source(&compiled.entry, &compiled.source, errors)
        })?;
        Ok(())
    }

    pub fn run_source(&self, source: &str) -> RunResult {
        let entry = self
            .config
            .source_entry_label
            .as_deref()
            .unwrap_or(SOURCE_ENTRY);
        match compile_and_lower_source(source, entry) {
            Ok((compiled, ir)) => self.run_ir(&compiled.entry, &compiled.source, ir),
            Err(diagnostics) => RunResult::fail(diagnostics),
        }
    }

    pub fn run_file(&self, path: impl AsRef<Path>) -> RunResult {
        match self.try_run_file(path) {
            Ok(output) => RunResult {
                ok: true,
                diagnostics: Vec::new(),
                printed: output.printed,
                return_value: output.return_value,
            },
            Err(report) => RunResult::fail(report_to_diagnostics(report)),
        }
    }

    /// Execute a file on disk, resolving imports. Returns miette reports for CLI display.
    pub fn try_run_file(&self, path: impl AsRef<Path>) -> miette::Result<RunOutput> {
        let (compiled, ir) = compile_and_lower_file_report(path.as_ref())?;
        self.execute_ir(&compiled.entry, &compiled.source, ir)
    }

    /// Lower a file on disk to IR, resolving imports.
    pub fn try_lower_file(&self, path: impl AsRef<Path>) -> miette::Result<IrProgram> {
        compile_and_lower_file_report(path.as_ref()).map(|(_, ir)| ir)
    }

    /// List probe bindings from a manifest. Set `discover_mcp` to query live MCP servers via `tools/list`.
    pub fn list_probes(
        &self,
        manifest: impl AsRef<Path>,
        discover_mcp: bool,
    ) -> miette::Result<ProbeListReport> {
        list_probe_manifest(manifest.as_ref(), discover_mcp).map_err(Report::new)
    }

    fn run_ir(&self, entry: &str, source: &str, ir: IrProgram) -> RunResult {
        match self.execute_ir(entry, source, ir) {
            Ok(output) => RunResult {
                ok: true,
                diagnostics: Vec::new(),
                printed: output.printed,
                return_value: output.return_value,
            },
            Err(report) => RunResult::fail(report_to_diagnostics(report)),
        }
    }

    fn execute_ir(&self, _entry: &str, _source: &str, ir: IrProgram) -> miette::Result<RunOutput> {
        let mut runtime = Runtime::new(&ir).with_capture_print(true);
        if let Some(manifest) = &self.config.probe_manifest {
            runtime = runtime
                .with_probe_manifest(manifest)
                .map_err(Report::new)?;
        }
        if let Some(path) = &self.config.telemetry_path {
            runtime = runtime.with_telemetry(path.to_string_lossy().into_owned());
        }

        let value = runtime.run(&ir).map_err(Report::new)?;
        Ok(RunOutput {
            printed: runtime.take_printed(),
            return_value: Some(value),
        })
    }
}

impl CheckResult {
    pub fn ok() -> Self {
        Self {
            ok: true,
            diagnostics: Vec::new(),
        }
    }

    pub fn fail(diagnostics: Vec<DiagnosticJson>) -> Self {
        Self {
            ok: false,
            diagnostics,
        }
    }
}

impl RunResult {
    pub fn fail(diagnostics: Vec<DiagnosticJson>) -> Self {
        Self {
            ok: false,
            diagnostics,
            printed: Vec::new(),
            return_value: None,
        }
    }
}

fn compile_source(source: &str, entry: &str) -> Result<CompiledSource, Vec<DiagnosticJson>> {
    let program = parse(source).map_err(|err| {
        to_diagnostics(entry, source, report_with_source(entry, source, err))
    })?;
    Ok(CompiledSource {
        entry: entry.to_string(),
        source: source.to_string(),
        program,
    })
}

fn compile_file_report(path: &Path) -> miette::Result<CompiledSource> {
    let source = fs::read_to_string(path).into_diagnostic()?;
    let entry = path.display().to_string();
    let program =
        parse(&source).map_err(|err| report_with_source(path, &source, err))?;
    let loaded =
        load_workspace(path, program).map_err(|err| report_with_source(path, &source, err))?;
    Ok(CompiledSource {
        entry,
        source,
        program: loaded.merged,
    })
}

fn compile_and_lower_source(
    source: &str,
    entry: &str,
) -> Result<(CompiledSource, IrProgram), Vec<DiagnosticJson>> {
    let compiled = compile_source(source, entry)?;
    let typed = typecheck(&compiled.program).map_err(|errors| {
        to_diagnostics(
            &compiled.entry,
            &compiled.source,
            report_with_source(&compiled.entry, &compiled.source, errors),
        )
    })?;
    let ir = lower(&typed).map_err(|err| {
        to_diagnostics(
            &compiled.entry,
            &compiled.source,
            Report::new(err),
        )
    })?;
    Ok((compiled, ir))
}

fn compile_and_lower_file_report(path: &Path) -> miette::Result<(CompiledSource, IrProgram)> {
    let compiled = compile_file_report(path)?;
    let typed = typecheck(&compiled.program).map_err(|errors| {
        report_with_source(&compiled.entry, &compiled.source, errors)
    })?;
    let ir = lower(&typed).map_err(Report::new)?;
    Ok((compiled, ir))
}

fn to_diagnostics(_entry: impl AsRef<str>, source: &str, report: Report) -> Vec<DiagnosticJson> {
    let fallback = if source.is_empty() {
        None
    } else {
        Some(source)
    };
    diagnostics_from_report_with_source(&report, fallback)
}

fn report_to_diagnostics(report: Report) -> Vec<DiagnosticJson> {
    diagnostics_from_report_with_source(&report, None)
}