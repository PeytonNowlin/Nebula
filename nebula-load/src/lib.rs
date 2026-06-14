mod diagnostic_extract;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use miette::Diagnostic;
use nebula_ast::*;
use nebula_syntax::{parse, ParseError};
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
pub enum LoadError {
    #[error("NEB-L001 [load_error] import file not found: {path}")]
    #[diagnostic(code(nebula::import_not_found))]
    NotFound { path: PathBuf, span: Span },

    #[error("NEB-L002 [load_error] circular import: {path}")]
    #[diagnostic(code(nebula::circular_import))]
    Circular { path: PathBuf, span: Span },

    #[error("NEB-L003 [load_error] duplicate {kind} `{name}` defined in {existing} and {new}")]
    #[diagnostic(code(nebula::import_duplicate_symbol))]
    Duplicate {
        kind: String,
        name: String,
        existing: PathBuf,
        new: PathBuf,
        span: Span,
    },

    #[error("NEB-L004 [load_error] imported file `{path}` must not define a mission")]
    #[diagnostic(code(nebula::library_has_mission))]
    LibraryHasMission { path: PathBuf, span: Span },

    #[error("NEB-L005 [load_error] failed to read `{path}`: {message}")]
    #[diagnostic(code(nebula::import_read_error))]
    Read {
        path: PathBuf,
        message: String,
        span: Span,
    },

    #[error("NEB-L006 [load_error] failed to parse `{path}`")]
    #[diagnostic(code(nebula::import_parse_error))]
    Parse {
        path: PathBuf,
        #[source]
        source: ParseError,
        span: Span,
    },
}

impl nebula_ast::NebError for LoadError {
    fn neb_code(&self) -> &'static str {
        match self {
            LoadError::NotFound { .. } => "NEB-L001",
            LoadError::Circular { .. } => "NEB-L002",
            LoadError::Duplicate { .. } => "NEB-L003",
            LoadError::LibraryHasMission { .. } => "NEB-L004",
            LoadError::Read { .. } => "NEB-L005",
            LoadError::Parse { source, .. } => source.neb_code(),
        }
    }

    fn neb_message(&self) -> String {
        match self {
            LoadError::NotFound { path, .. } => {
                format!("import file not found: {}", path.display())
            }
            LoadError::Circular { path, .. } => format!("circular import: {}", path.display()),
            LoadError::Duplicate {
                kind,
                name,
                existing,
                new,
                ..
            } => format!(
                "duplicate {kind} `{name}` defined in {} and {}",
                existing.display(),
                new.display()
            ),
            LoadError::LibraryHasMission { path, .. } => {
                format!(
                    "imported file `{}` must not define a mission",
                    path.display()
                )
            }
            LoadError::Read { path, message, .. } => {
                format!("failed to read `{}`: {message}", path.display())
            }
            LoadError::Parse { source, .. } => source.neb_message(),
        }
    }

    fn neb_span(&self) -> Option<Span> {
        match self {
            LoadError::NotFound { span, .. }
            | LoadError::Circular { span, .. }
            | LoadError::Duplicate { span, .. }
            | LoadError::LibraryHasMission { span, .. }
            | LoadError::Read { span, .. }
            | LoadError::Parse { span, .. } => Some(span.clone()),
        }
    }
}

impl LoadError {
    pub fn to_diagnostic_jsons(
        &self,
        source: Option<&str>,
        file: Option<&str>,
    ) -> Vec<DiagnosticJson> {
        if let LoadError::Parse {
            path,
            source: parse_err,
            ..
        } = self
        {
            let imported_source = fs::read_to_string(path).ok();
            let imported_file = path.display().to_string();
            vec![parse_err
                .to_diagnostic_json(Some(imported_file.as_str()), imported_source.as_deref())]
        } else {
            vec![self.to_diagnostic_json(file, source)]
        }
    }
}

#[derive(Debug, Default)]
struct SymbolRegistry {
    functions: HashMap<String, PathBuf>,
    structs: HashMap<String, PathBuf>,
    probes: HashMap<String, PathBuf>,
    sectors: HashMap<String, PathBuf>,
}

