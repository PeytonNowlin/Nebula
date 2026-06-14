use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

pub fn common_base(paths: &[PathBuf]) -> PathBuf {
    if paths.is_empty() {
        return PathBuf::from(".");
    }
    let parents: Vec<PathBuf> = paths
        .iter()
        .map(|path| {
            path.parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."))
        })
        .collect();
    let components: Vec<Vec<_>> = parents
        .iter()
        .map(|path| path.components().collect())
        .collect();
    let mut prefix = Vec::new();
    loop {
        let first = components.first().and_then(|parts| parts.get(prefix.len()));
        if first.is_none() {
            break;
        }
        let first = first.copied();
        if components
            .iter()
            .all(|parts| parts.get(prefix.len()) == first.as_ref())
        {
            if let Some(component) = first {
                prefix.push(component);
            } else {
                break;
            }
        } else {
            break;
        }
    }
    prefix.into_iter().collect()
}

pub fn relative_py_path(module_path: &Path, base: &Path) -> PathBuf {
    module_path
        .strip_prefix(base)
        .unwrap_or(module_path)
        .with_extension("py")
}

pub fn python_module_name(module_path: &Path, base: &Path) -> String {
    let rel = module_path.strip_prefix(base).unwrap_or(module_path);
    rel.with_extension("")
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(".")
}

pub fn python_import_from(imported: &Path, importer: &Path, base: &Path) -> String {
    let imported_name = python_module_name(imported, base);
    let importer_name = python_module_name(importer, base);
    if imported_name == importer_name {
        return imported_name;
    }
    if importer_name.is_empty() {
        return imported_name;
    }
    if imported_name.starts_with(&format!("{importer_name}.")) {
        return imported_name[importer_name.len() + 1..].to_string();
    }
    imported_name
}

pub fn sorted_modules<T>(modules: &BTreeMap<PathBuf, T>) -> Vec<PathBuf> {
    modules.keys().cloned().collect()
}