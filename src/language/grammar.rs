pub const GLYPH_PRIMITIVES: &[&str] = &[
    "SPEC",
    "PLAN",
    "GEN",
    "CHECK",
    "FIX",
    "PATCH",
    "SUM",
    "SUMMARIZE",
    "ASK",
    "EXPORT",
    "RUN",
    "READ",
    "WRITE",
];

pub const GLYPH_EBNF: &str = include_str!("../../spec/glyph.ebnf");
pub const GLYPH_GBNF: &str = include_str!("../../spec/glyph.gbnf");
pub const GLYPH_CONTROLLER_OUTPUT_JSON_SCHEMA: &str =
    include_str!("../../spec/controller-output.schema.json");

pub fn get_grammar_artifact(format: &str) -> Option<&'static str> {
    match format {
        "ebnf" => Some(GLYPH_EBNF),
        "gbnf" => Some(GLYPH_GBNF),
        "json-schema" => Some(GLYPH_CONTROLLER_OUTPUT_JSON_SCHEMA),
        _ => None,
    }
}
