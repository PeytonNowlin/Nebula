use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::thread;
use std::time::Duration;

use nebula_ir::lower;
use nebula_runtime::{ProbeInvocation, ProbeHost, RegistryProbeHost, Runtime};
use nebula_syntax::parse;
use nebula_types::typecheck;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn run_notify_probe(manifest_path: &PathBuf) {
    let src = r#"
mission main {
  probe notify(channel: Str, message: Str) -> Void;
  call notify(channel: "ops", message: "ready");
}
"#;
    let program = parse(src).expect("parse");
    let typed = typecheck(&program).expect("typecheck");
    let ir = lower(&typed).expect("lower");

    let mut runtime = Runtime::new(&ir)
        .with_probe_manifest(manifest_path)
        .expect("load probe manifest");
    runtime.run(&ir).expect("MCP probe should succeed");
}

#[test]
fn mcp_stdio_probe_invokes_mock_server() {
    let mock = workspace_root().join("scripts/mcp_mock_stdio.py");
    let manifest_path = std::env::temp_dir().join("nebula-mcp-stdio-manifest.json");
    fs::write(
        &manifest_path,
        format!(
            r#"{{
  "mcp_servers": {{
    "local": {{
      "transport": "stdio",
      "command": ["python3", "{}"]
    }}
  }},
  "probes": {{
    "notify": {{
      "kind": "mcp",
      "server": "local",
      "tool": "notify"
    }}
  }}
}}"#,
            mock.display()
        ),
    )
    .expect("write manifest");

    run_notify_probe(&manifest_path);
}

#[test]
fn mcp_http_probe_invokes_mock_server() {
    let mock = workspace_root().join("scripts/mcp_mock_http.py");
    let port = 18765u16;
    let mut server = Command::new("python3")
        .arg(&mock)
        .arg(port.to_string())
        .spawn()
        .expect("spawn mock HTTP MCP server");
    thread::sleep(Duration::from_millis(300));

    let manifest_path = std::env::temp_dir().join("nebula-mcp-http-manifest.json");
    fs::write(
        &manifest_path,
        format!(
            r#"{{
  "mcp_servers": {{
    "remote": {{
      "transport": "http",
      "url": "http://127.0.0.1:{port}/mcp"
    }}
  }},
  "probes": {{
    "notify": {{
      "kind": "mcp",
      "server": "remote",
      "tool": "notify"
    }}
  }}
}}"#
        ),
    )
    .expect("write manifest");

    let result = std::panic::catch_unwind(|| run_notify_probe(&manifest_path));
    stop_child(&mut server);
    if result.is_err() {
        panic!("MCP HTTP probe test panicked");
    }
}

#[test]
fn mcp_list_tools_reports_notify_from_mock_server() {
    use nebula_mcp::McpConnectionManager;
    use nebula_runtime::list_probe_manifest;

    let mock = workspace_root().join("scripts/mcp_mock_stdio.py");
    let manifest_path = std::env::temp_dir().join("nebula-mcp-list-tools-manifest.json");
    fs::write(
        &manifest_path,
        format!(
            r#"{{
  "mcp_servers": {{
    "local": {{
      "transport": "stdio",
      "command": ["python3", "{}"]
    }}
  }},
  "probes": {{
    "notify": {{
      "kind": "mcp",
      "server": "local",
      "tool": "notify"
    }}
  }}
}}"#,
            mock.display()
        ),
    )
    .expect("write manifest");

    let report = list_probe_manifest(&manifest_path, true).expect("list manifest");
    let server = report
        .mcp_servers
        .as_ref()
        .and_then(|servers| servers.get("local"))
        .expect("local server report");
    let tools = server.tools.as_ref().expect("tools discovered");
    assert!(tools.iter().any(|tool| tool.name == "notify"));

    let manifest = nebula_runtime::read_probe_manifest(&manifest_path).expect("read manifest");
    let manager = McpConnectionManager::new(manifest.mcp_servers).expect("manager");
    let direct = manager.list_tools("local").expect("list_tools");
    assert!(direct.iter().any(|tool| tool.name == "notify"));
}

#[test]
fn mcp_manifest_unknown_server_is_rejected() {
    let manifest_path = std::env::temp_dir().join("nebula-mcp-bad-manifest.json");
    fs::write(
        &manifest_path,
        r#"{
  "mcp_servers": {
    "local": {
      "transport": "stdio",
      "command": ["python3", "-c", "pass"]
    }
  },
  "probes": {
    "notify": {
      "kind": "mcp",
      "server": "missing",
      "tool": "notify"
    }
  }
}"#,
    )
    .expect("write manifest");

    let mut host = RegistryProbeHost::with_defaults();
    let err = host
        .load_manifest(&manifest_path)
        .expect_err("unknown MCP server should fail");
    assert!(
        err.to_string().contains("missing"),
        "unexpected error: {err}"
    );
}

#[test]
fn mcp_transport_error_reports_neb_p004() {
    let manifest_path = std::env::temp_dir().join("nebula-mcp-spawn-fail-manifest.json");
    fs::write(
        &manifest_path,
        r#"{
  "mcp_servers": {
    "local": {
      "transport": "stdio",
      "command": ["__nebula_missing_mcp_server__"]
    }
  },
  "probes": {
    "notify": {
      "kind": "mcp",
      "server": "local",
      "tool": "notify"
    }
  }
}"#,
    )
    .expect("write manifest");

    let mut host = RegistryProbeHost::with_defaults();
    host.load_manifest(&manifest_path).expect("load manifest");
    let err = host
        .invoke(&ProbeInvocation {
            name: "notify",
            args: Default::default(),
        })
        .expect_err("spawn failure should fail");
    assert!(
        err.to_string().contains("NEB-P004"),
        "expected NEB-P004, got: {err}"
    );
}

fn stop_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}
