use logos::Logos;
use miette::Diagnostic;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: std::ops::Range<usize>,
}

#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\r]+")]
#[logos(skip r"--[^\n]*")]
pub enum TokenKind {
    #[token("sector")]
    Sector,
    #[token("mission")]
    Mission,
    #[token("fn")]
    Fn,
    #[token("struct")]
    Struct,
    #[token("let")]
    Let,
    #[token("mut")]
    Mut,
    #[token("set")]
    Set,
    #[token("if")]
    If,
    #[token("then")]
    Then,
    #[token("else")]
    Else,
    #[token("while")]
    While,
    #[token("do")]
    Do,
    #[token("end")]
    End,
    #[token("emit")]
    Emit,
    #[token("return")]
    Return,
    #[token("probe")]
    Probe,
    #[token("call")]
    Call,
    #[token("telemetry")]
    Telemetry,
    #[token("import")]
    Import,
    #[token("plus")]
    Plus,
    #[token("minus")]
    Minus,
    #[token("times")]
    Times,
    #[token("div")]
    Div,
    #[token("mod")]
    Mod,
    #[token("eq")]
    Eq,
    #[token("ne")]
    Ne,
    #[token("lt")]
    Lt,
    #[token("gt")]
    Gt,
    #[token("le")]
    Le,
    #[token("ge")]
    Ge,
    #[token("less")]
    Less,
    #[token("than")]
    Than,
    #[token("greater")]
    Greater,
    #[token("and")]
    And,
    #[token("or")]
    Or,
    #[token("not")]
    Not,
    #[token("true")]
    True,
    #[token("false")]
    False,
    #[token("Some")]
    Some,
    #[token("None")]
    None,
    #[token("Int")]
    TyInt,
    #[token("Float")]
    TyFloat,
    #[token("Bool")]
    TyBool,
    #[token("Str")]
    TyStr,
    #[token("Void")]
    TyVoid,
    #[token("List")]
    TyList,
    #[token("Map")]
    TyMap,
    #[token("Option")]
    TyOption,

    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token(",")]
    Comma,
    #[token(":")]
    Colon,
    #[token("=")]
    Assign,
    #[token(";")]
    Semi,
    #[token(".")]
    Dot,
    #[token("->")]
    Arrow,
    #[token("<")]
    LtAngle,
    #[token(">")]
    GtAngle,

    #[regex(r"[0-9]+", |lex| lex.slice().parse::<i64>().ok())]
    IntLit(i64),
    #[regex(r"[0-9]+\.[0-9]+", |lex| lex.slice().parse::<f64>().ok())]
    FloatLit(f64),
    #[regex(r#""([^"\\]|\\.)*""#, parse_string)]
    StrLit(String),
    #[regex(r"[A-Za-z_][A-Za-z0-9_]*", |lex| lex.slice().to_string())]
    Ident(String),

    #[regex(r"\n+")]
    Newline,
}

fn parse_string(lex: &mut logos::Lexer<TokenKind>) -> Option<String> {
    let s = lex.slice();
    let inner = &s[1..s.len() - 1];
    let mut out = String::new();
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next()? {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                'n' => out.push('\n'),
                't' => out.push('\t'),
                _ => return None,
            }
        } else {
            out.push(c);
        }
    }
    Some(out)
}

#[derive(Debug, Error, Diagnostic)]
#[error("NEB-S001 [lex_error] unexpected character")]
#[diagnostic(code(nebula::lex_error))]
pub struct LexError {
    pub span: std::ops::Range<usize>,
}

pub fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    let mut lexer = TokenKind::lexer(source);
    let mut tokens = Vec::new();

    while let Some(result) = lexer.next() {
        match result {
            Ok(kind) => {
                if matches!(kind, TokenKind::Newline) {
                    continue;
                }
                tokens.push(Token {
                    kind,
                    span: lexer.span(),
                });
            }
            Err(_) => {
                return Err(LexError {
                    span: lexer.span(),
                });
            }
        }
    }

    Ok(tokens)
}