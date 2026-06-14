use serde::Serialize;
use serde_json::Value as JsonValue;

use nebula_ast::DiagnosticJson;
use nebula_runtime::{ProbeCallRecord, ProbeJsonlEvent};

/// One structured record describing a single program execution.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RunRecord {
    pub program: String,
    pub exit: u8,
    pub diagnostics: Vec<DiagnosticJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telemetry_path: Option<String>,
    pub probe_events: Vec<ProbeJsonlEvent>,
    pub duration_ms: u64,
    pub printed: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_value: Option<JsonValue>,
    pub probes_called: Vec<ProbeCallRecord>,
}

impl RunRecord {
    pub fn success(
        program: String,
        telemetry_path: Option<String>,
        probe_events: Vec<ProbeJsonlEvent>,
        duration_ms: u64,
        printed: Vec<String>,
        return_value: Option<JsonValue>,
        probes_called: Vec<ProbeCallRecord>,
    ) -> Self {
        Self {
            program,
            exit: 0,
            diagnostics: Vec::new(),
            telemetry_path,
            probe_events,
            duration_ms,
            printed,
            return_value,
            probes_called,
        }
    }

    pub fn failure(
        program: String,
        diagnostics: Vec<DiagnosticJson>,
        telemetry_path: Option<String>,
        probe_events: Vec<ProbeJsonlEvent>,
        duration_ms: u64,
        printed: Vec<String>,
        probes_called: Vec<ProbeCallRecord>,
    ) -> Self {
        Self {
            program,
            exit: 1,
            diagnostics,
            telemetry_path,
            probe_events,
            duration_ms,
            printed,
            return_value: None,
            probes_called,
        }
    }
}