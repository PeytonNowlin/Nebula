use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use miette::{IntoDiagnostic, Report};
use nebula_fmt::format;
use nebula_ir::lower;
use nebula_runtime::Runtime;
use nebula_syntax::parse;
use nebula_types::typecheck;

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

fn check(path: &PathBuf) -> miette::Result<()> {
    let source = read_file(path)?;
    let program = parse(&source).map_err(|e| Report::new(e))?;
    typecheck(&program).map_err(|errors| {
        let mut report = Report::new(errors[0].clone());
        for err in &errors[1..] {
            report = report.wrap_err(err.to_string());
        }
        report
    })?;
    println!("ok: {}", path.display());
    Ok(())
}

fn fmt(path: &PathBuf, write: bool) -> miette::Result<()> {
    let source = read_file(path)?;
    let formatted = format(&source).map_err(|e| Report::new(e))?;
    if write {
        fs::write(path, &formatted).into_diagnostic()?;
    } else {
        print!("{formatted}");
    }
    Ok(())
}

fn run(path: &PathBuf, telemetry: Option<PathBuf>) -> miette::Result<()> {
    let source = read_file(path)?;
    let program = parse(&source).map_err(|e| Report::new(e))?;
    let typed = typecheck(&program).map_err(|errors| {
        let mut report = Report::new(errors[0].clone());
        for err in &errors[1..] {
            report = report.wrap_err(err.to_string());
        }
        report
    })?;
    let ir = lower(&typed).map_err(|e| Report::new(e))?;

    let mut runtime = Runtime::new(&ir);
    if let Some(tel_path) = telemetry {
        runtime = runtime.with_telemetry(tel_path.to_string_lossy().into_owned());
    }

    runtime.run(&ir).map_err(|e| Report::new(e))?;
    Ok(())
}