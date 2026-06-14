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

mod pipeline;

use std::path::Path;

use miette::Report;
use nebula_ast::Program;
use nebula_diagnostics::diagnostics_from_report_with_source;
use nebula_python::{emit_workspace, EmitOptions};
use nebula_runtime::{list_probe_manifest, Runtime};
use nebula_diagnostics::diagnostics_from_type_errors;
use nebula_types::{typecheck, TypedProgram, TypecheckErrors};

pub use nebula_ast::Program as AstProgram;
pub use nebula_diagnostics::DiagnosticJson;
pub use nebula_ir::IrProgram;
pub use nebula_load::LoadedProgram;
pub use nebula_runtime::Value as HostValue;
pub use nebula_python::EmitResult;
pub use nebula_runtime::{DeclaredProbe, McpServerReport, ProbeListReport};
pub use pipeline::{
    format_entry, CompileArtifact, FormatResult, Pipeline, PipelineInput, WorkspaceArtifact,
};

/// Reusable host session. Clone to share probe/telemetry configuration across calls.
#[derive(Debug, Clone, Default)]
pub struct Host {
    config: HostConfig,
}

/// Configuration shared across [`Host`] calls in an agent loop.
#[derive(Debug, Clone, Default)]
pub struct HostConfig {
    /// When set, `check_file` / `run_file` load this probe manifest.
    pub probe_manifest: Option<std::path::PathBuf>,
    /// When set, `run_*` writes telemetry JSONL to this path.
    pub telemetry_path: Option<std::path::PathBuf>,
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

    /// Parse a file without resolving imports.
    pub fn try_parse_file(&self, path: impl AsRef<Path>) -> miette::Result<Program> {
        Ok(Pipeline::file(path.as_ref()).parse()?.program)
    }

    /// Parse and resolve imports for a workspace entry file.
    pub fn try_load_file(&self, path: impl AsRef<Path>) -> miette::Result<LoadedProgram> {
        Ok(Pipeline::file(path.as_ref()).workspace()?.loaded)
    }

    /// Load and typecheck a workspace entry file.
    pub fn try_typecheck_file(&self, path: impl AsRef<Path>) -> miette::Result<TypedProgram> {
        Ok(Pipeline::file(path.as_ref()).typecheck()?.1)
    }

    /// Load, typecheck, and lower a workspace entry file.
    pub fn try_compile_file(&self, path: impl AsRef<Path>) -> miette::Result<CompileArtifact> {
        Pipeline::file(path.as_ref()).compile()
    }

    /// Lower a file on disk to IR, resolving imports.
    pub fn try_lower_file(&self, path: impl AsRef<Path>) -> miette::Result<IrProgram> {
        Ok(self.try_compile_file(path)?.ir)
    }

    /// Parse, typecheck, and lower in-memory source (no import resolution).
    pub fn try_compile_source(
        &self,
        source: &str,
        entry_label: Option<&str>,
    ) -> miette::Result<CompileArtifact> {
        Pipeline::source(source, self.entry_label(entry_label)).compile()
    }

    /// Format a workspace entry file. When `write` is false, returns formatted entry text.
    pub fn try_format_file(
        &self,
        path: impl AsRef<Path>,
        write: bool,
    ) -> miette::Result<FormatResult> {
        format_entry(write, path.as_ref())
    }

    /// Typecheck in-memory source. Returns structured diagnostics for agent loops.
    pub fn check_source(&self, source: &str) -> CheckResult {
        self.check_workspace(Pipeline::source(
            source,
            self.entry_label(None),
        ))
    }

    /// Typecheck a file on disk. Returns structured diagnostics for agent loops.
    pub fn check_file(&self, path: impl AsRef<Path>) -> CheckResult {
        self.check_workspace(Pipeline::file(path.as_ref()))
    }

    /// Typecheck a file on disk. Returns miette reports for CLI display.
    pub fn try_check_file(&self, path: impl AsRef<Path>) -> miette::Result<()> {
        Pipeline::file(path.as_ref())
            .typecheck()
            .map(|_| ())
    }

    /// Execute in-memory source. Returns structured output for agent loops.
    pub fn run_source(&self, source: &str) -> RunResult {
        match self.compile_ir(Pipeline::source(
            source,
            self.entry_label(None),
        )) {
            Ok(ir) => adapt_run(self.execute_ir(ir)),
            Err(CompileFailure::Typecheck {
                entry,
                source,
                errors,
            }) => RunResult::fail(diagnostics_from_type_errors(&entry, &source, &errors)),
            Err(CompileFailure::Report(report)) => adapt_run(Err(report)),
        }
    }

