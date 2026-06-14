use nebula_ast::*;

use crate::lexer::{Token, TokenKind};

use super::{ParseError, Parser};

impl Parser {
    pub(super) fn parse_stmt(&mut self) -> Result<Spanned<Stmt>, ParseError> {
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
        let then_block = self.parse_end_control_block(&[TokenKind::Else, TokenKind::End])?;
        let else_block = if self.match_kind(TokenKind::Else).is_some() {
            Some(self.parse_end_control_block(&[TokenKind::End])?)
        } else {
            None
        };
        self.expect(TokenKind::End, "end")?;

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
        let body = self.parse_end_control_block(&[TokenKind::End])?;
        self.expect(TokenKind::End, "end")?;
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
        let body = self.parse_end_control_block(&[TokenKind::End])?;
        self.expect(TokenKind::End, "end")?;
        Ok(Stmt::Telemetry { body })
    }

    fn parse_end_control_block(
        &mut self,
        terminators: &[TokenKind],
    ) -> Result<Vec<Spanned<Stmt>>, ParseError> {
        if let Some(Token { span, .. }) = self.peek() {
            if self.peek().map(|t| &t.kind) == Some(&TokenKind::LBrace) {
                return Err(ParseError::DeprecatedBraceBlock {
                    canonical: "end".into(),
                    found: "brace block".into(),
                    span: span.clone(),
                });
            }
        }
        self.parse_end_block(terminators)
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

}