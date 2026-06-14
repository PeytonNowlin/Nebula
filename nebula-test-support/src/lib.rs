use std::fs;
use std::path::{Path, PathBuf};

use nebula_fmt::format;
use nebula_host::Host;
use nebula_runtime::Runtime;
use nebula_syntax::parse;
use nebula_types::typecheck;

pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

pub fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("golden")
}

/// Compare `actual` against `golden/{category}/{name}.golden`.
/// Set `NEBULA_UPDATE_GOLDEN=1` to rewrite goldens.
pub fn assert_golden(category: &str, name: &str, actual: &str) {
    let path = golden_dir().join(category).join(format!("{name}.golden"));
    if std::env::var("NEBULA_UPDATE_GOLDEN").is_ok() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, actual).expect("write golden file");
        return;
    }
    let expected = fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!(
            "missing golden file {} ({err}); run with NEBULA_UPDATE_GOLDEN=1",
            path.display()
        )
    });
    assert_eq!(expected, actual, "golden mismatch for {category}/{name}");
}

pub fn parse_and_typecheck(src: &str) -> nebula_types::TypedProgram {
    let program = parse(src).expect("parse failed");
    typecheck(&program).expect("typecheck failed")
}

pub fn compile_source(src: &str) -> nebula_ir::IrProgram {
    Host::new()
        .try_compile_source(src, None)
        .expect("compile source")
        .ir
}

pub fn compile_file(path: &Path) -> nebula_ir::IrProgram {
    Host::new().try_lower_file(path).expect("compile file")
}

pub fn run_source(src: &str) {
    let ir = compile_source(src);
    let mut runtime = Runtime::new(&ir);
    runtime.run(&ir).expect("run failed");
}

pub fn run_file(path: &Path) {
    let ir = compile_file(path);
    let mut runtime = Runtime::new(&ir);
    runtime.run(&ir).expect("run failed");
}

/// Format source and assert the formatter is idempotent.
pub fn fmt_roundtrip(src: &str) -> String {
    let once = format(src).expect("format failed");
    let twice = format(&once).expect("re-format failed");
    assert_eq!(once, twice, "formatter must be idempotent");
    once
}

pub fn join_errors<T: std::fmt::Display>(errors: &[T]) -> String {
    errors
        .iter()
        .map(|err| err.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}
