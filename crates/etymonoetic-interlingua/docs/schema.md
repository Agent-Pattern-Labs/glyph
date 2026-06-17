# Semantic Capsule Schema

The semantic capsule schema defines the first public contract for Etymonoetic Interlingua.

Current version: `0.1.0`

Bundled schema path:

```text
schemas/semantic-capsule.schema.json
```

## Required Top-Level Fields

- `schema_version`: currently `0.1.0`
- `id`: stable capsule identifier
- `capsule_type`: currently `lexeme`
- `surface`: form, normalized form, language, script, and part of speech
- `morphology`: segments and their role/glosses
- `etymology`: lineage and etymons
- `semantic_drift`: historical developments between senses
- `present_usage`: current senses and usage examples
- `cultural_frame`: cultural or discourse frames
- `pragmatics`: stance-sensitive interpretations
- `expansion`: paragraph expansion plus trace
- `capsule_summary`: compact summary
- `provenance`: declared sources
- `uncertainty`: overall uncertainty and unresolved questions

## Certainty Values

The schema currently allows:

- `attested`
- `reconstructed`
- `inferred`
- `speculative`
- `disputed`
- `unknown`

Use `attested` for claims with a supporting source, `inferred` for reasoned interpretation, `reconstructed` for historical linguistic reconstruction, and `speculative` only when a claim is intentionally weak.

## Provenance

Any object with `provenance_refs` must refer to an `id` in the top-level `provenance` array.

The validator enforces this reference integrity after JSON Schema validation.

## Design Constraint

Do not encode etymology as current meaning. Etymology, morphology, semantic drift, usage, and pragmatics are separate fields because they answer different questions.
