use nebula_ast::*;
use miette::Diagnostic;
use thiserror::Error;

use crate::lexer::{Token, TokenKind};

#[derive(Debug, Error, Diagnostic)]
pub enum ParseError {
    #[error(transparent)]
    Lex(#[from] super::lexer::LexError),
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
}

pub fn parse(source: &str) -> Result<Program, ParseError> {
    let tokens = super::lexer::lex(source)?;
    let mut parser = Parser::new(tokens);
    parser.parse_program()
}

struct Parser {
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

    fn eof_span(&self) -> Span {
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

    fn parse_stmt(&mut self) -> Result<Spanned<Stmt>, ParseError> {
        let start = self.peek().map(|t| t.span.start).unwrap_or(0);
        let stmt = match self.peek().map(|t| &t.kind) {
            Some(TokenKind::Let) => self.parse_let_stmt()?,
            Some(TokenKind::Set) => self.parse_set_stmt()?,
            Some(TokenKind::If) => self.parse_if_stmt()?,
            Some(TokenKind::While) => self.parse_while_stmt()?,
            Some(TokenKind::Emit) => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(TokenKind::Semi, ";")?;
                Stmt::Emit(expr)
            }
            Some(TokenKind::Return) => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(TokenKind::Semi, ";")?;
                Stmt::Return(expr)
            }
            Some(TokenKind::Call) => self.parse_call_stmt()?,
            Some(TokenKind::Telemetry) => self.parse_telemetry_stmt()?,
            _ => {
                let expr = self.parse_expr()?;
                self.expect(TokenKind::Semi, ";")?;
                Stmt::Expr(expr)
            }
        };
        let end = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span.end)
            .unwrap_or(start);
        Ok(Spanned::new(stmt, start..end))
    }

    fn parse_let_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance();
        let mutable = self.match_kind(TokenKind::Mut).is_some();
        let name = self.parse_ident()?;
        self.expect(TokenKind::Colon, ":")?;
        let ty = self.parse_type()?;
        self.expect(TokenKind::Assign, "=")?;
        let value = self.parse_expr()?;
        self.expect(TokenKind::Semi, ";")?;
        Ok(Stmt::Let {
            mutable,
            name,
            ty,
            value,
        })
    }

    fn parse_set_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance();
        let name = self.parse_ident()?;
        self.expect(TokenKind::Assign, "=")?;
        let value = self.parse_expr()?;
        self.expect(TokenKind::Semi, ";")?;
        Ok(Stmt::Set { name, value })
    }

    fn parse_if_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance();
        let condition = self.parse_expr()?;
        self.expect(TokenKind::Then, "then")?;
        let (then_block, then_brace) =
            self.parse_block(&[TokenKind::Else, TokenKind::End])?;
        let (else_block, else_brace) = if self.match_kind(TokenKind::Else).is_some() {
            let (block, brace) = self.parse_block(&[TokenKind::End])?;
            (Some(block), brace)
        } else {
            (None, true)
        };

        if !then_brace || else_block.is_some() && !else_brace {
            self.expect(TokenKind::End, "end")?;
        }

        Ok(Stmt::If {
            condition,
            then_block,
            else_block,
        })
    }

    fn parse_while_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance();
        let condition = self.parse_expr()?;
        self.expect(TokenKind::Do, "do")?;
        let (body, brace_style) = self.parse_block(&[TokenKind::End])?;
        if !brace_style {
            self.expect(TokenKind::End, "end")?;
        }
        Ok(Stmt::While { condition, body })
    }

    fn parse_call_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance();
        let name = self.parse_qualifiable_name()?;
        self.expect(TokenKind::LParen, "(")?;
        let args = self.parse_named_arg_list()?;
        self.expect(TokenKind::RParen, ")")?;
        self.expect(TokenKind::Semi, ";")?;
        Ok(Stmt::Call { name, args })
    }

    fn parse_telemetry_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance();
        let (body, brace_style) = self.parse_block(&[TokenKind::End])?;
        if !brace_style {
            self.expect(TokenKind::End, "end")?;
        }
        Ok(Stmt::Telemetry { body })
    }

    fn parse_block(
        &mut self,
        terminators: &[TokenKind],
    ) -> Result<(Vec<Spanned<Stmt>>, bool), ParseError> {
        if self.peek().map(|t| &t.kind) == Some(&TokenKind::LBrace) {
            self.advance();
            let mut stmts = Vec::new();
            while self.peek().map(|t| &t.kind) != Some(&TokenKind::RBrace) {
                stmts.push(self.parse_stmt()?);
            }
            self.expect(TokenKind::RBrace, "}")?;
            Ok((stmts, true))
        } else {
            Ok((self.parse_end_block(terminators)?, false))
        }
    }

    fn parse_end_block(
        &mut self,
        terminators: &[TokenKind],
    ) -> Result<Vec<Spanned<Stmt>>, ParseError> {
        let mut stmts = Vec::new();
        loop {
            match self.peek().map(|t| t.kind.clone()) {
                Some(kind) if terminators.contains(&kind) => break,
                Some(_) => stmts.push(self.parse_stmt()?),
                None => {
                    return Err(ParseError::Eof {
                        span: self.eof_span(),
                    });
                }
            }
        }
        Ok(stmts)
    }

    fn parse_named_arg_list(&mut self) -> Result<Vec<Spanned<NamedArg>>, ParseError> {
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

    fn parse_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        self.parse_or_expr()
    }

    fn parse_or_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let mut left = self.parse_and_expr()?;
        while self.match_kind(TokenKind::Or).is_some() {
            let right = self.parse_and_expr()?;
            let span = left.span.start..right.span.end;
            left = Spanned::new(
                Expr::Binary {
                    left: Box::new(left),
                    op: BinaryOp::Or,
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    fn parse_and_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let mut left = self.parse_cmp_expr()?;
        while self.match_kind(TokenKind::And).is_some() {
            let right = self.parse_cmp_expr()?;
            let span = left.span.start..right.span.end;
            left = Spanned::new(
                Expr::Binary {
                    left: Box::new(left),
                    op: BinaryOp::And,
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    fn try_consume_cmp_op(&mut self) -> Option<BinaryOp> {
        match self.peek().map(|t| &t.kind) {
            Some(TokenKind::Eq) => {
                self.advance();
                Some(BinaryOp::Eq)
            }
            Some(TokenKind::Ne) => {
                self.advance();
                Some(BinaryOp::Ne)
            }
            Some(TokenKind::Lt) => {
                self.advance();
                Some(BinaryOp::Lt)
            }
            Some(TokenKind::Gt) => {
                self.advance();
                Some(BinaryOp::Gt)
            }
            Some(TokenKind::Le) => {
                self.advance();
                Some(BinaryOp::Le)
            }
            Some(TokenKind::Ge) => {
                self.advance();
                Some(BinaryOp::Ge)
            }
            Some(TokenKind::Less) if self.tokens.get(self.pos + 1).map(|t| &t.kind) == Some(&TokenKind::Than) => {
                self.advance();
                self.advance();
                Some(BinaryOp::Lt)
            }
            Some(TokenKind::Greater) if self.tokens.get(self.pos + 1).map(|t| &t.kind) == Some(&TokenKind::Than) => {
                self.advance();
                self.advance();
                Some(BinaryOp::Gt)
            }
            _ => None,
        }
    }

    fn parse_cmp_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let mut left = self.parse_add_expr()?;
        while let Some(op) = self.try_consume_cmp_op() {
            let right = self.parse_add_expr()?;
            let span = left.span.start..right.span.end;
            left = Spanned::new(
                Expr::Binary {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    fn parse_add_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let mut left = self.parse_mul_expr()?;
        loop {
            let op = match self.peek().map(|t| &t.kind) {
                Some(TokenKind::Plus) => Some(BinaryOp::Plus),
                Some(TokenKind::Minus) => Some(BinaryOp::Minus),
                _ => None,
            };
            match op {
                Some(op) => {
                    self.advance();
                    let right = self.parse_mul_expr()?;
                    let span = left.span.start..right.span.end;
                    left = Spanned::new(
                        Expr::Binary {
                            left: Box::new(left),
                            op,
                            right: Box::new(right),
                        },
                        span,
                    );
                }
                None => break,
            }
        }
        Ok(left)
    }

    fn parse_mul_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let mut left = self.parse_unary_expr()?;
        loop {
            let op = match self.peek().map(|t| &t.kind) {
                Some(TokenKind::Times) => Some(BinaryOp::Times),
                Some(TokenKind::Div) => Some(BinaryOp::Div),
                Some(TokenKind::Mod) => Some(BinaryOp::Mod),
                _ => None,
            };
            match op {
                Some(op) => {
                    self.advance();
                    let right = self.parse_unary_expr()?;
                    let span = left.span.start..right.span.end;
                    left = Spanned::new(
                        Expr::Binary {
                            left: Box::new(left),
                            op,
                            right: Box::new(right),
                        },
                        span,
                    );
                }
                None => break,
            }
        }
        Ok(left)
    }

    fn parse_unary_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        if self.peek().map(|t| &t.kind) == Some(&TokenKind::Not) {
            let start = self.advance().unwrap().span.start;
            let operand = self.parse_unary_expr()?;
            let span = start..operand.span.end;
            return Ok(Spanned::new(
                Expr::Unary {
                    op: UnaryOp::Not,
                    operand: Box::new(operand),
                },
                span,
            ));
        }
        self.parse_postfix_expr()
    }

    fn parse_postfix_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let mut expr = self.parse_primary_expr()?;

        loop {
            if self.peek().map(|t| &t.kind) != Some(&TokenKind::Dot) {
                break;
            }

            let _dot = self.advance().unwrap();
            let member = self.parse_ident()?;

            if self.peek().map(|t| &t.kind) == Some(&TokenKind::LParen) {
                if let Some(base) = qual_name_from_expr(&expr.node) {
                    self.advance();
                    let mut args = Vec::new();
                    if self.peek().map(|t| &t.kind) != Some(&TokenKind::RParen) {
                        loop {
                            args.push(self.parse_expr()?);
                            if self.match_kind(TokenKind::Comma).is_none() {
                                break;
                            }
                        }
                    }
                    let end = self.expect(TokenKind::RParen, ")")?.span.end;
                    let qualified = format!("{base}.{}", member.node);
                    let span = expr.span.start..end;
                    expr = Spanned::new(
                        Expr::Call {
                            callee: Spanned::new(qualified, span.clone()),
                            args,
                        },
                        span,
                    );
                    continue;
                }
            }

            if self.peek().map(|t| &t.kind) == Some(&TokenKind::LBrace) {
                if let Some(base) = qual_name_from_expr(&expr.node) {
                    self.advance();
                    let mut fields = Vec::new();
                    if self.peek().map(|t| &t.kind) != Some(&TokenKind::RBrace) {
                        loop {
                            let fname = self.parse_ident()?;
                            self.expect(TokenKind::Colon, ":")?;
                            let value = self.parse_expr()?;
                            let span = fname.span.start..value.span.end;
                            fields.push(Spanned::new(FieldInit { name: fname, value }, span));
                            if self.match_kind(TokenKind::Comma).is_none() {
                                break;
                            }
                        }
                    }
                    let end = self.expect(TokenKind::RBrace, "}")?.span.end;
                    let qualified = format!("{base}.{}", member.node);
                    let span = expr.span.start..end;
                    expr = Spanned::new(
                        Expr::StructLit {
                            name: Spanned::new(qualified, span.clone()),
                            fields,
                        },
                        span,
                    );
                    continue;
                }
            }

            let span = expr.span.start..member.span.end;
            expr = Spanned::new(
                Expr::FieldAccess {
                    object: Box::new(expr),
                    field: member,
                },
                span,
            );
        }

        Ok(expr)
    }

    fn parse_primary_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let tok = self.peek().cloned().ok_or(ParseError::Eof {
            span: self.eof_span(),
        })?;
        let start = tok.span.start;

        match tok.kind {
            TokenKind::IntLit(n) => {
                self.advance();
                Ok(Spanned::new(Expr::Int(n), tok.span))
            }
            TokenKind::FloatLit(n) => {
                self.advance();
                Ok(Spanned::new(Expr::Float(n), tok.span))
            }
            TokenKind::StrLit(s) => {
                self.advance();
                Ok(Spanned::new(Expr::Str(s), tok.span))
            }
            TokenKind::True => {
                self.advance();
                Ok(Spanned::new(Expr::Bool(true), tok.span))
            }
            TokenKind::False => {
                self.advance();
                Ok(Spanned::new(Expr::Bool(false), tok.span))
            }
            TokenKind::None => {
                self.advance();
                Ok(Spanned::new(Expr::None, tok.span))
            }
            TokenKind::Some => {
                self.advance();
                self.expect(TokenKind::LParen, "(")?;
                let inner = self.parse_expr()?;
                let end = self.expect(TokenKind::RParen, ")")?.span.end;
                Ok(Spanned::new(
                    Expr::Some(Box::new(inner)),
                    start..end,
                ))
            }
            TokenKind::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                let end = self.expect(TokenKind::RParen, ")")?.span.end;
                Ok(Spanned::new(expr.node, start..end))
            }
            TokenKind::LBracket => {
                self.advance();
                let mut items = Vec::new();
                if self.peek().map(|t| &t.kind) != Some(&TokenKind::RBracket) {
                    loop {
                        items.push(self.parse_expr()?);
                        if self.match_kind(TokenKind::Comma).is_none() {
                            break;
                        }
                    }
                }
                let end = self.expect(TokenKind::RBracket, "]")?.span.end;
                Ok(Spanned::new(Expr::List(items), start..end))
            }
            TokenKind::LBrace => {
                self.advance();
                let mut entries = Vec::new();
                if self.peek().map(|t| &t.kind) != Some(&TokenKind::RBrace) {
                    loop {
                        let key = self.parse_expr()?;
                        self.expect(TokenKind::Colon, ":")?;
                        let value = self.parse_expr()?;
                        let span = key.span.start..value.span.end;
                        entries.push(Spanned::new(MapEntry { key, value }, span));
                        if self.match_kind(TokenKind::Comma).is_none() {
                            break;
                        }
                    }
                }
                let end = self.expect(TokenKind::RBrace, "}")?.span.end;
                Ok(Spanned::new(Expr::Map(entries), start..end))
            }
            TokenKind::Ident(name) => {
                self.advance();
                if self.peek().map(|t| &t.kind) == Some(&TokenKind::LParen) {
                    self.advance();
                    let mut args = Vec::new();
                    if self.peek().map(|t| &t.kind) != Some(&TokenKind::RParen) {
                        loop {
                            args.push(self.parse_expr()?);
                            if self.match_kind(TokenKind::Comma).is_none() {
                                break;
                            }
                        }
                    }
                    let end = self.expect(TokenKind::RParen, ")")?.span.end;
                    Ok(Spanned::new(
                        Expr::Call {
                            callee: Spanned::new(name.clone(), tok.span.clone()),
                            args,
                        },
                        start..end,
                    ))
                } else if self.peek().map(|t| &t.kind) == Some(&TokenKind::LBrace) {
                    self.advance();
                    let mut fields = Vec::new();
                    if self.peek().map(|t| &t.kind) != Some(&TokenKind::RBrace) {
                        loop {
                            let fname = self.parse_ident()?;
                            self.expect(TokenKind::Colon, ":")?;
                            let value = self.parse_expr()?;
                            let span = fname.span.start..value.span.end;
                            fields.push(Spanned::new(FieldInit { name: fname, value }, span));
                            if self.match_kind(TokenKind::Comma).is_none() {
                                break;
                            }
                        }
                    }
                    let end = self.expect(TokenKind::RBrace, "}")?.span.end;
                    Ok(Spanned::new(
                        Expr::StructLit {
                            name: Spanned::new(name, tok.span),
                            fields,
                        },
                        start..end,
                    ))
                } else {
                    let span = tok.span.clone();
                    Ok(Spanned::new(Expr::Ident(Spanned::new(name, span.clone())), span))
                }
            }
            kind => Err(ParseError::Unexpected {
                expected: "expression".into(),
                found: format!("{:?}", kind),
                span: tok.span,
            }),
        }
    }

    fn parse_qualifiable_name(&mut self) -> Result<Spanned<String>, ParseError> {
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

    fn parse_ident(&mut self) -> Result<Spanned<String>, ParseError> {
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

fn qual_name_from_expr(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident(name) => Some(name.node.clone()),
        Expr::FieldAccess { object, field } => qual_name_from_expr(&object.node)
            .map(|base| format!("{base}.{}", field.node)),
        _ => None,
    }
}