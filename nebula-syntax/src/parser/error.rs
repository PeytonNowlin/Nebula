use miette::Diagnostic;
use nebula_ast::{NebError, Span};
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
pub enum ParseError {
    #[error(transparent)]
    Lex(#[from] super::super::lexer::LexError),
    #[error("NEB-S002 [parse_error] unexpected token: expected {expected}, found {found}")]
    #[diagnostic(code(nebula::parse_error))]
    Unexpected {
        expected: String,
        found: String,
        span: Span,
    },
    #[error("NEB-S003 [parse_error] unexpected end of file")]
    #[diagnostic(code(nebula::eof))]
    Eof { span: Span },

    #[error("NEB-S004 [parse_error] use `{canonical}` instead of `{found}`")]
    #[diagnostic(code(nebula::deprecated_cmp_op))]
    DeprecatedComparison {
        canonical: String,
        found: String,
        span: Span,
    },

    #[error("NEB-S005 [parse_error] use `{canonical}`-delimited block instead of `{found}`")]
    #[diagnostic(code(nebula::deprecated_brace_block))]
    DeprecatedBraceBlock {
        canonical: String,
        found: String,
        span: Span,
    },
}

impl NebError for ParseError {
    fn neb_code(&self) -> &'static str {
        match self {
            ParseError::Lex(err) => err.neb_code(),
            ParseError::Unexpected { .. } => "NEB-S002",
            ParseError::Eof { .. } => "NEB-S003",
            ParseError::DeprecatedComparison { .. } => "NEB-S004",
            ParseError::DeprecatedBraceBlock { .. } => "NEB-S005",
        }
    }

    fn neb_message(&self) -> String {
        match self {
            ParseError::Lex(err) => err.neb_message(),
            ParseError::Unexpected { expected, found, .. } => {
                format!("unexpected token: expected {expected}, found {found}")
            }
            ParseError::Eof { .. } => "unexpected end of file".to_string(),
            ParseError::DeprecatedComparison { canonical, found, .. } => {
                format!("use `{canonical}` instead of `{found}`")
            }
            ParseError::DeprecatedBraceBlock { canonical, found, .. } => {
                format!("use `{canonical}`-delimited block instead of `{found}`")
            }
        }
    }

    fn neb_span(&self) -> Option<Span> {
        match self {
            ParseError::Lex(err) => err.neb_span(),
            ParseError::Unexpected { span, .. }
            | ParseError::Eof { span }
            | ParseError::DeprecatedComparison { span, .. }
            | ParseError::DeprecatedBraceBlock { span, .. } => Some(span.clone()),
        }
    }
}