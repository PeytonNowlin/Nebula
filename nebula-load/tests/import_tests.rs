use std::fs;
use std::path::Path;

use nebula_load::load_program;
use nebula_syntax::parse;

fn workspace_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

#[test]
fn load_std_math_module() {
    let entry = workspace_root().join("examples/import_demo.neb");
    let source = fs::read_to_string(&entry).expect("read import_demo");
    let program = parse(&source).expect("parse import_demo");
    let loaded = load_program(&entry, program).expect("load imports");

    let has_math_fn = loaded.items.iter().any(|item| {
        matches!(
            &item.node,
            nebula_ast::TopLevel::Sector(sector)
                if sector.node.items.iter().any(|sitem| {
                    matches!(
                        sitem,
                        nebula_ast::SectorItem::Fn(f) if f.node.name.node == "double"
                    )
                })
        )
    });

    assert!(
        has_math_fn,
        "merged program should include imported math sector"
    );
    assert!(
        !loaded
            .items
            .iter()
            .any(|item| matches!(&item.node, nebula_ast::TopLevel::Import(_))),
        "import statements should be removed after loading"
    );
}

#[test]
fn reject_library_with_mission() {
    let dir = tempfile::tempdir().expect("tempdir");
    let lib = dir.path().join("lib.neb");
    fs::write(
        &lib,
        r#"
mission main {
  print("bad");
}
"#,
    )
    .expect("write lib");

    let entry = dir.path().join("main.neb");
    fs::write(&entry, r#"import "lib.neb"; mission main { print("ok"); }"#).expect("write main");

    let source = fs::read_to_string(&entry).expect("read main");
    let program = parse(&source).expect("parse main");
    let err = load_program(&entry, program).expect_err("library mission should fail");
    assert!(err.to_string().contains("NEB-L004"));
}

#[test]
fn reject_circular_imports() {
    let dir = tempfile::tempdir().expect("tempdir");
    let a = dir.path().join("a.neb");
    let b = dir.path().join("b.neb");

    fs::write(
        &a,
        r#"import "b.neb"; sector a { fn one() -> Int { return 1; } }"#,
    )
    .unwrap();
    fs::write(
        &b,
        r#"import "a.neb"; sector b { fn two() -> Int { return 2; } }"#,
    )
    .unwrap();

    let entry = dir.path().join("main.neb");
    fs::write(&entry, r#"import "a.neb"; mission main { print("ok"); }"#).unwrap();

    let program = parse(&fs::read_to_string(&entry).unwrap()).unwrap();
    let err = load_program(&entry, program).expect_err("cycle should fail");
    assert!(err.to_string().contains("NEB-L002"));
}
