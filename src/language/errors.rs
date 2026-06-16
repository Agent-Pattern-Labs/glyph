use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("{message} at {line}:{column}")]
pub struct GlyphSyntaxError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

impl GlyphSyntaxError {
    pub fn new(message: impl Into<String>, line: usize, column: usize) -> Self {
        Self {
            message: message.into(),
            line,
            column,
        }
    }
}
