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