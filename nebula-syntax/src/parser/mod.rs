mod error;
mod expr;
mod stmt;

pub use error::ParseError;

use nebula_ast::*;

use crate::lexer::{Token, TokenKind};

pub fn parse(source: &str) -> Result<Program, ParseError> {
    let tokens = super::lexer::lex(source)?;
    let mut parser = Parser::new(tokens);
    parser.parse_program()
}

pub(super) struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<Token> {
        let tok = self.tokens.get(self.pos).cloned();
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    fn expect(&mut self, expected: TokenKind, label: &str) -> Result<Token, ParseError> {
        match self.peek() {
            Some(tok) if tok.kind == expected => Ok(self.advance().unwrap()),
            Some(tok) => Err(ParseError::Unexpected {
                expected: label.into(),
                found: format!("{:?}", tok.kind),
                span: tok.span.clone(),
            }),
            None => Err(ParseError::Eof { span: self.eof_span() }),
        }
    }

    pub(super) fn eof_span(&self) -> Span {
        if let Some(last) = self.tokens.last() {
            let end = last.span.end;
            end..end
        } else {
            0..0
        }
    }

    fn match_kind(&mut self, kind: TokenKind) -> Option<Token> {
        if self.peek().map(|t| &t.kind) == Some(&kind) {
            self.advance()
        } else {
            None
        }
    }

    fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut items = Vec::new();
        while self.peek().is_some() {
            items.push(self.parse_top_level()?);
        }
        Ok(Program { items })
    }

    fn parse_top_level(&mut self) -> Result<Spanned<TopLevel>, ParseError> {
        let start = self.peek().map(|t| t.span.start).unwrap_or(0);
        let item = match self.peek().map(|t| &t.kind) {
            Some(TokenKind::Import) => {
                self.advance();
                let path = self.parse_string_lit()?;
                let _ = self.match_kind(TokenKind::Semi);
                TopLevel::Import(path)
            }
            Some(TokenKind::Sector) => TopLevel::Sector(self.parse_sector()?),
            Some(TokenKind::Mission) => TopLevel::Mission(self.parse_mission()?),
            Some(tok) => {
                return Err(ParseError::Unexpected {
                    expected: "import, sector, or mission".into(),
                    found: format!("{:?}", tok),
                    span: self.peek().unwrap().span.clone(),
                });
            }
            None => return Err(ParseError::Eof { span: self.eof_span() }),
        };
        let end = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span.end)
            .unwrap_or(start);
        Ok(Spanned::new(item, start..end))
    }

    fn parse_sector(&mut self) -> Result<Spanned<Sector>, ParseError> {
        let start = self.expect(TokenKind::Sector, "sector")?.span.start;
        let name = self.parse_ident()?;
        self.expect(TokenKind::LBrace, "{")?;
        let mut items = Vec::new();
        while self.peek().map(|t| &t.kind) != Some(&TokenKind::RBrace) {
            items.push(self.parse_sector_item()?);
        }
        let end = self.expect(TokenKind::RBrace, "}")?.span.end;
        Ok(Spanned::new(Sector { name, items }, start..end))
    }

    fn parse_sector_item(&mut self) -> Result<SectorItem, ParseError> {
        match self.peek().map(|t| &t.kind) {
            Some(TokenKind::Fn) => Ok(SectorItem::Fn(self.parse_fn_decl()?)),
            Some(TokenKind::Struct) => Ok(SectorItem::Struct(self.parse_struct_decl()?)),
            Some(TokenKind::Probe) => Ok(SectorItem::Probe(self.parse_probe_decl()?)),
            Some(tok) => Err(ParseError::Unexpected {
                expected: "fn, struct, or probe".into(),
                found: format!("{:?}", tok),
                span: self.peek().unwrap().span.clone(),
            }),
            None => Err(ParseError::Eof { span: self.eof_span() }),
        }
    }

    fn parse_mission(&mut self) -> Result<Spanned<Mission>, ParseError> {
        let start = self.expect(TokenKind::Mission, "mission")?.span.start;
        let name = self.parse_ident()?;
        self.expect(TokenKind::LBrace, "{")?;
        let mut items = Vec::new();
        while self.peek().map(|t| &t.kind) != Some(&TokenKind::RBrace) {
            items.push(self.parse_mission_item()?);
        }
        let end = self.expect(TokenKind::RBrace, "}")?.span.end;
        Ok(Spanned::new(Mission { name, items }, start..end))
    }

    fn parse_mission_item(&mut self) -> Result<MissionItem, ParseError> {
        if self.peek().map(|t| &t.kind) == Some(&TokenKind::Probe) {
            Ok(MissionItem::Probe(self.parse_probe_decl()?))
        } else {
            Ok(MissionItem::Stmt(self.parse_stmt()?))
        }
    }

    fn parse_fn_decl(&mut self) -> Result<Spanned<FnDecl>, ParseError> {
        let start = self.expect(TokenKind::Fn, "fn")?.span.start;
        let name = self.parse_ident()?;
        self.expect(TokenKind::LParen, "(")?;
        let params = self.parse_param_list()?;
        self.expect(TokenKind::RParen, ")")?;
        self.expect(TokenKind::Arrow, "->")?;
        let return_type = self.parse_type()?;
        self.expect(TokenKind::LBrace, "{")?;
        let mut body = Vec::new();
        while self.peek().map(|t| &t.kind) != Some(&TokenKind::RBrace) {
            body.push(self.parse_stmt()?);
        }
        let end = self.expect(TokenKind::RBrace, "}")?.span.end;
        Ok(Spanned::new(
            FnDecl {
                name,
                params,
                return_type,
                body,
            },
            start..end,
        ))
    }

    fn parse_struct_decl(&mut self) -> Result<Spanned<StructDecl>, ParseError> {
        let start = self.expect(TokenKind::Struct, "struct")?.span.start;
        let name = self.parse_ident()?;
        self.expect(TokenKind::LBrace, "{")?;
        let mut fields = Vec::new();
        while self.peek().map(|t| &t.kind) != Some(&TokenKind::RBrace) {
            let fname = self.parse_ident()?;
            self.expect(TokenKind::Colon, ":")?;
            let ty = self.parse_type()?;
            self.expect(TokenKind::Semi, ";")?;
            let fspan = fname.span.start..ty.span.end;
            fields.push(Spanned::new(FieldDecl { name: fname, ty }, fspan));
        }
        let end = self.expect(TokenKind::RBrace, "}")?.span.end;
        Ok(Spanned::new(StructDecl { name, fields }, start..end))
    }

    fn parse_probe_decl(&mut self) -> Result<Spanned<ProbeDecl>, ParseError> {
        let start = self.expect(TokenKind::Probe, "probe")?.span.start;
        let name = self.parse_ident()?;
        self.expect(TokenKind::LParen, "(")?;
        let params = self.parse_param_list()?;
        self.expect(TokenKind::RParen, ")")?;
        self.expect(TokenKind::Arrow, "->")?;
        let return_type = self.parse_type()?;
        let end = self.expect(TokenKind::Semi, ";")?.span.end;
        Ok(Spanned::new(
            ProbeDecl {
                name,
                params,
                return_type,
            },
            start..end,
        ))
    }

    fn parse_param_list(&mut self) -> Result<Vec<Spanned<Param>>, ParseError> {
        let mut params = Vec::new();
        if self.peek().map(|t| &t.kind) == Some(&TokenKind::RParen) {
            return Ok(params);
        }
        loop {
            let pname = self.parse_ident()?;
            self.expect(TokenKind::Colon, ":")?;
            let ty = self.parse_type()?;
            let span = pname.span.start..ty.span.end;
            params.push(Spanned::new(Param { name: pname, ty }, span));
            if self.match_kind(TokenKind::Comma).is_none() {
                break;
            }
        }
        Ok(params)
    }

    fn parse_type(&mut self) -> Result<Spanned<Type>, ParseError> {
        let start = self.peek().map(|t| t.span.start).unwrap_or(0);
        let ty = match self.peek().map(|t| &t.kind) {
            Some(TokenKind::TyInt) => {
                self.advance();
                Type::Int
            }
            Some(TokenKind::TyFloat) => {
                self.advance();
                Type::Float
            }
            Some(TokenKind::TyBool) => {
                self.advance();
                Type::Bool
            }
            Some(TokenKind::TyStr) => {
                self.advance();
                Type::Str
            }
            Some(TokenKind::TyVoid) => {
                self.advance();
                Type::Void
            }
            Some(TokenKind::TyList) => {
                self.advance();
                self.expect(TokenKind::LtAngle, "<")?;
                let inner = self.parse_type()?;
                self.expect(TokenKind::GtAngle, ">")?;
                Type::List(Box::new(inner.node))
            }
            Some(TokenKind::TyMap) => {
                self.advance();
                self.expect(TokenKind::LtAngle, "<")?;
                let key = self.parse_type()?;
                self.expect(TokenKind::Comma, ",")?;
                let val = self.parse_type()?;
                self.expect(TokenKind::GtAngle, ">")?;
                Type::Map(Box::new(key.node), Box::new(val.node))
            }
            Some(TokenKind::TyOption) => {
                self.advance();
                self.expect(TokenKind::LtAngle, "<")?;
                let inner = self.parse_type()?;
                self.expect(TokenKind::GtAngle, ">")?;
                Type::Option(Box::new(inner.node))
            }
            Some(TokenKind::Fn) => {
                self.advance();
                self.expect(TokenKind::LParen, "(")?;
                let mut params = Vec::new();
                if self.peek().map(|t| &t.kind) != Some(&TokenKind::RParen) {
                    loop {
                        params.push(self.parse_type()?.node);
                        if self.match_kind(TokenKind::Comma).is_none() {
                            break;
                        }
                    }
                }
                self.expect(TokenKind::RParen, ")")?;
                self.expect(TokenKind::Arrow, "->")?;
                let ret = self.parse_type()?;
                Type::Fn(params, Box::new(ret.node))
            }
            Some(TokenKind::Ident(_)) => {
                let first = if let Some(Token { kind: TokenKind::Ident(n), .. }) = self.advance() {
                    n
                } else {
                    unreachable!()
                };
                if self.peek().map(|t| &t.kind) == Some(&TokenKind::Dot) {
                    self.advance();
                    let second = self.parse_ident()?;
                    Type::Named(format!("{first}.{}", second.node))
                } else {
                    Type::Named(first)
                }
            }
            Some(tok) => {
                return Err(ParseError::Unexpected {
                    expected: "type".into(),
                    found: format!("{:?}", tok),
                    span: self.peek().unwrap().span.clone(),
                });
            }
            None => return Err(ParseError::Eof { span: self.eof_span() }),
        };
        let end = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span.end)
            .unwrap_or(start);
        Ok(Spanned::new(ty, start..end))
    }

    pub(super) fn parse_qualifiable_name(&mut self) -> Result<Spanned<String>, ParseError> {
        let first = self.parse_ident()?;
        if self.peek().map(|t| &t.kind) == Some(&TokenKind::Dot) {
            self.advance();
            let second = self.parse_ident()?;
            let span = first.span.start..second.span.end;
            Ok(Spanned::new(
                format!("{}.{}", first.node, second.node),
                span,
            ))
        } else {
            Ok(first)
        }
    }

    pub(super) fn parse_ident(&mut self) -> Result<Spanned<String>, ParseError> {
        match self.advance() {
            Some(Token {
                kind: TokenKind::Ident(name),
                span,
            }) => Ok(Spanned::new(name, span)),
            Some(tok) => Err(ParseError::Unexpected {
                expected: "identifier".into(),
                found: format!("{:?}", tok.kind),
                span: tok.span,
            }),
            None => Err(ParseError::Eof { span: self.eof_span() }),
        }
    }

    pub(super) fn parse_named_arg_list(&mut self) -> Result<Vec<Spanned<NamedArg>>, ParseError> {
        let mut args = Vec::new();
        if self.peek().map(|t| &t.kind) == Some(&TokenKind::RParen) {
            return Ok(args);
        }
        loop {
            let name = self.parse_ident()?;
            self.expect(TokenKind::Colon, ":")?;
            let value = self.parse_expr()?;
            let span = name.span.start..value.span.end;
            args.push(Spanned::new(NamedArg { name, value }, span));
            if self.match_kind(TokenKind::Comma).is_none() {
                break;
            }
        }
        Ok(args)
    }

    fn parse_string_lit(&mut self) -> Result<Spanned<String>, ParseError> {
        match self.advance() {
            Some(Token {
                kind: TokenKind::StrLit(s),
                span,
            }) => Ok(Spanned::new(s, span)),
            Some(tok) => Err(ParseError::Unexpected {
                expected: "string literal".into(),
                found: format!("{:?}", tok.kind),
                span: tok.span,
            }),
            None => Err(ParseError::Eof { span: self.eof_span() }),
        }
    }
}