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
mod run_record;

use std::path::Path;
use std::time::Instant;

use miette::Report;
use nebula_ast::{NebError, Program};
use nebula_diagnostics::diagnostics_from_report_with_source;
use nebula_python::{emit_workspace, EmitOptions};
use nebula_runtime::RuntimeError;
use nebula_runtime::{list_probe_manifest, value_json, Runtime};
use nebula_types::{diagnostics_from_type_errors, typecheck, TypecheckErrors, TypedProgram};

pub use nebula_ast::DiagnosticJson;
pub use nebula_ast::Program as AstProgram;
pub use nebula_ir::IrProgram;
pub use nebula_load::LoadedProgram;
pub use nebula_python::EmitResult;
pub use nebula_runtime::Value as HostValue;
pub use nebula_runtime::{
    DeclaredProbe, McpServerReport, ProbeListReport, ResourceLimits, SecretsStore,
};
pub use pipeline::{
    format_entry, CompileArtifact, FormatResult, Pipeline, PipelineInput, WorkspaceArtifact,
};
pub use run_record::RunRecord;

/// Reusable host session. Clone to share probe/telemetry configuration across calls.
#[derive(Debug, Clone, Default)]
pub struct Host {
    config: HostConfig,
}

/// Configuration shared across [`Host`] calls in an agent loop.
#[derive(Debug, Clone)]
pub struct HostConfig {
    /// When set, `check_file` / `run_file` load this probe manifest.
    pub probe_manifest: Option<std::path::PathBuf>,
    /// Resolved secret values merged on top of manifest `secrets` at run time.
    pub secrets: SecretsStore,
    /// When set, `run_*` writes telemetry JSONL to this path.
    pub telemetry_path: Option<std::path::PathBuf>,
    /// Label used in diagnostics for [`Host::check_source`] / [`Host::run_source`].
    pub source_entry_label: Option<String>,
    /// Interpreter resource limits applied on `run_*` paths.
    pub resource_limits: nebula_runtime::ResourceLimits,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            probe_manifest: None,
            secrets: SecretsStore::new(),
            telemetry_path: None,
            source_entry_label: None,
            resource_limits: nebula_runtime::ResourceLimits::agent_defaults(),
        }
    }
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
    pub record: RunRecord,
}

