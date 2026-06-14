use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand};
use miette::{IntoDiagnostic, Report};
use nebula_diagnostics::{diagnostics_from_report_with_source, emit_json_diagnostics};
use nebula_host::{AstProgram, DeclaredProbe, Host, HostConfig, IrProgram};
use nebula_runtime::ResourceLimits;
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
        /// Disable interpreter resource limits (timeout, loop iterations, memory)
        #[arg(long)]
        no_resource_limits: bool,
        /// Wall-clock execution limit in milliseconds (default: 30000)
        #[arg(long)]
        max_runtime_ms: Option<u64>,
        /// Global while-loop iteration budget (default: 1000000)
        #[arg(long)]
        max_loop_iterations: Option<u64>,
        /// Approximate memory budget in megabytes (default: 64)
        #[arg(long)]
        max_memory_mb: Option<u64>,
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
    program: &'a AstProgram,
}

#[derive(Serialize)]
struct IrExport<'a> {
    entry: String,
    ir: &'a IrProgram,
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Commands::Check { file, json } => {
            let host = Host::new();
            if json {
                let result = host.check_file(&file);
                if result.ok {
                    println!("[]");
                    ExitCode::SUCCESS
                } else {
                    emit_json_diagnostics(&result.diagnostics);
                    ExitCode::FAILURE
                }
            } else {
                match host.try_check_file(&file) {
                    Ok(()) => {
                        println!("ok: {}", file.display());
                        ExitCode::SUCCESS
                    }
                    Err(err) => emit_failure(err, None, false),
                }
            }
        }
        Commands::Parse { file, json, load } => {
            let host = Host::new();
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
        Commands::Ir { file, json } => {
            let host = Host::new();
            match host.try_lower_file(&file) {
                Ok(ir) => emit_json_export(json, &IrExport {
                    entry: file.display().to_string(),
                    ir: &ir,
                }),
                Err(err) => emit_failure(err, None, json),
            }
        }
        Commands::Fmt { file, write } => {
            let host = Host::new();
            match host.try_format_file(&file, write) {
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
            }
        }
        Commands::Run {
            file,
            telemetry,
            probes,
            no_resource_limits,
            max_runtime_ms,
            max_loop_iterations,
            max_memory_mb,
            json,
        } => {
            let host = run_host(
                probes,
                telemetry,
                build_resource_limits(
                    no_resource_limits,
                    max_runtime_ms,
                    max_loop_iterations,
                    max_memory_mb,
                ),
            );
            if json {
                let result = host.run_file(&file);
                match serde_json::to_string(&result.record) {
                    Ok(payload) => println!("{payload}"),
                    Err(err) => {
                        eprintln!("failed to serialize run record: {err}");
                        return ExitCode::FAILURE;
                    }
                }
                if result.ok {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::FAILURE
                }
            } else {
                match host.try_run_file(&file) {
                    Ok(output) => {
                        for line in &output.printed {
                            println!("{line}");
                        }
                        ExitCode::SUCCESS
                    }
                    Err(err) => emit_failure(err, None, false),
                }
            }
        }
        Commands::Probes { command } => {
            let host = Host::new();
            match command {
                ProbesCommands::List { probes, mcp, json } => {
                    match list_probes(&host, &probes, mcp, json) {
                        Ok(()) => ExitCode::SUCCESS,
                        Err(err) => emit_failure(err, None, json),
                    }
                }
            }
        }
        Commands::Compile {
            file,
            target,
            out,
            telemetry,
            probes,
        } => {
            let host = host_with_config(probes, telemetry);
            match compile(&host, &file, target, out) {
                Ok(()) => ExitCode::SUCCESS,
                Err(err) => emit_failure(err, None, false),
            }
        }
    }
}

fn host_with_config(
    probe_manifest: Option<PathBuf>,
    telemetry_path: Option<PathBuf>,
) -> Host {
    if probe_manifest.is_some() || telemetry_path.is_some() {
        Host::with_config(HostConfig {
            probe_manifest,
            secrets: Default::default(),
            telemetry_path,
            source_entry_label: None,
            resource_limits: ResourceLimits::agent_defaults(),
        })
    } else {
        Host::new()
    }
}

fn run_host(
    probe_manifest: Option<PathBuf>,
    telemetry_path: Option<PathBuf>,
    resource_limits: ResourceLimits,
) -> Host {
    Host::with_config(HostConfig {
        probe_manifest,
        secrets: Default::default(),
        telemetry_path,
        source_entry_label: None,
        resource_limits,
    })
}

fn build_resource_limits(
    no_resource_limits: bool,
    max_runtime_ms: Option<u64>,
    max_loop_iterations: Option<u64>,
    max_memory_mb: Option<u64>,
) -> ResourceLimits {
    if no_resource_limits {
        return ResourceLimits::unlimited();
    }
    let mut limits = ResourceLimits::agent_defaults();
    if let Some(ms) = max_runtime_ms {
        limits.max_runtime = Some(Duration::from_millis(ms));
    }
    if let Some(iterations) = max_loop_iterations {
        limits.max_loop_iterations = Some(iterations);
    }
    if let Some(mb) = max_memory_mb {
        limits.max_memory_bytes = Some(mb as usize * 1024 * 1024);
    }
    limits
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
            DeclaredProbe::Command { name, command, .. } => {
                println!("  {name:<16} command  {}", command.join(" "));
            }
            DeclaredProbe::Mcp { name, server, tool } => {
                if let Some(tool) = tool {
                    println!("  {name:<16} mcp  server={server}  tool={tool}");
                } else {
                    println!("  {name:<16} mcp  server={server}");
                }
            }
            DeclaredProbe::ReadFile { name } => println!("  {name:<16} read_file"),
            DeclaredProbe::WriteFile { name } => println!("  {name:<16} write_file"),
            DeclaredProbe::HttpGet { name, .. } => println!("  {name:<16} http_get"),
            DeclaredProbe::JsonParse { name } => println!("  {name:<16} json_parse"),
            DeclaredProbe::EnvGet { name } => println!("  {name:<16} env_get"),
            DeclaredProbe::SecretGet { name } => println!("  {name:<16} secret_get"),
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

fn compile(host: &Host, path: &PathBuf, target: CompileTarget, out: PathBuf) -> miette::Result<()> {
    match target {
        CompileTarget::Python => compile_python(host, path, out),
    }
}

fn compile_python(host: &Host, path: &PathBuf, out: PathBuf) -> miette::Result<()> {
    let result = host.try_emit_python(path, &out)?;
    println!(
        "compiled {} module(s) to {}",
        result.modules_emitted,
        out.display()
    );
    println!("run: python {}", result.entry_module.display());
    Ok(())
}