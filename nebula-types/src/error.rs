use miette::Diagnostic;
use nebula_ast::Span;
use thiserror::Error;

#[derive(Debug, Clone, Error, Diagnostic)]
pub enum TypeError {
    #[error("NEB-T001 [type_error] undefined identifier `{name}`")]
    #[diagnostic(code(nebula::undefined_ident))]
    UndefinedIdent {
        name: String,
        #[label("undefined identifier")]
        span: Span,
    },

    #[error("NEB-T002 [type_error] type mismatch: expected {expected}, found {found}")]
    #[diagnostic(code(nebula::type_mismatch))]
    Mismatch {
        expected: String,
        found: String,
        #[label("type mismatch")]
        span: Span,
    },

    #[error("NEB-T003 [type_error] cannot assign to immutable binding `{name}`")]
    #[diagnostic(code(nebula::immutable_assign))]
    ImmutableAssign {
        name: String,
        #[label("immutable binding")]
        span: Span,
    },

    #[error("NEB-T004 [type_error] undefined function `{name}`")]
    #[diagnostic(code(nebula::undefined_fn))]
    UndefinedFn {
        name: String,
        #[label("undefined function")]
        span: Span,
    },

    #[error("NEB-T005 [type_error] undefined struct `{name}`")]
    #[diagnostic(code(nebula::undefined_struct))]
    UndefinedStruct {
        name: String,
        #[label("undefined struct")]
        span: Span,
    },

    #[error("NEB-T006 [type_error] undefined probe `{name}`")]
    #[diagnostic(code(nebula::undefined_probe))]
    UndefinedProbe {
        name: String,
        #[label("undefined probe")]
        span: Span,
    },

    #[error("NEB-T007 [type_error] missing mission entry point `main`")]
    #[diagnostic(code(nebula::missing_main))]
    MissingMain {
        #[label("program root")]
        span: Span,
    },

    #[error("NEB-T008 [type_error] unknown field `{field}` on struct `{struct_name}`")]
    #[diagnostic(code(nebula::unknown_field))]
    UnknownField {
        struct_name: String,
        field: String,
        #[label("unknown field")]
        span: Span,
    },

    #[error("NEB-T009 [type_error] duplicate {kind} `{name}`")]
    #[diagnostic(code(nebula::duplicate_symbol))]
    DuplicateSymbol {
        kind: String,
        name: String,
        #[label("duplicate symbol")]
        span: Span,
    },
}

impl nebula_ast::NebError for TypeError {
    fn neb_code(&self) -> &'static str {
        match self {
            TypeError::UndefinedIdent { .. } => "NEB-T001",
            TypeError::Mismatch { .. } => "NEB-T002",
            TypeError::ImmutableAssign { .. } => "NEB-T003",
            TypeError::UndefinedFn { .. } => "NEB-T004",
            TypeError::UndefinedStruct { .. } => "NEB-T005",
            TypeError::UndefinedProbe { .. } => "NEB-T006",
            TypeError::MissingMain { .. } => "NEB-T007",
            TypeError::UnknownField { .. } => "NEB-T008",
            TypeError::DuplicateSymbol { .. } => "NEB-T009",
        }
    }

    fn neb_message(&self) -> String {
        match self {
            TypeError::UndefinedIdent { name, .. } => {
                format!("undefined identifier `{name}`")
            }
            TypeError::Mismatch { expected, found, .. } => {
                format!("type mismatch: expected {expected}, found {found}")
            }
            TypeError::ImmutableAssign { name, .. } => {
                format!("cannot assign to immutable binding `{name}`")
            }
            TypeError::UndefinedFn { name, .. } => format!("undefined function `{name}`"),
            TypeError::UndefinedStruct { name, .. } => format!("undefined struct `{name}`"),
            TypeError::UndefinedProbe { name, .. } => format!("undefined probe `{name}`"),
            TypeError::MissingMain { .. } => "missing mission entry point `main`".to_string(),
            TypeError::UnknownField {
                struct_name,
                field,
                ..
            } => format!("unknown field `{field}` on struct `{struct_name}`"),
            TypeError::DuplicateSymbol { kind, name, .. } => {
                format!("duplicate {kind} `{name}`")
            }
        }
    }

    fn neb_span(&self) -> Option<Span> {
        Some(match self {
            TypeError::UndefinedIdent { span, .. }
            | TypeError::Mismatch { span, .. }
            | TypeError::ImmutableAssign { span, .. }
            | TypeError::UndefinedFn { span, .. }
            | TypeError::UndefinedStruct { span, .. }
            | TypeError::UndefinedProbe { span, .. }
            | TypeError::MissingMain { span }
            | TypeError::UnknownField { span, .. }
            | TypeError::DuplicateSymbol { span, .. } => span.clone(),
        })
    }
}