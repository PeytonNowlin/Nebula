//! CLI e2e for `nebula compile` — the deployment step of "author in Nebula,
//! ship as Python". Exercises the binary directly (beyond the library-level emit
//! test and the CI smoke step), including `--json` machine-legible output.

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

#[test]
fn compile_json_emits_record_and_artifact_runs() {
    let root = workspace_root();
    let out = std::env::temp_dir().join("nebula-compile-cli-test");
    let _ = std::fs::remove_dir_all(&out);

    let output = Command::new(env!("CARGO_BIN_EXE_nebula"))
        .args([
            "compile",
            "examples/import_demo.neb",
            "--target",
            "python",
            "--out",
        ])
        .arg(&out)
        .arg("--json")
        .current_dir(&root)
        .output()
        .expect("run nebula compile --json");
    assert!(
        output.status.success(),
        "compile failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let record: Value = serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim())
        .expect("compile --json should emit a JSON record");
    assert_eq!(record["target"], "python");
    assert_eq!(record["modules_emitted"], 2);

    // The emitted entry module is a runnable Python artifact.
    let entry = record["entry_module"]
        .as_str()
        .expect("record has entry_module");
    let run = Command::new("python3")
        .arg(entry)
        .current_dir(&root)
        .output()
        .expect("run emitted artifact");
    assert!(
        run.status.success(),
        "emitted artifact failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "42\n21");
}
