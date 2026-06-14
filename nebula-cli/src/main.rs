use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use miette::{IntoDiagnostic, Report};
use nebula_ast::Program;
use nebula_diagnostics::{diagnostics_from_report_with_source, emit_json_diagnostics};
use nebula_fmt::format_program;
use nebula_host::{Host, HostConfig};
use nebula_ir::lower;
use nebula_ir::IrProgram;
use nebula_load::load_workspace;
use nebula_python::{emit_workspace, EmitOptions};
use nebula_syntax::parse;
use nebula_types::{report_with_source, typecheck};
use serde::Serialize;

#[derive(Parser)]
#[command(name = "nebula", version, about = "Nebula — agent-native programming language")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Parse and typecheck a Nebula file
    Check {
        file: PathBuf,
        /// Emit structured JSON diagnostics on failure
        #[arg(long)]
        json: bool,
    },
    /// Parse a Nebula file and export its AST
    Parse {
        file: PathBuf,
        /// Emit the AST as JSON on stdout
        #[arg(long)]
        json: bool,
        /// Resolve imports and export the merged AST
        #[arg(long)]
        load: bool,
    },
    /// Typecheck and lower a Nebula file, exporting IR
    Ir {
        file: PathBuf,
        /// Emit the IR as JSON on stdout
        #[arg(long)]
        json: bool,
    },
    /// Format a Nebula file to canonical form
    Fmt {
        file: PathBuf,
        #[arg(long)]
        write: bool,
    },
    /// Run a Nebula file via the interpreter
    Run {
        file: PathBuf,
        #[arg(long)]
        telemetry: Option<PathBuf>,
        /// JSON manifest mapping declared probes to host handlers
        #[arg(long)]
        probes: Option<PathBuf>,
        /// Emit structured JSON diagnostics on failure
        #[arg(long)]
        json: bool,
    },
    /// Compile a Nebula file to another target language
    Compile {
        file: PathBuf,
        #[arg(long, value_enum, default_value_t = CompileTarget::Python)]
        target: CompileTarget,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        telemetry: Option<PathBuf>,
        #[arg(long)]
        probes: Option<PathBuf>,
    },
}

#[derive(Clone, Copy, Default, clap::ValueEnum)]
enum CompileTarget {
    #[default]
    Python,
}

#[derive(Serialize)]
struct AstExport<'a> {
    entry: String,
    loaded: bool,
    program: &'a Program,
}

#[derive(Serialize)]
struct IrExport<'a> {
    entry: String,
    ir: &'a IrProgram,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Commands::Parse { file, json, load } => match parse_export(&file, load) {
            Ok(program) => emit_json_export(json, &AstExport {
                entry: file.display().to_string(),
                loaded: load,
                program: &program,
            }),
            Err(err) => emit_failure(err, None, json),
        },
        Commands::Ir { file, json } => match ir_export(&file) {
            Ok(ir) => emit_json_export(json, &IrExport {
                entry: file.display().to_string(),
                ir: &ir,
            }),
            Err(err) => emit_failure(err, None, json),
        },
        Commands::Check { file, json } => {
            let host = Host::new();
            match host.try_check_file(&file) {
                Ok(()) => {
                    if json {
                        println!("[]");
                    } else {
                        println!("ok: {}", file.display());
                    }
                    ExitCode::SUCCESS
                }
                Err(err) => emit_failure(err, None, json),
            }
        }
        Commands::Fmt { file, write } => match fmt(&file, write) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => emit_failure(err, None, false),
        },
        Commands::Run {
            file,
            telemetry,
            probes,
            json,
        } => {
            let host = Host::with_config(HostConfig {
                probe_manifest: probes,
                telemetry_path: telemetry,
                ..HostConfig::default()
            });
            match host.try_run_file(&file) {
                Ok(output) => {
                    if !json {
                        for line in &output.printed {
                            println!("{line}");
                        }
                    }
                    ExitCode::SUCCESS
                }
                Err(err) => emit_failure(err, None, json),
            }
        }
        Commands::Compile {
            file,
            target,
            out,
            telemetry,
            probes,
        } => match compile(&file, target, out, telemetry, probes) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => emit_failure(err, None, false),
        },
    }
}

fn emit_json_export<T: Serialize>(json: bool, value: &T) -> ExitCode {
    if !json {
        eprintln!("pass --json to emit structured output");
        return ExitCode::FAILURE;
    }
    match serde_json::to_string(value) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("failed to serialize JSON: {err}");
            ExitCode::FAILURE
        }
    }
}

fn emit_failure(err: Report, fallback_source: Option<&str>, json: bool) -> ExitCode {
    if json {
        let diagnostics = diagnostics_from_report_with_source(&err, fallback_source);
        emit_json_diagnostics(&diagnostics);
    } else {
        eprintln!("{err:?}");
    }
    ExitCode::FAILURE
}

fn read_file(path: &PathBuf) -> miette::Result<String> {
    fs::read_to_string(path).into_diagnostic()
}

fn parse_export(path: &PathBuf, load: bool) -> miette::Result<Program> {
    let source = read_file(path)?;
    let program = parse(&source).map_err(|err| report_with_source(path, &source, err))?;
    if load {
        let loaded =
            load_workspace(path, program).map_err(|err| report_with_source(path, &source, err))?;
        Ok(loaded.merged)
    } else {
        Ok(program)
    }
}

fn ir_export(path: &PathBuf) -> miette::Result<IrProgram> {
    Host::new().try_lower_file(path)
}

fn fmt(path: &PathBuf, write: bool) -> miette::Result<()> {
    let source = read_file(path)?;
    let program = parse(&source).map_err(|err| report_with_source(path, &source, err))?;
    let loaded =
        load_workspace(path, program).map_err(|err| report_with_source(path, &source, err))?;

    let entry_canonical = fs::canonicalize(path).into_diagnostic()?;

    if write {
        for (module_path, module_program) in &loaded.modules {
            let formatted = format_program(module_program);
            fs::write(module_path, &formatted).into_diagnostic()?;
        }
        eprintln!(
            "formatted {} module(s), entry {}",
            loaded.modules.len(),
            path.display()
        );
    } else if let Some(entry_program) = loaded.modules.get(&entry_canonical) {
        print!("{}", format_program(entry_program));
    } else {
        print!("{}", format_program(&loaded.merged));
    }
    Ok(())
}

fn compile(
    path: &PathBuf,
    target: CompileTarget,
    out: PathBuf,
    telemetry: Option<PathBuf>,
    probes: Option<PathBuf>,
) -> miette::Result<()> {
    match target {
        CompileTarget::Python => compile_python(path, out, telemetry, probes),
    }
}

fn compile_python(
    path: &PathBuf,
    out: PathBuf,
    telemetry: Option<PathBuf>,
    probes: Option<PathBuf>,
) -> miette::Result<()> {
    let source = read_file(path)?;
    let program = parse(&source).map_err(|err| report_with_source(path, &source, err))?;
    let loaded =
        load_workspace(path, program).map_err(|err| report_with_source(path, &source, err))?;
    let typed = typecheck(&loaded.merged)
        .map_err(|errors| report_with_source(path, &source, errors))?;
    let ir = lower(&typed).map_err(Report::new)?;

    let result = emit_workspace(
        &loaded,
        &ir,
        &EmitOptions {
            out_dir: out.clone(),
            entry_path: path.clone(),
            probe_manifest: probes,
            telemetry_path: telemetry,
        },
    )
    .map_err(Report::new)?;

    println!(
        "compiled {} module(s) to {}",
        result.modules_emitted,
        out.display()
    );
    println!("run: python {}", result.entry_module.display());
    Ok(())
}