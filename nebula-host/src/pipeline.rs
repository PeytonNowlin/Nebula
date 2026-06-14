use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;

use miette::{IntoDiagnostic, Report};
use nebula_ast::Program;
use nebula_fmt::format_program;
use nebula_ir::{lower, IrProgram};
use nebula_load::{load_workspace, LoadedProgram};
use nebula_syntax::parse;
use nebula_types::{report_with_source, typecheck, TypedProgram};

/// Entry point for the unified compile pipeline.
pub enum PipelineInput<'a> {
    Source { source: &'a str, entry: &'a str },
    File { path: &'a Path },
}

/// Parse → Load → Typecheck → Lower → Run
pub struct Pipeline<'a> {
    input: PipelineInput<'a>,
}

#[derive(Debug, Clone)]
pub struct ParsedArtifact {
    pub entry: String,
    pub source: String,
    pub program: Program,
}

#[derive(Debug, Clone)]
pub struct WorkspaceArtifact {
    pub entry: String,
    pub source: String,
    pub loaded: LoadedProgram,
}

#[derive(Debug, Clone)]
pub struct CompileArtifact {
    pub entry: String,
    pub source: String,
    pub loaded: LoadedProgram,
    pub typed: TypedProgram,
    pub ir: IrProgram,
}

#[derive(Debug, Clone)]
pub struct FormatResult {
    pub modules_written: usize,
    pub entry_display: Option<String>,
}

impl<'a> Pipeline<'a> {
    pub fn source(source: &'a str, entry: &'a str) -> Self {
        Self {
            input: PipelineInput::Source { source, entry },
        }
    }

    pub fn file(path: &'a Path) -> Self {
        Self {
            input: PipelineInput::File { path },
        }
    }

    pub fn parse(&self) -> Result<ParsedArtifact, Report> {
        parse_input(&self.input)
    }

    pub fn workspace(&self) -> Result<WorkspaceArtifact, Report> {
        let parsed = self.parse()?;
        load_workspace_from(parsed, &self.input)
    }

    pub fn typecheck(&self) -> Result<(WorkspaceArtifact, TypedProgram), Report> {
        let workspace = self.workspace()?;
        let typed = typecheck_loaded(&workspace)?;
        Ok((workspace, typed))
    }

    pub fn compile(&self) -> Result<CompileArtifact, Report> {
        let (workspace, typed) = self.typecheck()?;
        let ir = lower(&typed).map_err(Report::new)?;
        Ok(CompileArtifact {
            entry: workspace.entry,
            source: workspace.source,
            loaded: workspace.loaded,
            typed,
            ir,
        })
    }
}

pub fn format_entry(write: bool, path: &Path) -> Result<FormatResult, Report> {
    let workspace = Pipeline::file(path).workspace()?;
    let entry_canonical = fs::canonicalize(path).into_diagnostic()?;

    if write {
        for (module_path, module_program) in &workspace.loaded.modules {
            let formatted = format_program(module_program);
            fs::write(module_path, &formatted).into_diagnostic()?;
        }
        Ok(FormatResult {
            modules_written: workspace.loaded.modules.len(),
            entry_display: None,
        })
    } else {
        let formatted = workspace
            .loaded
            .modules
            .get(&entry_canonical)
            .map(format_program)
            .unwrap_or_else(|| format_program(&workspace.loaded.merged));
        Ok(FormatResult {
            modules_written: 0,
            entry_display: Some(formatted),
        })
    }
}

fn parse_input(input: &PipelineInput<'_>) -> Result<ParsedArtifact, Report> {
    match input {
        PipelineInput::Source { source, entry } => {
            let program = parse(source).map_err(|err| report_with_source(entry, source, err))?;
            Ok(ParsedArtifact {
                entry: entry.to_string(),
                source: source.to_string(),
                program,
            })
        }
        PipelineInput::File { path } => {
            let source = fs::read_to_string(path).into_diagnostic()?;
            let entry = path.display().to_string();
            let program =
                parse(&source).map_err(|err| report_with_source(path, &source, err))?;
            Ok(ParsedArtifact {
                entry,
                source,
                program,
            })
        }
    }
}

fn load_workspace_from(
    parsed: ParsedArtifact,
    input: &PipelineInput<'_>,
) -> Result<WorkspaceArtifact, Report> {
    match input {
        PipelineInput::File { path } => {
            let loaded = load_workspace(path, parsed.program)
                .map_err(|err| report_with_source(path, &parsed.source, err))?;
            Ok(WorkspaceArtifact {
                entry: parsed.entry,
                source: parsed.source,
                loaded,
            })
        }
        PipelineInput::Source { .. } => Ok(WorkspaceArtifact {
            entry: parsed.entry,
            source: parsed.source,
            loaded: source_only_loaded(parsed.program),
        }),
    }
}

fn typecheck_loaded(workspace: &WorkspaceArtifact) -> Result<TypedProgram, Report> {
    typecheck(&workspace.loaded.merged).map_err(|errors| {
        report_with_source(&workspace.entry, &workspace.source, errors)
    })
}

fn source_only_loaded(program: Program) -> LoadedProgram {
    LoadedProgram {
        merged: program,
        modules: BTreeMap::new(),
        symbol_sources: HashMap::new(),
        import_graph: BTreeMap::new(),
    }
}