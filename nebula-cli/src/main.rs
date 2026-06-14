use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use miette::{IntoDiagnostic, Report};
use nebula_ast::Program;
use nebula_diagnostics::{diagnostics_from_report_with_source, emit_json_diagnostics};
use nebula_host::{DeclaredProbe, Host, HostConfig};
use nebula_ir::IrProgram;
use nebula_python::{emit_workspace, EmitOptions};
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
    /// Introspect probe host manifests and MCP tool availability
    Probes {
        #[command(subcommand)]
        command: ProbesCommands,
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

#[derive(Subcommand)]
enum ProbesCommands {
    /// List declared probe bindings (and optionally live MCP tools)
    List {
        /// JSON manifest mapping declared probes to host handlers
        #[arg(long)]
        probes: PathBuf,
        /// Query MCP servers via tools/list
        #[arg(long)]
        mcp: bool,
        /// Emit structured JSON on stdout
        #[arg(long)]
        json: bool,
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
    let host = Host::new();
    match cli.command {
        Commands::Parse { file, json, load } => {
            let program_result = if load {
                host.try_load_file(&file).map(|loaded| loaded.merged)
            } else {
                host.try_parse_file(&file)
            };
            match program_result {
                Ok(program) => emit_json_export(json, &AstExport {
                    entry: file.display().to_string(),
                    loaded: load,
                    program: &program,
                }),
                Err(err) => emit_failure(err, None, json),
            }
        }
        Commands::Ir { file, json } => match host.try_lower_file(&file) {
            Ok(ir) => emit_json_export(json, &IrExport {
                entry: file.display().to_string(),
                ir: &ir,
            }),
            Err(err) => emit_failure(err, None, json),
        },
        Commands::Check { file, json } => match host.try_check_file(&file) {
            Ok(()) => {
                if json {
                    println!("[]");
                } else {
                    println!("ok: {}", file.display());
                }
                ExitCode::SUCCESS
            }
            Err(err) => emit_failure(err, None, json),
        },
        Commands::Fmt { file, write } => match host.try_format_file(&file, write) {
            Ok(result) => {
                if let Some(formatted) = result.entry_display {
                    print!("{formatted}");
                } else {
                    eprintln!(
                        "formatted {} module(s), entry {}",
                        result.modules_written,
                        file.display()
                    );
                }
                ExitCode::SUCCESS
            }
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
        Commands::Probes { command } => match command {
            ProbesCommands::List { probes, mcp, json } => {
                match list_probes(&host, &probes, mcp, json) {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(err) => emit_failure(err, None, json),
                }
            }
        },
        Commands::Compile {
            file,
            target,
            out,
            telemetry,
            probes,
        } => match compile(&host, &file, target, out, telemetry, probes) {
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

fn list_probes(
    host: &Host,
    path: &PathBuf,
    discover_mcp: bool,
    json: bool,
) -> miette::Result<()> {
    let report = host.list_probes(path, discover_mcp)?;
    if json {
        let payload = serde_json::to_string(&report).into_diagnostic()?;
        println!("{payload}");
        return Ok(());
    }

    println!("manifest: {}", report.manifest);
    println!("probes:");
    for probe in &report.probes {
        match probe {
            DeclaredProbe::Jsonl { name, path } => {
                if let Some(path) = path {
                    println!("  {name:<16} jsonl  path={path}");
                } else {
                    println!("  {name:<16} jsonl");
                }
            }
            DeclaredProbe::Command { name, command } => {
                println!("  {name:<16} command  {}", command.join(" "));
            }
            DeclaredProbe::Mcp { name, server, tool } => {
                if let Some(tool) = tool {
                    println!("  {name:<16} mcp  server={server}  tool={tool}");
                } else {
                    println!("  {name:<16} mcp  server={server}");
                }
            }
        }
    }

    if let Some(servers) = &report.mcp_servers {
        println!("mcp servers:");
        let mut ids: Vec<_> = servers.keys().collect();
        ids.sort();
        for server_id in ids {
            let server = &servers[server_id];
            println!("  {server_id} ({})", server.transport);
            if let Some(tools) = &server.tools {
                for tool in tools {
                    if let Some(description) = &tool.description {
                        println!("    - {} — {description}", tool.name);
                    } else {
                        println!("    - {}", tool.name);
                    }
                }
            } else if let Some(error) = &server.error {
                println!("    (tools/list failed: {error})");
            }
        }
    }

    Ok(())
}

fn compile(
    host: &Host,
    path: &PathBuf,
    target: CompileTarget,
    out: PathBuf,
    telemetry: Option<PathBuf>,
    probes: Option<PathBuf>,
) -> miette::Result<()> {
    match target {
        CompileTarget::Python => compile_python(host, path, out, telemetry, probes),
    }
}

fn compile_python(
    host: &Host,
    path: &PathBuf,
    out: PathBuf,
    telemetry: Option<PathBuf>,
    probes: Option<PathBuf>,
) -> miette::Result<()> {
    let compiled = host.try_compile_file(path)?;
    let result = emit_workspace(
        &compiled.loaded,
        &compiled.ir,
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