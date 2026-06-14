use crate::diagnostic_json::{make_span, DiagnosticJson};

/// Structured Nebula error metadata for JSON diagnostics and agent tooling.
pub trait NebError {
    fn neb_code(&self) -> &'static str;
    /// Human-readable detail without the `NEB-XXX [tag]` prefix.
    fn neb_message(&self) -> String;
    /// Primary source span when the error is tied to source text.
    fn neb_span(&self) -> Option<crate::Span>;

    fn to_diagnostic_json(&self, file: Option<&str>, source: Option<&str>) -> DiagnosticJson {
        DiagnosticJson {
            code: self.neb_code().to_string(),
            span: self.neb_span().map(|span| make_span(file, &span, source)),
            message: self.neb_message(),
        }
    }
}

/// Map a miette `#[diagnostic(code(...))]` identifier to a stable `NEB-*` code.
pub fn neb_code_from_miette(code: &str) -> Option<&'static str> {
    Some(match code {
        "nebula::lex_error" => "NEB-S001",
        "nebula::parse_error" => "NEB-S002",
        "nebula::eof" => "NEB-S003",
        "nebula::deprecated_cmp_op" => "NEB-S004",
        "nebula::deprecated_brace_block" => "NEB-S005",
        "nebula::undefined_ident" => "NEB-T001",
        "nebula::type_mismatch" => "NEB-T002",
        "nebula::immutable_assign" => "NEB-T003",
        "nebula::undefined_fn" => "NEB-T004",
        "nebula::undefined_struct" => "NEB-T005",
        "nebula::undefined_probe" => "NEB-T006",
        "nebula::missing_main" => "NEB-T007",
        "nebula::unknown_field" => "NEB-T008",
        "nebula::duplicate_symbol" => "NEB-T009",
        "nebula::typecheck_failed" => return None,
        "nebula::import_not_found" => "NEB-L001",
        "nebula::circular_import" => "NEB-L002",
        "nebula::import_duplicate_symbol" => "NEB-L003",
        "nebula::library_has_mission" => "NEB-L004",
        "nebula::import_read_error" => "NEB-L005",
        "nebula::import_parse_error" => "NEB-L006",
        "nebula::ir_error" => "NEB-R001",
        "nebula::runtime_error" => "NEB-R002",
        "nebula::undefined_var" => "NEB-R003",
        "nebula::divide_by_zero" => "NEB-R004",
        "nebula::index_out_of_bounds" => "NEB-R005",
        "nebula::key_not_found" => "NEB-R006",
        "nebula::integer_overflow" => "NEB-R007",
        "nebula::execution_timeout" => "NEB-R008",
        "nebula::loop_iteration_limit" => "NEB-R009",
        "nebula::memory_limit_exceeded" => "NEB-R010",
        "nebula::probe_not_implemented" => "NEB-P002",
        "nebula::probe_failed" => "NEB-P003",
        "nebula::mcp_transport" => "NEB-P004",
        _ => return None,
    })
}

impl<E: NebError + ?Sized> NebError for &E {
    fn neb_code(&self) -> &'static str {
        (*self).neb_code()
    }

    fn neb_message(&self) -> String {
        (*self).neb_message()
    }

    fn neb_span(&self) -> Option<crate::Span> {
        (*self).neb_span()
    }
}
