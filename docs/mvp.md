# MVP

The MVP is a working semantic capsule contract plus validation tooling. It is deliberately small: the first target is to make layered lexical meaning inspectable, testable, and extensible before inventing a compact notation.

## Included

- A bundled JSON Schema for etymonoetic lexical capsules.
- A Python validator that checks schema conformance.
- A provenance reference check so claims can point to declared sources.
- A CLI for validating and inspecting capsules.
- A CLI starter generator for valid placeholder capsules.
- A JSONL export command for text-to-capsule and capsule-to-expansion training records.
- Two seed examples: `iconoclast` and `radical`.
- Tests that enforce required layers and provenance integrity.

## Not Included Yet

- Production-quality lexicographic citations.
- Adapters for OntoLex, lemonEty, Wiktionary, WordNet, ConceptNet, or corpora.
- Automated capsule generation from raw text.
- Compact EI notation.
- Production model training datasets.

## Why Schema First

The schema is the stable center of the project. It lets adapters, examples, generated data, evals, and future notation all target the same representation.

The MVP requires these layers to remain separate:

- surface
- morphology
- etymology
- semantic drift
- present usage
- cultural frame
- pragmatics
- expansion
- provenance
- uncertainty

This prevents a word from being collapsed into a synonym or treated as if etymology were its true meaning.

## Validation Workflow

```bash
PYTHONPATH=src python3 -m etymonoetic_interlingua validate examples/iconoclast.json examples/radical.json
```

Or, after installing the package:

```bash
ei validate examples/iconoclast.json examples/radical.json
ei show examples/iconoclast.json
ei expand examples/iconoclast.json --trace
ei new sincere --part-of-speech adjective --output examples/sincere.json
ei export-training examples/iconoclast.json examples/radical.json --output training.seed.jsonl
```

## Next Milestones

1. Add cited production capsules for 10 to 25 words.
2. Add an adapter interface for imported lexical resources.
3. Implement a Wiktionary-derived seed adapter with conservative provenance labels.
4. Add an OntoLex/lemonEty export path.
5. Create text-to-capsule and capsule-to-text training examples.
6. Define a compact EI notation only after the schema stabilizes.
