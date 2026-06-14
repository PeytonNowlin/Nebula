use std::fs;
use std::path::PathBuf;

use nebula_fmt::format_program;
use nebula_load::load_workspace;
use nebula_syntax::parse;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

#[test]
fn format_workspace_canonicalizes_imported_modules() {
    let entry = workspace_root().join("examples/import_demo.neb");
    let source = fs::read_to_string(&entry).expect("read import_demo");
    let program = parse(&source).expect("parse");
    let loaded = load_workspace(&entry, program).expect("load workspace");

    let math_path = workspace_root().join("std/math.neb");
    let math_canonical = fs::canonicalize(&math_path).expect("canonical math");
    let math_program = loaded.modules.get(&math_canonical).expect("math module");
    let formatted_math = format_program(math_program);

    assert!(formatted_math.contains("sector math {\n"));
    assert!(formatted_math.contains("fn double(n: Int) -> Int {\n"));
    assert!(formatted_math.contains("return n times 2;\n"));
}