use serde::Serialize;

use super::examples::CompressionExample;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CompressionStats {
    #[serde(rename = "naturalLanguageChars")]
    pub natural_language_chars: usize,
    #[serde(rename = "glyphChars")]
    pub glyph_chars: usize,
    #[serde(rename = "naturalLanguageWords")]
    pub natural_language_words: usize,
    #[serde(rename = "glyphWords")]
    pub glyph_words: usize,
    #[serde(rename = "naturalLanguageApproxTokens")]
    pub natural_language_approx_tokens: usize,
    #[serde(rename = "glyphApproxTokens")]
    pub glyph_approx_tokens: usize,
    #[serde(rename = "compressionRatio")]
    pub compression_ratio: f64,
}

pub fn compare_compression(glyph_source: &str, example: &CompressionExample) -> CompressionStats {
    let natural_language = example.natural_language.trim();
    let glyph = glyph_source.trim();

    CompressionStats {
        natural_language_chars: natural_language.len(),
        glyph_chars: glyph.len(),
        natural_language_words: count_words(natural_language),
        glyph_words: count_words(glyph),
        natural_language_approx_tokens: approximate_tokens(natural_language),
        glyph_approx_tokens: approximate_tokens(glyph),
        compression_ratio: natural_language.len() as f64 / glyph.len().max(1) as f64,
    }
}

pub fn count_words(text: &str) -> usize {
    text.split_whitespace().count()
}

pub fn approximate_tokens(text: &str) -> usize {
    text.len().div_ceil(4)
}
