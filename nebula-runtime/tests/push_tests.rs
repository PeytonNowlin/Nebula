use std::path::PathBuf;

use nebula_ir::lower;
use nebula_load::load_program;
use nebula_runtime::Runtime;
use nebula_syntax::parse;
use nebula_types::typecheck;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

#[test]
fn push_mutates_list_in_place() {
    let entry = workspace_root().join("examples/push_demo.neb");
    let source = std::fs::read_to_string(&entry).expect("read push_demo");
    let program = parse(&source).expect("parse push_demo");
    let loaded = load_program(&entry, program).expect("load push_demo");
    let typed = typecheck(&loaded).expect("typecheck push_demo");
    let ir = lower(&typed).expect("lower push_demo");

    let mut runtime = Runtime::new(&ir);
    runtime.run(&ir).expect("run push_demo");
}