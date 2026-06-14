//! Proves the "author in Nebula, ship as Python" integration path: compile a
//! Nebula library to a Python package, then import it from plain Python and call
//! its sector functions. Mirrors docs/author-in-nebula-ship-as-python.md and the
//! example pair examples/agent_lib.neb + examples/agent_lib_harness.py.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use nebula_ir::lower;
use nebula_load::load_workspace;
use nebula_python::{emit_workspace, EmitOptions};
use nebula_syntax::parse;
use nebula_types::typecheck;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

#[test]
fn nebula_library_is_callable_from_python() {
    let root = workspace_root();
    let file = root.join("examples/agent_lib.neb");
    let out_dir = std::env::temp_dir().join("nebula-pymod-agent_lib");
    let _ = fs::remove_dir_all(&out_dir);

    // Compile examples/agent_lib.neb -> Python package (the deployment artifact).
    let source = fs::read_to_string(&file).expect("read agent_lib.neb");
    let program = parse(&source).expect("parse");
    let loaded = load_workspace(&file, program).expect("load");
    let typed = typecheck(&loaded.merged).expect("typecheck");
    let ir = lower(&typed).expect("lower");
    emit_workspace(
        &loaded,
        &ir,
        &EmitOptions {
            out_dir: out_dir.clone(),
            entry_path: file.clone(),
            probe_manifest: None,
            telemetry_path: None,
        },
    )
    .expect("emit");

    // Import the compiled package from Python and call its sector functions.
    let harness = root.join("examples/agent_lib_harness.py");
    let output = Command::new("python3")
        .arg(&harness)
        .arg(&out_dir)
        .output()
        .expect("run python harness");

    assert!(
        output.status.success(),
        "python harness failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("agent_lib harness ok"),
        "harness did not confirm success: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}
