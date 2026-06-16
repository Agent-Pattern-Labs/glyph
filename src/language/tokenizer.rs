use serde_json::Number;

use super::errors::GlyphSyntaxError;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Identifier(String),
    String(String),
    Number(Number),
    Boolean(bool),
    Symbol(char),
    Arrow,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
    pub column: usize,
}

pub fn tokenize(source: &str) -> Result<Vec<Token>, GlyphSyntaxError> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;
    let mut line = 1;
    let mut column = 1;

    while index < chars.len() {
        let ch = chars[index];

        if matches!(ch, ' ' | '\t' | '\r' | '\n') {
            advance(&chars, &mut index, &mut line, &mut column);
            continue;
        }

        if ch == '#' {
            while index < chars.len() && chars[index] != '\n' {
                advance(&chars, &mut index, &mut line, &mut column);
            }
            continue;
        }

        let start_line = line;
        let start_column = column;

        if ch == '-' && chars.get(index + 1) == Some(&'>') {
            advance(&chars, &mut index, &mut line, &mut column);
            advance(&chars, &mut index, &mut line, &mut column);
            tokens.push(Token {
                kind: TokenKind::Arrow,
                line: start_line,
                column: start_column,
            });
            continue;
        }

        if ch == '"' {
            advance(&chars, &mut index, &mut line, &mut column);
            let mut value = String::new();
            let mut closed = false;

            while index < chars.len() {
                let part = advance(&chars, &mut index, &mut line, &mut column);
                if part == '"' {
                    closed = true;
                    break;
                }

                if part == '\\' {
                    if index >= chars.len() {
                        return Err(GlyphSyntaxError::new(
                            "Unterminated string literal",
                            start_line,
                            start_column,
                        ));
                    }

                    let escaped = advance(&chars, &mut index, &mut line, &mut column);
                    value.push(match escaped {
                        '"' => '"',
                        '\\' => '\\',
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        other => other,
                    });
                    continue;
                }

                value.push(part);
            }

            if !closed {
                return Err(GlyphSyntaxError::new(
                    "Unterminated string literal",
                    start_line,
                    start_column,
                ));
            }

            tokens.push(Token {
                kind: TokenKind::String(value),
                line: start_line,
                column: start_column,
            });
            continue;
        }

        if ch.is_ascii_digit()
            || (ch == '-'
                && chars
                    .get(index + 1)
                    .is_some_and(|next| next.is_ascii_digit()))
        {
            let mut raw = String::new();
            if ch == '-' {
                raw.push(advance(&chars, &mut index, &mut line, &mut column));
            }

            while index < chars.len() && chars[index].is_ascii_digit() {
                raw.push(advance(&chars, &mut index, &mut line, &mut column));
            }

            if chars.get(index) == Some(&'.') {
                raw.push(advance(&chars, &mut index, &mut line, &mut column));
                if !chars.get(index).is_some_and(|digit| digit.is_ascii_digit()) {
                    return Err(GlyphSyntaxError::new(
                        "Invalid number literal",
                        start_line,
                        start_column,
                    ));
                }

                while index < chars.len() && chars[index].is_ascii_digit() {
                    raw.push(advance(&chars, &mut index, &mut line, &mut column));
                }
            }

            let number = parse_number(&raw).ok_or_else(|| {
                GlyphSyntaxError::new("Invalid number literal", start_line, start_column)
            })?;
            tokens.push(Token {
                kind: TokenKind::Number(number),
                line: start_line,
                column: start_column,
            });
            continue;
        }

        if ch.is_ascii_alphabetic() || ch == '_' {
            let mut value = String::new();
            while index < chars.len()
                && (chars[index].is_ascii_alphanumeric() || chars[index] == '_')
            {
                value.push(advance(&chars, &mut index, &mut line, &mut column));
            }

            let kind = match value.as_str() {
                "true" => TokenKind::Boolean(true),
                "false" => TokenKind::Boolean(false),
                _ => TokenKind::Identifier(value),
            };
            tokens.push(Token {
                kind,
                line: start_line,
                column: start_column,
            });
            continue;
        }

        if matches!(
            ch,
            '{' | '}' | '(' | ')' | '[' | ']' | ':' | ',' | '=' | '.'
        ) {
            tokens.push(Token {
                kind: TokenKind::Symbol(ch),
                line: start_line,
                column: start_column,
            });
            advance(&chars, &mut index, &mut line, &mut column);
            continue;
        }

        return Err(GlyphSyntaxError::new(
            format!("Unknown token \"{ch}\""),
            start_line,
            start_column,
        ));
    }

    tokens.push(Token {
        kind: TokenKind::Eof,
        line,
        column,
    });
    Ok(tokens)
}

fn advance(chars: &[char], index: &mut usize, line: &mut usize, column: &mut usize) -> char {
    let ch = chars[*index];
    *index += 1;
    if ch == '\n' {
        *line += 1;
        *column = 1;
    } else {
        *column += 1;
    }
    ch
}

fn parse_number(raw: &str) -> Option<Number> {
    if raw.contains('.') {
        raw.parse::<f64>().ok().and_then(Number::from_f64)
    } else {
        raw.parse::<i64>().ok().map(Number::from)
    }
}