    /// Execute a file on disk. Returns structured output for agent loops.
    pub fn run_file(&self, path: impl AsRef<Path>) -> RunResult {
        match self.compile_ir(Pipeline::file(path.as_ref())) {
            Ok(ir) => adapt_run(self.execute_ir(ir)),
            Err(CompileFailure::Typecheck {
                entry,
                source,
                errors,
            }) => RunResult::fail(diagnostics_from_type_errors(&entry, &source, &errors)),
            Err(CompileFailure::Report(report)) => adapt_run(Err(report)),
        }
    }

    /// Execute in-memory source. Returns miette reports for CLI display.
    pub fn try_run_source(&self, source: &str) -> miette::Result<RunOutput> {
        let compiled = self.try_compile_source(source, None)?;
        self.execute_ir(compiled.ir)
    }

    /// Execute a file on disk, resolving imports. Returns miette reports for CLI display.
    pub fn try_run_file(&self, path: impl AsRef<Path>) -> miette::Result<RunOutput> {
        let compiled = self.try_compile_file(path)?;
        self.execute_ir(compiled.ir)
    }

    /// Compile a workspace entry file and emit Python modules to `out_dir`.
    pub fn try_emit_python(
        &self,
        path: impl AsRef<Path>,
        out_dir: impl AsRef<Path>,
    ) -> miette::Result<EmitResult> {
        let path = path.as_ref();
        let compiled = self.try_compile_file(path)?;
        emit_workspace(
            &compiled.loaded,
            &compiled.ir,
            &EmitOptions {
                out_dir: out_dir.as_ref().to_path_buf(),
                entry_path: path.to_path_buf(),
                probe_manifest: self.config.probe_manifest.clone(),
                telemetry_path: self.config.telemetry_path.clone(),
            },
        )
        .map_err(Report::new)
    }

    /// List probe bindings from a manifest. Set `discover_mcp` to query live MCP servers via `tools/list`.
    pub fn list_probes(
        &self,
        manifest: impl AsRef<Path>,
        discover_mcp: bool,
    ) -> miette::Result<ProbeListReport> {
        list_probe_manifest(manifest.as_ref(), discover_mcp).map_err(Report::new)
    }

    fn check_workspace(&self, pipeline: Pipeline<'_>) -> CheckResult {
        let workspace = match pipeline.workspace() {
            Ok(workspace) => workspace,
            Err(report) => return CheckResult::fail(diagnostics_from_report(&report)),
        };
        match typecheck(&workspace.loaded.merged) {
            Ok(_) => CheckResult::ok(),
            Err(errors) => CheckResult::fail(diagnostics_from_type_errors(
                &workspace.entry,
                &workspace.source,
                &errors,
            )),
        }
    }

    fn compile_ir(&self, pipeline: Pipeline<'_>) -> Result<IrProgram, CompileFailure> {
        let workspace = pipeline.workspace().map_err(CompileFailure::Report)?;
        let typed = match typecheck(&workspace.loaded.merged) {
            Ok(typed) => typed,
            Err(errors) => {
                return Err(CompileFailure::Typecheck {
                    entry: workspace.entry,
                    source: workspace.source,
                    errors,
                });
            }
        };
        nebula_ir::lower(&typed)
            .map_err(Report::new)
            .map_err(CompileFailure::Report)
    }

    fn entry_label<'a>(&'a self, override_label: Option<&'a str>) -> &'a str {
        override_label
            .or(self.config.source_entry_label.as_deref())
            .unwrap_or("<source>")
    }

    fn execute_ir(&self, ir: IrProgram) -> miette::Result<RunOutput> {
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

enum CompileFailure {
    Report(Report),
    Typecheck {
        entry: String,
        source: String,
        errors: TypecheckErrors,
    },
}

fn adapt_run(result: Result<RunOutput, Report>) -> RunResult {
    match result {
        Ok(output) => RunResult {
            ok: true,
            diagnostics: Vec::new(),
            printed: output.printed,
            return_value: output.return_value,
        },
        Err(report) => RunResult::fail(diagnostics_from_report(&report)),
    }
}

fn diagnostics_from_report(report: &Report) -> Vec<DiagnosticJson> {
    diagnostics_from_report_with_source(report, None)
}