# Golden Fixtures

Each fixture group uses the same base name:

- `<name>.glyph`: source program.
- `<name>.ir.json`: exact GlyphIR expected from the parser/compiler.
- `<name>.trace.json`: normalized mock-runtime trace.

Normalized traces include stable semantic fields only:

- `stepId`
- `operation`
- `status`
- `outputSummary`
- `iteration`, when present

They intentionally omit `durationMs`, resolved arguments, and full output values because those are runtime-dependent or verbose.
