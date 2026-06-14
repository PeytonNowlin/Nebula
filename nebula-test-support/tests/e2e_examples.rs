use std::fs;

use nebula_test_support::{compile_file, fmt_roundtrip, run_file, workspace_root};

fn example(name: &str) -> std::path::PathBuf {
    workspace_root().join("examples").join(name)
}

#[test]
fn e2e_hello_compiles_and_runs() {
    run_file(&example("hello.neb"));
}

#[test]
fn e2e_fizzbuzz_compiles_and_runs() {
    run_file(&example("fizzbuzz.neb"));
}

#[test]
fn e2e_push_demo_compiles_and_runs() {
    run_file(&example("push_demo.neb"));
}

#[test]
fn e2e_end_demo_compiles_and_runs() {
    run_file(&example("end_demo.neb"));
}

#[test]
fn e2e_import_demo_compiles_and_runs() {
    run_file(&example("import_demo.neb"));
}

#[test]
fn e2e_agent_counter_typechecks() {
    let path = example("agent_counter.neb");
    compile_file(&path);
}

#[test]
fn e2e_examples_are_valid_nebula_files() {
    let examples_dir = workspace_root().join("examples");
    let mut count = 0;
    for entry in fs::read_dir(examples_dir).expect("read examples") {
        let path = entry.expect("dir entry").path();
        if path.extension().is_some_and(|ext| ext == "neb") {
            compile_file(&path);
            count += 1;
        }
    }
    assert!(count >= 5, "expected at least 5 example programs");
}

#[test]
fn e2e_examples_survive_format_roundtrip() {
    for name in [
        "hello.neb",
        "fizzbuzz.neb",
        "push_demo.neb",
        "end_demo.neb",
        "import_demo.neb",
    ] {
        let source = fs::read_to_string(example(name)).expect("read example");
        fmt_roundtrip(&source);
    }
}
