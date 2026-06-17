# Glyph Specification

This directory is the language-neutral contract for Glyph implementations.

The Rust runtime is the current reference implementation. Future implementations should target these files instead of copying Rust internals.

## Files

- `glyph.ebnf`: human-readable source grammar.
- `glyph.gbnf`: grammar-constrained decoding artifact for llama.cpp-style runtimes.
- `controller-output.schema.json`: JSON envelope for models that can constrain JSON but not arbitrary source text.
- `generic-tool-plan.schema.json`: executable JSON tool-plan baseline schema used for comparison against Glyph source.
- `glyph-ir.schema.json`: JSON Schema for GlyphIR `version: "0.1"`.
- `fixtures/`: golden source, IR, and normalized trace fixtures.

## Compatibility Rule

An implementation is compatible when it can:

1. Parse each `*.glyph` fixture.
2. Emit exactly the corresponding `*.ir.json` fixture.
3. Execute with the mock harness semantics and emit the corresponding normalized trace fixture.

Trace durations are intentionally excluded from golden fixtures because they are runtime-dependent.
