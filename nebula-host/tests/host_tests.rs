use nebula_host::{Host, HostConfig};
use nebula_test_support::workspace_root;

#[test]
fn check_source_accepts_valid_program() {
    let host = Host::new();
    let result = host.check_source("mission main {}");
    assert!(result.ok);
    assert!(result.diagnostics.is_empty());
}

#[test]
fn check_source_returns_structured_type_error() {
    let host = Host::new();
    let result = host.check_source(r#"mission main { let x: Int = "nope"; }"#);
    assert!(!result.ok);
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].code, "NEB-T002");
}

#[test]
fn run_source_captures_print_without_stdout() {
    let host = Host::new();
    let result = host.run_source(r#"mission main { print("Hello from Nebula"); }"#);
    assert!(result.ok, "{:?}", result.diagnostics);
    assert_eq!(result.printed, vec!["Hello from Nebula"]);
}

#[test]
fn check_file_resolves_imports() {
    let host = Host::new();
    let result = host.check_file(workspace_root().join("examples/import_demo.neb"));
    assert!(result.ok, "{:?}", result.diagnostics);
}

#[test]
fn run_file_executes_imported_program() {
    let host = Host::new();
    let result = host.run_file(workspace_root().join("examples/hello.neb"));
    assert!(result.ok, "{:?}", result.diagnostics);
    assert_eq!(result.printed, vec!["Hello from Nebula"]);
}

#[test]
fn compile_file_resolves_imports_for_lower() {
    let host = Host::new();
    let ir = host
        .try_lower_file(workspace_root().join("examples/import_demo.neb"))
        .expect("lower import demo");
    assert!(!ir.mission.stmts.is_empty());
}

#[test]
fn format_file_loads_workspace_modules() {
    let host = Host::new();
    let result = host
        .try_format_file(workspace_root().join("examples/hello.neb"), false)
        .expect("format hello");
    assert!(result.entry_display.unwrap().contains("mission main"));
}

#[test]
fn run_source_divide_by_zero_includes_line_and_column() {
    let host = Host::new();
    let result = host.run_source(
        r#"mission main {
  let x: Int = 1 div 0;
}
"#,
    );
    assert!(!result.ok);
    assert_eq!(result.diagnostics.len(), 1);
    let diag = &result.diagnostics[0];
    assert_eq!(diag.code, "NEB-R004");
    assert_eq!(diag.message, "division by zero");
    let span = diag.span.as_ref().expect("runtime diagnostic span");
    assert_eq!(span.line, Some(2));
    assert!(span.column.unwrap_or(0) > 0);
    assert!(span.start < span.end);
}

#[test]
fn host_config_is_reused_across_calls() {
    let host = Host::with_config(HostConfig {
        source_entry_label: Some("agent.neb".into()),
        ..HostConfig::default()
    });
    let result = host.check_source(r#"mission main { let x: Int = "bad"; }"#);
    assert!(!result.ok);
    assert_eq!(
        result.diagnostics[0].span.as_ref().unwrap().file.as_deref(),
        Some("agent.neb")
    );
}