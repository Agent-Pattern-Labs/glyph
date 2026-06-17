# Using EI Today

Etymonoetic Interlingua can be used today as a structured lexical meaning layer. The current implementation is not an automatic etymology engine; it is a schema and toolchain for creating, validating, inspecting, and exporting semantic capsules.

## 1. Create a Starter Capsule

```bash
PYTHONPATH=src python3 -m etymonoetic_interlingua new sincere --part-of-speech adjective --output examples/sincere.json
```

This creates a valid capsule with explicit `unknown` placeholders. A starter capsule is not final semantic data. It is a safe scaffold that forces every layer to be considered.

## 2. Fill the Layers

Edit the generated JSON and replace placeholders in:

- `morphology`
- `etymology`
- `semantic_drift`
- `present_usage`
- `cultural_frame`
- `pragmatics`
- `expansion`
- `provenance`
- `uncertainty`

Keep the layers separate. Do not turn an etymological origin into the current meaning.

## 3. Validate the Capsule

```bash
PYTHONPATH=src python3 -m etymonoetic_interlingua validate examples/sincere.json
```

Validation checks the JSON Schema and verifies that every `provenance_refs` value points to a declared source.

## 4. Inspect and Expand

```bash
PYTHONPATH=src python3 -m etymonoetic_interlingua show examples/iconoclast.json
PYTHONPATH=src python3 -m etymonoetic_interlingua expand examples/iconoclast.json --trace
```

`show` gives a compact capsule summary. `expand` prints the explainable paragraph and, with `--trace`, shows which layers contributed to it.

## 5. Export Training Records

```bash
PYTHONPATH=src python3 -m etymonoetic_interlingua export-training examples/iconoclast.json examples/radical.json --output training.seed.jsonl
```

The export creates two JSONL records per capsule:

- `text_to_capsule`: lexical item to full semantic capsule
- `capsule_to_expansion`: full capsule to explainable paragraph and trace

This is the first bridge toward model training and evals. More task types can be added later, such as contextual sense selection, pragmatic stance detection, and cross-lingual transfer.

## 6. Use the Schema in Other Systems

The schema can be used directly in apps, annotation tools, data pipelines, or eval harnesses:

```text
src/etymonoetic_interlingua/schemas/semantic-capsule.schema.json
```

Useful immediate integrations:

- lexical annotation projects
- AI interpretability demos
- word-to-paragraph expansion datasets
- RAG memory records for nuanced word meanings
- evals for etymological fallacy avoidance
- adapters from Wiktionary, WordNet, ConceptNet, OntoLex, or corpora

## Current Limits

The toolchain does not yet fetch external sources or generate authoritative capsules automatically. Any production capsule should include cited lexicographic, corpus, or scholarly provenance.
