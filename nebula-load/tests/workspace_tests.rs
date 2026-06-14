use std::path::PathBuf;

use nebula_load::load_workspace;
use nebula_syntax::parse;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

#[test]
fn load_workspace_collects_entry_and_imported_modules() {
    let entry = workspace_root().join("examples/import_demo.neb");
    let source = std::fs::read_to_string(&entry).expect("read import_demo");
    let program = parse(&source).expect("parse");
    let loaded = load_workspace(&entry, program).expect("load workspace");

    assert_eq!(loaded.modules.len(), 2);
    assert!(loaded.merged.items.iter().any(|item| {
        matches!(&item.node, nebula_ast::TopLevel::Sector(_))
    }));
    assert!(loaded
        .merged
        .items
        .iter()
        .any(|item| matches!(&item.node, nebula_ast::TopLevel::Mission(_))));

    let entry_canonical = std::fs::canonicalize(&entry).expect("canonical entry");
    let entry_module = loaded.modules.get(&entry_canonical).expect("entry module");
    assert!(entry_module
        .items
        .iter()
        .any(|item| matches!(&item.node, nebula_ast::TopLevel::Import(_))));

    let math_path = workspace_root().join("std/math.neb");
    let math_canonical = std::fs::canonicalize(&math_path).expect("canonical math");
    let math_module = loaded.modules.get(&math_canonical).expect("math module");
    assert!(math_module.items.iter().all(|item| {
        matches!(
            &item.node,
            nebula_ast::TopLevel::Sector(_) | nebula_ast::TopLevel::Import(_)
        )
    }));

    assert_eq!(
        loaded.symbol_sources.get("math.double").map(PathBuf::as_path),
        Some(math_canonical.as_path())
    );
    assert_eq!(
        loaded.symbol_sources.get("math").map(PathBuf::as_path),
        Some(math_canonical.as_path())
    );

    let entry_imports = loaded.import_graph.get(&entry_canonical).expect("entry imports");
    assert!(entry_imports.contains(&math_canonical));
}