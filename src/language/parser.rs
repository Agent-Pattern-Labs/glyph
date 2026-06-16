use serde_json::Number;

use super::ast::{
    CallArgAst, ContextAst, ExpressionAst, FlowAst, ObjectEntryAst, ProgramAst, RepairBlockAst,
    StatementAst, ToolCallAst,
};
use super::errors::GlyphSyntaxError;
use super::tokenizer::{Token, TokenKind, tokenize};

pub fn parse_glyph(source: &str) -> Result<ProgramAst, GlyphSyntaxError> {
    Parser::new(tokenize(source)?).parse_program()
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, index: 0 }
    }

    fn parse_program(&mut self) -> Result<ProgramAst, GlyphSyntaxError> {
        let mut goal = None;
        let mut context = None;
        let mut flows = Vec::new();

        while !self.is_eof() {
            if self.match_identifier("goal") {
                if goal.is_some() {
                    return self.fail_here("Duplicate goal declaration");
                }
                goal = Some(self.parse_goal()?);
                continue;
            }

            if self.match_identifier("ctx") {
                if context.is_some() {
                    return self.fail_here("Duplicate ctx declaration");
                }
                context = Some(self.parse_context()?);
                continue;
            }

            if self.match_identifier("flow") {
                flows.push(self.parse_flow()?);
                continue;
            }

            if let TokenKind::Identifier(value) = &self.peek().kind {
                return self.fail_here(&format!("Unknown block type \"{value}\""));
            }

            return self.fail_here(&format!("Unexpected token {}", self.describe(self.peek())));
        }

        if flows.is_empty() {
            return self.fail_at_last("Program must declare at least one flow");
        }

        Ok(ProgramAst {
            goal,
            context,
            flows,
        })
    }

    fn parse_goal(&mut self) -> Result<String, GlyphSyntaxError> {
        self.consume_identifier("goal", "Expected goal")?;
        self.consume_string("Expected goal string")
    }

    fn parse_context(&mut self) -> Result<ContextAst, GlyphSyntaxError> {
        self.consume_identifier("ctx", "Expected ctx")?;
        self.consume_symbol('{', "Expected opening brace after ctx")?;
        let entries = self.parse_object_entries('}')?;
        self.consume_symbol('}', "Missing closing brace for ctx block")?;
        Ok(ContextAst { entries })
    }

    fn parse_flow(&mut self) -> Result<FlowAst, GlyphSyntaxError> {
        self.consume_identifier("flow", "Expected flow")?;
        let name = self.consume_identifier_any("Expected flow name")?;
        self.consume_symbol('{', "Expected opening brace after flow name")?;

        let mut steps = Vec::new();
        while !self.is_eof() && !self.match_symbol('}') {
            steps.push(self.parse_statement()?);
        }

        self.consume_symbol('}', &format!("Missing closing brace for flow \"{name}\""))?;
        Ok(FlowAst { name, steps })
    }

    fn parse_statement(&mut self) -> Result<StatementAst, GlyphSyntaxError> {
        if self.match_identifier("repair") {
            return self.parse_repair_block().map(StatementAst::RepairBlock);
        }

        if matches!(self.peek().kind, TokenKind::Identifier(_)) {
            return self.parse_tool_call().map(StatementAst::ToolCall);
        }

        self.fail_here(&format!(
            "Unexpected token in flow: {}",
            self.describe(self.peek())
        ))
    }

    fn parse_tool_call(&mut self) -> Result<ToolCallAst, GlyphSyntaxError> {
        let op = self.consume_identifier_any("Expected tool operation")?;
        self.consume_symbol('(', &format!("Expected opening parenthesis after {op}"))?;
        let args = self.parse_call_args()?;
        self.consume_symbol(')', &format!("Missing closing parenthesis for {op}"))?;

        let assign_to = if self.match_arrow() {
            self.advance();
            Some(
                self.consume_identifier_any("Invalid assignment: expected variable name after ->")?,
            )
        } else {
            None
        };

        Ok(ToolCallAst {
            op,
            args,
            assign_to,
        })
    }

    fn parse_call_args(&mut self) -> Result<Vec<CallArgAst>, GlyphSyntaxError> {
        let mut args = Vec::new();
        if self.match_symbol(')') {
            return Ok(args);
        }

        loop {
            let name = if matches!(self.peek().kind, TokenKind::Identifier(_))
                && matches!(self.peek_offset(1).kind, TokenKind::Symbol('='))
            {
                let name = self.consume_identifier_any("Expected argument name")?;
                self.consume_symbol('=', &format!("Expected = after argument name \"{name}\""))?;
                Some(name)
            } else {
                None
            };

            args.push(CallArgAst {
                name,
                value: self.parse_expression()?,
            });

            if self.match_symbol(',') {
                self.advance();
                if self.match_symbol(')') {
                    break;
                }
                continue;
            }

            break;
        }

        Ok(args)
    }

    fn parse_repair_block(&mut self) -> Result<RepairBlockAst, GlyphSyntaxError> {
        self.consume_identifier("repair", "Invalid repair block: expected repair")?;
        let target =
            self.consume_identifier_any("Invalid repair block: expected target variable")?;
        self.consume_identifier("with", "Invalid repair block: expected with")?;
        let report =
            self.consume_identifier_any("Invalid repair block: expected report variable")?;
        self.consume_identifier("max", "Invalid repair block: expected max")?;
        let max_token = self.peek().clone();
        let max_number = self.consume_number("Invalid repair block: max must be a number")?;
        let max = number_to_usize(&max_number).ok_or_else(|| {
            GlyphSyntaxError::new(
                "Invalid repair block: max must be a non-negative integer",
                max_token.line,
                max_token.column,
            )
        })?;

        self.consume_symbol('{', "Invalid repair block: expected opening brace")?;
        let mut steps = Vec::new();
        while !self.is_eof() && !self.match_symbol('}') {
            steps.push(self.parse_statement()?);
        }
        self.consume_symbol('}', "Invalid repair block: missing closing brace")?;

        Ok(RepairBlockAst {
            target,
            report,
            max,
            steps,
        })
    }

    fn parse_expression(&mut self) -> Result<ExpressionAst, GlyphSyntaxError> {
        match self.peek().kind.clone() {
            TokenKind::String(value) => {
                self.advance();
                Ok(ExpressionAst::String(value))
            }
            TokenKind::Number(value) => {
                self.advance();
                Ok(ExpressionAst::Number(value))
            }
            TokenKind::Boolean(value) => {
                self.advance();
                Ok(ExpressionAst::Boolean(value))
            }
            TokenKind::Identifier(name) => {
                self.advance();
                if name == "ctx" && self.match_symbol('.') {
                    let mut path = Vec::new();
                    while self.match_symbol('.') {
                        self.advance();
                        path.push(self.consume_identifier_any("Expected ctx property after .")?);
                    }
                    if path.is_empty() {
                        return self.fail_here("Expected ctx property reference");
                    }
                    return Ok(ExpressionAst::CtxRef(path));
                }

                Ok(ExpressionAst::VarRef(name))
            }
            TokenKind::Symbol('[') => {
                self.advance();
                let mut items = Vec::new();
                while !self.is_eof() && !self.match_symbol(']') {
                    items.push(self.parse_expression()?);
                    if self.match_symbol(',') {
                        self.advance();
                        continue;
                    }
                    if !self.match_symbol(']') {
                        return self
                            .fail_here("Invalid array literal: expected comma or closing bracket");
                    }
                }
                self.consume_symbol(']', "Invalid array literal: missing closing bracket")?;
                Ok(ExpressionAst::Array(items))
            }
            TokenKind::Symbol('{') => {
                self.advance();
                let entries = self.parse_object_entries('}')?;
                self.consume_symbol('}', "Invalid object literal: missing closing brace")?;
                Ok(ExpressionAst::Object(entries))
            }
            other => self.fail_here(&format!(
                "Invalid argument: expected expression, got {}",
                self.describe_kind(&other)
            )),
        }
    }

    fn parse_object_entries(
        &mut self,
        end_symbol: char,
    ) -> Result<Vec<ObjectEntryAst>, GlyphSyntaxError> {
        let mut entries = Vec::new();

        while !self.is_eof() && !self.match_symbol(end_symbol) {
            let key = match self.peek().kind.clone() {
                TokenKind::Identifier(value) => {
                    self.advance();
                    value
                }
                TokenKind::String(value) => {
                    self.advance();
                    value
                }
                _ => {
                    return self.fail_here(&format!(
                        "Invalid object key: expected identifier or string, got {}",
                        self.describe(self.peek())
                    ));
                }
            };

            self.consume_symbol(':', &format!("Expected : after object key \"{key}\""))?;
            entries.push(ObjectEntryAst {
                key,
                value: self.parse_expression()?,
            });

            if self.match_symbol(',') {
                self.advance();
            }
        }

        Ok(entries)
    }

    fn consume_identifier(&mut self, value: &str, message: &str) -> Result<(), GlyphSyntaxError> {
        if !self.match_identifier(value) {
            return self.fail_here(message);
        }
        self.advance();
        Ok(())
    }

    fn consume_identifier_any(&mut self, message: &str) -> Result<String, GlyphSyntaxError> {
        match self.peek().kind.clone() {
            TokenKind::Identifier(value) => {
                self.advance();
                Ok(value)
            }
            _ => self.fail_here(message),
        }
    }

    fn consume_string(&mut self, message: &str) -> Result<String, GlyphSyntaxError> {
        match self.peek().kind.clone() {
            TokenKind::String(value) => {
                self.advance();
                Ok(value)
            }
            _ => self.fail_here(message),
        }
    }

    fn consume_number(&mut self, message: &str) -> Result<Number, GlyphSyntaxError> {
        match self.peek().kind.clone() {
            TokenKind::Number(value) => {
                self.advance();
                Ok(value)
            }
            _ => self.fail_here(message),
        }
    }

    fn consume_symbol(&mut self, value: char, message: &str) -> Result<(), GlyphSyntaxError> {
        if !self.match_symbol(value) {
            return self.fail_here(message);
        }
        self.advance();
        Ok(())
    }

    fn match_identifier(&self, value: &str) -> bool {
        matches!(&self.peek().kind, TokenKind::Identifier(current) if current == value)
    }

    fn match_symbol(&self, value: char) -> bool {
        matches!(self.peek().kind, TokenKind::Symbol(current) if current == value)
    }

    fn match_arrow(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Arrow)
    }

    fn advance(&mut self) -> Token {
        let token = self.peek().clone();
        self.index += 1;
        token
    }

    fn peek(&self) -> &Token {
        self.tokens
            .get(self.index)
            .unwrap_or_else(|| self.tokens.last().expect("parser always has eof token"))
    }

    fn peek_offset(&self, offset: usize) -> &Token {
        self.tokens
            .get(self.index + offset)
            .unwrap_or_else(|| self.tokens.last().expect("parser always has eof token"))
    }

    fn is_eof(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Eof)
    }

    fn fail_here<T>(&self, message: &str) -> Result<T, GlyphSyntaxError> {
        let token = self.peek();
        Err(GlyphSyntaxError::new(message, token.line, token.column))
    }

    fn fail_at_last<T>(&self, message: &str) -> Result<T, GlyphSyntaxError> {
        let token = self
            .tokens
            .get(self.tokens.len().saturating_sub(2))
            .unwrap_or_else(|| self.peek());
        Err(GlyphSyntaxError::new(message, token.line, token.column))
    }

    fn describe(&self, token: &Token) -> String {
        self.describe_kind(&token.kind)
    }

    fn describe_kind(&self, kind: &TokenKind) -> String {
        match kind {
            TokenKind::Identifier(value) => format!("identifier \"{value}\""),
            TokenKind::String(value) => format!("string \"{value}\""),
            TokenKind::Number(value) => format!("number \"{value}\""),
            TokenKind::Boolean(value) => format!("boolean \"{value}\""),
            TokenKind::Symbol(value) => format!("symbol \"{value}\""),
            TokenKind::Arrow => "arrow \"->\"".to_string(),
            TokenKind::Eof => "end of file".to_string(),
        }
    }
}

fn number_to_usize(number: &Number) -> Option<usize> {
    number
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
}