impl SymbolRegistry {
    fn into_symbol_sources(self) -> HashMap<String, PathBuf> {
        let mut sources = HashMap::new();
        sources.extend(self.sectors);
        sources.extend(self.functions);
        sources.extend(self.structs);
        sources.extend(self.probes);
        sources
    }

    fn register_sector(
        &mut self,
        sector: &Spanned<Sector>,
        source: &Path,
    ) -> Result<(), LoadError> {
        let sector_name = sector.node.name.node.clone();

        if let Some(existing) = self.sectors.get(&sector_name) {
            return Err(LoadError::Duplicate {
                kind: "sector".into(),
                name: sector_name,
                existing: existing.clone(),
                new: source.to_path_buf(),
                span: sector.node.name.span.clone(),
            });
        }
        self.sectors
            .insert(sector_name.clone(), source.to_path_buf());

        for item in &sector.node.items {
            match item {
                SectorItem::Fn(f) => register_symbol(
                    "function",
                    &format!("{sector_name}.{}", f.node.name.node),
                    f.node.name.span.clone(),
                    source,
                    &mut self.functions,
                )?,
                SectorItem::Struct(s) => register_symbol(
                    "struct",
                    &format!("{sector_name}.{}", s.node.name.node),
                    s.node.name.span.clone(),
                    source,
                    &mut self.structs,
                )?,
                SectorItem::Probe(p) => register_symbol(
                    "probe",
                    &format!("{sector_name}.{}", p.node.name.node),
                    p.node.name.span.clone(),
                    source,
                    &mut self.probes,
                )?,
            }
        }
        Ok(())
    }
}

fn register_symbol(
    kind: &str,
    name: &str,
    span: Span,
    source: &Path,
    table: &mut HashMap<String, PathBuf>,
) -> Result<(), LoadError> {
    if let Some(existing) = table.get(name) {
        return Err(LoadError::Duplicate {
            kind: kind.into(),
            name: name.into(),
            existing: existing.clone(),
            new: source.to_path_buf(),
            span,
        });
    }
    table.insert(name.to_string(), source.to_path_buf());
    Ok(())
}

/// Result of resolving imports: a merged program plus each source file's AST.
#[derive(Debug, Clone)]
pub struct LoadedProgram {
    pub merged: Program,
    pub modules: BTreeMap<PathBuf, Program>,
    /// Maps qualified symbols (`sector`, `sector.fn`, `sector.Struct`, probes) to defining file.
    pub symbol_sources: HashMap<String, PathBuf>,
    /// Direct imports per module (canonical `.neb` paths).
    pub import_graph: BTreeMap<PathBuf, Vec<PathBuf>>,
}

struct Loader {
    entry_path: PathBuf,
    loaded: HashSet<PathBuf>,
    loading: Vec<PathBuf>,
    imported_sectors: Vec<Spanned<TopLevel>>,
    modules: BTreeMap<PathBuf, Program>,
    registry: SymbolRegistry,
    import_graph: BTreeMap<PathBuf, Vec<PathBuf>>,
}

impl Loader {
    fn new(entry_path: PathBuf) -> Self {
        Self {
            entry_path,
            loaded: HashSet::new(),
            loading: Vec::new(),
            imported_sectors: Vec::new(),
            modules: BTreeMap::new(),
            registry: SymbolRegistry::default(),
            import_graph: BTreeMap::new(),
        }
    }

    fn record_import(&mut self, from: &Path, to: &Path) -> Result<(), LoadError> {
        let from_canonical = fs::canonicalize(from).map_err(|_| LoadError::NotFound {
            path: from.to_path_buf(),
            span: 0..0,
        })?;
        let to_canonical = fs::canonicalize(to).map_err(|_| LoadError::NotFound {
            path: to.to_path_buf(),
            span: 0..0,
        })?;
        let imports = self.import_graph.entry(from_canonical).or_default();
        if !imports.contains(&to_canonical) {
            imports.push(to_canonical);
        }
        Ok(())
    }

