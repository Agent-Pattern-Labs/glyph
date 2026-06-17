# Source Policy

EI capsules must separate lexical evidence from EI interpretation.

## Source-Derived Facts

Facts such as etymological forms, borrowing paths, historical senses, and dictionary senses should cite a source in the top-level `provenance` array.

For the current curated set, the primary external source is English Wiktionary. Capsule wording is manually normalized into EI layers rather than copied wholesale.

## Manual EI Analysis

Fields such as cultural frame, pragmatic stance, expansion trace, and some semantic-drift descriptions are EI analysis unless explicitly tied to a cited source.

Use a separate manual provenance entry for these fields:

```json
{
  "id": "ei-manual-example",
  "source_type": "manual",
  "citation": "Etymonoetic Interlingua curated analysis for example."
}
```

## Certainty Labels

- `attested`: supported by cited lexical, corpus, or scholarly evidence.
- `reconstructed`: historical reconstruction, preferably with scholarly citation.
- `inferred`: reasoned EI interpretation from cited evidence or usage.
- `speculative`: intentionally weak claim that needs support.
- `disputed`: competing analyses exist or the source marks uncertainty.
- `unknown`: placeholder or unsupported field.

## Licensing

Do not copy long dictionary text into capsules.

When a capsule uses Wiktionary as a source, include the page URL, access date, and license note. Source-derived facts retain their source terms. Original EI layer analysis is project-authored.

## Production Criteria

A production-grade capsule should have:

- at least one dictionary or scholarly source
- separate provenance for manual EI analysis
- uncertainty notes for unresolved issues
- no uncited claims marked as `attested`
- no folk etymology presented as origin
- current usage separated from etymology