/// Successful in-process execution output.
#[derive(Debug, Clone)]
pub struct RunOutput {
    pub printed: Vec<String>,
    pub return_value: Option<HostValue>,
    pub probe_events: Vec<nebula_runtime::ProbeJsonlEvent>,
    pub probes_called: Vec<nebula_runtime::ProbeCallRecord>,
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
        self.check_workspace(Pipeline::source(source, self.entry_label(None)))
    }

    /// Typecheck a file on disk. Returns structured diagnostics for agent loops.
    pub fn check_file(&self, path: impl AsRef<Path>) -> CheckResult {
        self.check_workspace(Pipeline::file(path.as_ref()))
    }

    /// Typecheck a file on disk. Returns miette reports for CLI display.
    pub fn try_check_file(&self, path: impl AsRef<Path>) -> miette::Result<()> {
        Pipeline::file(path.as_ref()).typecheck().map(|_| ())
    }

    /// Execute in-memory source. Returns structured output for agent loops.
    pub fn run_source(&self, source: &str) -> RunResult {
        let started = Instant::now();
        let program = self.entry_label(None).to_string();
        let telemetry_path = self.telemetry_path_string();
        match self.compile_ir(Pipeline::source(source, self.entry_label(None))) {
            Ok(compiled) => self.finish_run(compiled, program, telemetry_path, started),
            Err(CompileFailure::Typecheck {
                entry,
                source,
                errors,
            }) => RunResult::from_record(RunRecord::failure(
                program,
                diagnostics_from_type_errors(&entry, &source, &errors),
                telemetry_path,
                Vec::new(),
                elapsed_ms(started),
                Vec::new(),
                Vec::new(),
            )),
            Err(CompileFailure::Report(report)) => RunResult::from_record(RunRecord::failure(
                program,
                diagnostics_from_report(&report),
                telemetry_path,
                Vec::new(),
                elapsed_ms(started),
                Vec::new(),
                Vec::new(),
            )),
        }
    }

    /// Execute a file on disk. Returns structured output for agent loops.
    pub fn run_file(&self, path: impl AsRef<Path>) -> RunResult {
        let started = Instant::now();
        let program = path.as_ref().display().to_string();
        let telemetry_path = self.telemetry_path_string();
        match self.compile_ir(Pipeline::file(path.as_ref())) {
            Ok(compiled) => self.finish_run(compiled, program, telemetry_path, started),
            Err(CompileFailure::Typecheck {
                entry,
                source,
                errors,
            }) => RunResult::from_record(RunRecord::failure(
                program,
                diagnostics_from_type_errors(&entry, &source, &errors),
                telemetry_path,
                Vec::new(),
                elapsed_ms(started),
                Vec::new(),
                Vec::new(),
            )),
            Err(CompileFailure::Report(report)) => RunResult::from_record(RunRecord::failure(
                program,
                diagnostics_from_report(&report),
                telemetry_path,
                Vec::new(),
                elapsed_ms(started),
                Vec::new(),
                Vec::new(),
            )),
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

    fn compile_ir(&self, pipeline: Pipeline<'_>) -> Result<CompiledIr, CompileFailure> {
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
        let ir = nebula_ir::lower(&typed)
            .map_err(Report::new)
            .map_err(CompileFailure::Report)?;
        Ok(CompiledIr {
            ir,
            entry: workspace.entry,
            source: workspace.source,
        })
    }

    fn finish_run(
        &self,
        compiled: CompiledIr,
        program: String,
        telemetry_path: Option<String>,
        started: Instant,
    ) -> RunResult {
        let duration_ms = elapsed_ms(started);
        match self.execute_ir_raw(compiled.ir) {
            Ok(RunOutput {
                printed,
                return_value,
                probe_events,
                probes_called,
            }) => RunResult::from_record(RunRecord::success(
                program,
                telemetry_path,
                probe_events,
                duration_ms,
                printed.clone(),
                return_value.as_ref().map(value_json::value_to_json),
                probes_called.clone(),
            ))
            .with_output(printed, return_value),
            Err((err, probe_events, printed, probes_called)) => {
                RunResult::from_record(RunRecord::failure(
                    program,
                    vec![err.to_diagnostic_json(Some(&compiled.entry), Some(&compiled.source))],
                    telemetry_path,
                    probe_events,
                    duration_ms,
                    printed,
                    probes_called,
                ))
            }
        }
    }

    fn execute_ir_raw(
        &self,
        ir: IrProgram,
    ) -> Result<
        RunOutput,
        (
            RuntimeError,
            Vec<nebula_runtime::ProbeJsonlEvent>,
            Vec<String>,
            Vec<nebula_runtime::ProbeCallRecord>,
        ),
    > {
        let mut runtime = Runtime::new(&ir).with_capture_print(true);
        if let Some(manifest) = &self.config.probe_manifest {
            let overlay = if self.config.secrets.is_empty() {
                None
            } else {
                Some(&self.config.secrets)
            };
            runtime = runtime
                .with_probe_manifest(manifest, overlay)
                .map_err(|err| (err, Vec::new(), Vec::new(), Vec::new()))?;
        }
        if let Some(path) = &self.config.telemetry_path {
            runtime = runtime.with_telemetry(path.to_string_lossy().into_owned());
        }
        runtime = runtime.with_resource_limits(self.config.resource_limits.clone());

        let run_result = runtime.run(&ir);
        let probe_events = runtime.take_probe_events();
        let probes_called = runtime.take_probes_called();
        let printed = runtime.take_printed();
        match run_result {
            Ok(value) => Ok(RunOutput {
                printed,
                return_value: Some(value),
                probe_events,
                probes_called,
            }),
            Err(err) => Err((err, probe_events, printed, probes_called)),
        }
    }

    fn telemetry_path_string(&self) -> Option<String> {
        self.config
            .telemetry_path
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned())
    }

    fn entry_label<'a>(&'a self, override_label: Option<&'a str>) -> &'a str {
        override_label
            .or(self.config.source_entry_label.as_deref())
            .unwrap_or("<source>")
    }

    fn execute_ir(&self, ir: IrProgram) -> miette::Result<RunOutput> {
        let mut runtime = Runtime::new(&ir).with_capture_print(true);
        if let Some(manifest) = &self.config.probe_manifest {
            let overlay = if self.config.secrets.is_empty() {
                None
            } else {
                Some(&self.config.secrets)
            };
            runtime = runtime
                .with_probe_manifest(manifest, overlay)
                .map_err(Report::new)?;
        }
        if let Some(path) = &self.config.telemetry_path {
            runtime = runtime.with_telemetry(path.to_string_lossy().into_owned());
        }
        runtime = runtime.with_resource_limits(self.config.resource_limits.clone());

        let value = runtime.run(&ir).map_err(Report::new)?;
        Ok(RunOutput {
            printed: runtime.take_printed(),
            return_value: Some(value),
            probe_events: runtime.take_probe_events(),
            probes_called: runtime.take_probes_called(),
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
    pub fn from_record(record: RunRecord) -> Self {
        let ok = record.exit == 0;
        Self {
            ok,
            diagnostics: record.diagnostics.clone(),
            printed: Vec::new(),
            return_value: None,
            record,
        }
    }

    pub fn fail(diagnostics: Vec<DiagnosticJson>) -> Self {
        Self::from_record(RunRecord::failure(
            "<unknown>".into(),
            diagnostics,
            None,
            Vec::new(),
            0,
            Vec::new(),
            Vec::new(),
        ))
    }

    fn with_output(mut self, printed: Vec<String>, return_value: Option<HostValue>) -> Self {
        self.printed = printed;
        self.return_value = return_value;
        self
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis() as u64
}

struct CompiledIr {
    ir: IrProgram,
    entry: String,
    source: String,
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
        Ok(RunOutput {
            printed,
            return_value,
            probe_events,
            probes_called,
        }) => RunResult::from_record(RunRecord::success(
            "<unknown>".into(),
            None,
            probe_events,
            0,
            printed.clone(),
            return_value.as_ref().map(value_json::value_to_json),
            probes_called.clone(),
        ))
        .with_output(printed, return_value),
        Err(report) => RunResult::fail(diagnostics_from_report(&report)),
    }
}

fn diagnostics_from_report(report: &Report) -> Vec<DiagnosticJson> {
    diagnostics_from_report_with_source(report, None)
}