    fn load(mut self, program: Program) -> Result<LoadedProgram, LoadError> {
        let entry_canonical =
            fs::canonicalize(&self.entry_path).map_err(|_| LoadError::NotFound {
                path: self.entry_path.clone(),
                span: 0..0,
            })?;
        self.modules
            .insert(entry_canonical.clone(), program.clone());

        let entry_dir = self
            .entry_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        let mut imports = Vec::new();
        let mut entry_items = Vec::new();

        for item in program.items {
            match item.node {
                TopLevel::Import(path) => imports.push((path, item.span)),
                other => entry_items.push(Spanned::new(other, item.span)),
            }
        }

        for (import_path, span) in imports {
            let resolved = resolve_import_path(&entry_dir, &import_path.node);
            let entry = self.entry_path.clone();
            self.record_import(&entry, &resolved)?;
            self.load_file(&resolved, span)?;
        }

        for item in &entry_items {
            match &item.node {
                TopLevel::Sector(sector) => {
                    self.registry.register_sector(sector, &entry_canonical)?;
                }
                TopLevel::Mission(mission) => {
                    for mitem in &mission.node.items {
                        if let MissionItem::Probe(probe) = mitem {
                            register_symbol(
                                "probe",
                                &probe.node.name.node,
                                probe.node.name.span.clone(),
                                &entry_canonical,
                                &mut self.registry.probes,
                            )?;
                        }
                    }
                }
                _ => {}
            }
        }

        let mut items = self.imported_sectors;
        items.extend(entry_items);

        Ok(LoadedProgram {
            merged: Program { items },
            modules: self.modules,
            symbol_sources: self.registry.into_symbol_sources(),
            import_graph: self.import_graph,
        })
    }

    fn load_file(&mut self, path: &Path, span: Span) -> Result<(), LoadError> {
        let canonical = fs::canonicalize(path).map_err(|_| LoadError::NotFound {
            path: path.to_path_buf(),
            span: span.clone(),
        })?;

        if self.loading.iter().any(|p| p == &canonical) {
            return Err(LoadError::Circular {
                path: canonical,
                span,
            });
        }

        if self.loaded.contains(&canonical) {
            return Ok(());
        }

        self.loading.push(canonical.clone());

        let source = fs::read_to_string(&canonical).map_err(|e| LoadError::Read {
            path: canonical.clone(),
            message: e.to_string(),
            span: span.clone(),
        })?;

        let program = parse(&source).map_err(|e| LoadError::Parse {
            path: canonical.clone(),
            source: e,
            span: span.clone(),
        })?;
        self.modules.insert(canonical.clone(), program.clone());

        let module_dir = canonical
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();

        let mut imports = Vec::new();
        let mut sectors = Vec::new();

        for item in program.items {
            match item.node {
                TopLevel::Import(import_path) => imports.push((import_path, item.span)),
                TopLevel::Sector(sector) => sectors.push(sector),
                TopLevel::Mission(_) => {
                    return Err(LoadError::LibraryHasMission {
                        path: canonical.clone(),
                        span: item.span,
                    });
                }
            }
        }

        for (import_path, import_span) in &imports {
            let resolved = resolve_import_path(&module_dir, &import_path.node);
            self.record_import(&canonical, &resolved)?;
            self.load_file(&resolved, import_span.clone())?;
        }

        for sector in sectors {
            let span = sector.span.clone();
            self.registry.register_sector(&sector, &canonical)?;
            self.imported_sectors
                .push(Spanned::new(TopLevel::Sector(sector), span));
        }

        self.loaded.insert(canonical);
        self.loading.pop();
        Ok(())
    }
}

fn resolve_import_path(base_dir: &Path, import_path: &str) -> PathBuf {
    let path = PathBuf::from(import_path);
    if path.is_absolute() {
        path
    } else {
        base_dir.join(path)
    }
}

/// Resolve all `import` statements in `program`, loading library modules from disk.
/// Returns the merged program (imports stripped) and each loaded file's original AST.
pub fn load_workspace(entry_path: &Path, program: Program) -> Result<LoadedProgram, LoadError> {
    Loader::new(entry_path.to_path_buf()).load(program)
}

/// Resolve imports and return only the merged program.
pub fn load_program(entry_path: &Path, program: Program) -> Result<Program, LoadError> {
    load_workspace(entry_path, program).map(|loaded| loaded.merged)
}
