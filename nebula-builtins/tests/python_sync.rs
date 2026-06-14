use std::fs;
use std::path::PathBuf;

use nebula_builtins::manifest;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn parse_python_all(source: &str) -> Vec<String> {
    let start = source
        .find("__all__")
        .expect("__all__ export list in python builtins");
    let block = &source[start..];
    let open = block.find('[').expect("__all__ opening bracket");
    let close = block.find(']').expect("__all__ closing bracket");
    block[open + 1..close]
        .lines()
        .filter_map(|line| {
            let line = line.trim().trim_end_matches(',');
            line.strip_prefix('"')
                .and_then(|rest| rest.strip_suffix('"'))
                .map(str::to_string)
        })
        .collect()
}

/// Python operator lowering helpers live in builtins.py but are not Nebula-callable builtins.
const PYTHON_OPERATOR_HELPERS: &[&str] = &[
    "nebula_add",
    "nebula_sub",
    "nebula_mul",
    "nebula_div",
    "nebula_mod",
];

#[test]
fn python_builtins_all_matches_manifest() {
    let source = fs::read_to_string(workspace_root().join("python/nebula_runtime/builtins.py"))
        .expect("read python builtins shim");

    let python_all: std::collections::BTreeSet<_> = parse_python_all(&source).into_iter().collect();
    let manifest_python: std::collections::BTreeSet<_> = manifest()
        .builtins()
        .iter()
        .map(|builtin| builtin.python_name.clone())
        .collect();

    let python_callables: std::collections::BTreeSet<_> = python_all
        .iter()
        .filter(|name| !PYTHON_OPERATOR_HELPERS.contains(&name.as_str()))
        .cloned()
        .collect();

    assert_eq!(
        python_callables, manifest_python,
        "callable entries in python/nebula_runtime/builtins.py __all__ must match nebula-builtins/builtins.toml"
    );

    for builtin in manifest().builtins() {
        assert!(
            source.contains(&format!("def {}(", builtin.python_name)),
            "missing Python implementation for {}",
            builtin.python_name
        );
    }
}