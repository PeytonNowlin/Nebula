use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::thread;

use nebula_runtime::{list_probe_manifest, ProbeHost, ProbeInvocation, RegistryProbeHost, Value};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn write_manifest(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, body).expect("write manifest");
    path
}

#[test]
fn secrets_resolve_from_env() {
    let key = "NEBULA_SECRET_RESOLVE_TEST";
    std::env::set_var(key, "resolved-token");

    let dir = std::env::temp_dir().join("nebula-secrets-tests");
    fs::create_dir_all(&dir).expect("create temp dir");
    let manifest = write_manifest(
        &dir,
        "resolve.json",
        r#"{
  "secrets": {
    "api_token": { "env": "NEBULA_SECRET_RESOLVE_TEST" }
  },
  "probes": {
    "secret_get": { "kind": "secret_get" }
  }
}"#,
    );

    let mut host = RegistryProbeHost::with_defaults();
    host.load_manifest(&manifest, None).expect("load manifest");
    let value = host
        .invoke(&ProbeInvocation {
            name: "secret_get",
            args: HashMap::from([("name".into(), Value::Str("api_token".into()))]),
        })
        .expect("secret_get");
    assert!(matches!(
        value,
        Value::Some(inner) if matches!(*inner, Value::Str(ref s) if s == "resolved-token")
    ));
}

#[test]
fn secret_get_returns_option_for_missing_name() {
    let dir = std::env::temp_dir().join("nebula-secrets-tests");
    fs::create_dir_all(&dir).expect("create temp dir");
    let manifest = write_manifest(
        &dir,
        "missing-name.json",
        r#"{
  "secrets": {
    "api_token": { "value": "inline" }
  },
  "probes": {
    "secret_get": { "kind": "secret_get" }
  }
}"#,
    );

    let mut host = RegistryProbeHost::with_defaults();
    host.load_manifest(&manifest, None).expect("load manifest");
    let missing = host
        .invoke(&ProbeInvocation {
            name: "secret_get",
            args: HashMap::from([("name".into(), Value::Str("other".into()))]),
        })
        .expect("secret_get missing");
    assert!(matches!(missing, Value::None));
}

#[test]
fn missing_secret_env_fails_at_load() {
    let dir = std::env::temp_dir().join("nebula-secrets-tests");
    fs::create_dir_all(&dir).expect("create temp dir");
    let manifest = write_manifest(
        &dir,
        "missing-env.json",
        r#"{
  "secrets": {
    "api_token": { "env": "NEBULA_SECRET_DEFINITELY_MISSING_VAR" }
  },
  "probes": {
    "secret_get": { "kind": "secret_get" }
  }
}"#,
    );

    let mut host = RegistryProbeHost::with_defaults();
    let err = host
        .load_manifest(&manifest, None)
        .expect_err("unset env should fail");
    assert!(err.to_string().contains("unset environment variable"));
}

#[test]
fn http_get_manifest_headers_are_applied() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let port = listener.local_addr().expect("local addr").port();
    let server = thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 2048];
            let n = stream.read(&mut buf).expect("read request");
            let request = String::from_utf8_lossy(&buf[..n]);
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("x-test-token: bearer inline"),
                "request missing auth header: {request}"
            );
            let body = "secret-body";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
        }
    });

    let dir = std::env::temp_dir().join("nebula-secrets-tests");
    fs::create_dir_all(&dir).expect("create temp dir");
    let manifest = write_manifest(
        &dir,
        "http-headers.json",
        r#"{
  "secrets": {
    "api_token": { "value": "inline" }
  },
  "probes": {
    "fetch": {
      "kind": "http_get",
      "headers": {
        "X-Test-Token": "Bearer ${secret:api_token}"
      }
    }
  }
}"#,
    );

    let mut host = RegistryProbeHost::with_defaults();
    host.load_manifest(&manifest, None).expect("load manifest");
    let value = host
        .invoke(&ProbeInvocation {
            name: "fetch",
            args: HashMap::from([(
                "url".into(),
                Value::Str(format!("http://127.0.0.1:{port}/")),
            )]),
        })
        .expect("http_get");
    assert!(matches!(value, Value::Str(ref s) if s == "secret-body"));
    server.join().expect("join server");
}

#[test]
fn probes_list_reports_secret_names_not_values() {
    let dir = std::env::temp_dir().join("nebula-secrets-tests");
    fs::create_dir_all(&dir).expect("create temp dir");
    let manifest = write_manifest(
        &dir,
        "list.json",
        r#"{
  "secrets": {
    "api_token": { "value": "super-secret-value" }
  },
  "probes": {
    "secret_get": { "kind": "secret_get" }
  }
}"#,
    );

    let report = list_probe_manifest(&manifest, false).expect("list manifest");
    assert_eq!(report.secrets, vec!["api_token".to_string()]);
    let payload = serde_json::to_string(&report).expect("serialize report");
    assert!(!payload.contains("super-secret-value"));
}

#[test]
fn host_overlay_wins_over_manifest_secret() {
    let dir = std::env::temp_dir().join("nebula-secrets-tests");
    fs::create_dir_all(&dir).expect("create temp dir");
    let manifest = write_manifest(
        &dir,
        "overlay.json",
        r#"{
  "secrets": {
    "api_token": { "value": "from-manifest" }
  },
  "probes": {
    "secret_get": { "kind": "secret_get" }
  }
}"#,
    );

    let overlay = HashMap::from([("api_token".to_string(), "from-overlay".to_string())]);
    let mut host = RegistryProbeHost::with_defaults();
    host.load_manifest(&manifest, Some(&overlay))
        .expect("load manifest");
    let value = host
        .invoke(&ProbeInvocation {
            name: "secret_get",
            args: HashMap::from([("name".into(), Value::Str("api_token".into()))]),
        })
        .expect("secret_get");
    assert!(matches!(
        value,
        Value::Some(inner) if matches!(*inner, Value::Str(ref s) if s == "from-overlay")
    ));
}

#[test]
fn bundle_manifest_includes_secret_get() {
    let manifest = workspace_root().join("probes/bundle.json");
    let report = list_probe_manifest(&manifest, false).expect("list bundle");
    assert!(
        report
            .probes
            .iter()
            .any(|probe| matches!(probe, nebula_runtime::DeclaredProbe::SecretGet { name } if name == "secret_get")),
        "bundle should expose secret_get"
    );
}
