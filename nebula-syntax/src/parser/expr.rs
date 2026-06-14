use nebula_ast::*;

use crate::lexer::{Token, TokenKind};

use super::{ParseError, Parser};

impl Parser {
    pub(super) fn parse_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
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
            _ => None,
        }
    }

    fn reject_deprecated_cmp_synonym(&mut self) -> Result<(), ParseError> {
        let Some(Token { kind, span }) = self.peek().cloned() else {
            return Ok(());
        };
        let TokenKind::Ident(word) = kind else {
            return Ok(());
        };
        let synonym = match word.as_str() {
            "less" if matches!(
                self.tokens.get(self.pos + 1).map(|t| &t.kind),
                Some(TokenKind::Ident(than)) if than == "than"
            ) => Some(("lt", "less than")),
            "greater" if matches!(
                self.tokens.get(self.pos + 1).map(|t| &t.kind),
                Some(TokenKind::Ident(than)) if than == "than"
            ) => Some(("gt", "greater than")),
            _ => None,
        };
        if let Some((canonical, found)) = synonym {
            let end = self
                .tokens
                .get(self.pos + 1)
                .map(|t| t.span.end)
                .unwrap_or(span.end);
            return Err(ParseError::DeprecatedComparison {
                canonical: canonical.into(),
                found: found.into(),
                span: span.start..end,
            });
        }
        Ok(())
    }

    fn parse_cmp_expr(&mut self) -> Result<Spanned<Expr>, ParseError> {
        let mut left = self.parse_add_expr()?;
        loop {
            self.reject_deprecated_cmp_synonym()?;
            let Some(op) = self.try_consume_cmp_op() else {
                break;
            };
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
            TokenKind::Call => {
                self.advance();
                let name = self.parse_qualifiable_name()?;
                self.expect(TokenKind::LParen, "(")?;
                let args = self.parse_named_arg_list()?;
                let end = self.expect(TokenKind::RParen, ")")?.span.end;
                Ok(Spanned::new(
                    Expr::ProbeCall { name, args },
                    start..end,
                ))
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
}

fn qual_name_from_expr(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident(name) => Some(name.node.clone()),
        Expr::FieldAccess { object, field } => qual_name_from_expr(&object.node)
            .map(|base| format!("{base}.{}", field.node)),
        _ => None,
    }
}