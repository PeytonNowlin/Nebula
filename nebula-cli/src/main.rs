use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use miette::{IntoDiagnostic, Report};
use nebula_fmt::format;
use nebula_ir::lower;
use nebula_load::load_program;
use nebula_runtime::Runtime;
use nebula_syntax::parse;
use nebula_types::{report_with_source, typecheck};

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
    },
}

struct CompiledSource {
    path: PathBuf,
    source: String,
    program: nebula_ast::Program,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Check { file } => check(&file),
        Commands::Fmt { file, write } => fmt(&file, write),
        Commands::Run { file, telemetry } => run(&file, telemetry),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err:?}");
            ExitCode::FAILURE
        }
    }
}

fn read_file(path: &PathBuf) -> miette::Result<String> {
    fs::read_to_string(path).into_diagnostic()
}

fn compile_pipeline(path: &PathBuf) -> miette::Result<CompiledSource> {
    let source = read_file(path)?;
    let program = parse(&source).map_err(|err| report_with_source(path, &source, err))?;
    let program =
        load_program(path, program).map_err(|err| report_with_source(path, &source, err))?;
    Ok(CompiledSource {
        path: path.clone(),
        source,
        program,
    })
}

fn check(path: &PathBuf) -> miette::Result<()> {
    let compiled = compile_pipeline(path)?;
    typecheck(&compiled.program)
        .map_err(|errors| report_with_source(&compiled.path, &compiled.source, errors))?;
    println!("ok: {}", path.display());
    Ok(())
}

fn fmt(path: &PathBuf, write: bool) -> miette::Result<()> {
    let source = read_file(path)?;
    let formatted = format(&source).map_err(|err| report_with_source(path, &source, err))?;
    if write {
        fs::write(path, &formatted).into_diagnostic()?;
    } else {
        print!("{formatted}");
    }
    Ok(())
}

fn run(path: &PathBuf, telemetry: Option<PathBuf>) -> miette::Result<()> {
    let compiled = compile_pipeline(path)?;
    let typed = typecheck(&compiled.program)
        .map_err(|errors| report_with_source(&compiled.path, &compiled.source, errors))?;
    let ir = lower(&typed).map_err(Report::new)?;

    let mut runtime = Runtime::new(&ir);
    if let Some(tel_path) = telemetry {
        runtime = runtime.with_telemetry(tel_path.to_string_lossy().into_owned());
    }

    runtime.run(&ir).map_err(Report::new)?;
    Ok(())
}