# Using EI Today

Etymonoetic Interlingua can be used today as a structured lexical meaning layer. The current implementation is not an automatic etymology engine; it is a schema and toolchain for creating, validating, inspecting, and exporting semantic capsules.

## 1. Create a Starter Capsule

During development, run commands through Cargo. After installing with `cargo install --path .`, replace `cargo run --` with `ei`.

```bash
cargo run -- new sincere --part-of-speech adjective --output examples/sincere.json
```

To start with a Wiktionary source citation:

```bash
cargo run -- new sincere --part-of-speech adjective --wiktionary-source --output examples/sincere.json
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
cargo run -- validate examples/sincere.json
```

Validation checks the JSON Schema and verifies that every `provenance_refs` value points to a declared source.

Validate the curated capsule set:

```bash
cargo run -- validate capsules/en/*.json
```

## 4. Inspect and Expand

```bash
cargo run -- show examples/iconoclast.json
cargo run -- expand examples/iconoclast.json --trace
```

`show` gives a compact capsule summary. `expand` prints the explainable paragraph and, with `--trace`, shows which layers contributed to it.

## 5. Export Training Records

```bash
cargo run -- export-training examples/iconoclast.json examples/radical.json --output training.seed.jsonl
```

The export creates two JSONL records per capsule:

- `text_to_capsule`: lexical item to full semantic capsule
- `capsule_to_expansion`: full capsule to explainable paragraph and trace

This is the first bridge toward model training and evals. More task types can be added later, such as contextual sense selection, pragmatic stance detection, and cross-lingual transfer.

## 6. Use the Schema in Other Systems

The schema can be used directly in apps, annotation tools, data pipelines, or eval harnesses:

```text
schemas/semantic-capsule.schema.json
```

Useful immediate integrations:

- lexical annotation projects
- AI interpretability demos
- word-to-paragraph expansion datasets
- RAG memory records for nuanced word meanings
- evals for etymological fallacy avoidance
- adapters from Wiktionary, WordNet, ConceptNet, OntoLex, or corpora

## 7. Use the Curated Capsule Set

The repository includes a small production-candidate set in:

```text
capsules/en/
```

The set is indexed by:

```text
capsules/manifest.json
```

These capsules are suitable for demos, schema iteration, training-record export, and eval prototyping. They are not yet a comprehensive lexical database.

## Current Limits

The toolchain does not yet fetch external sources or generate authoritative capsules automatically. Any production capsule should include cited lexicographic, corpus, or scholarly provenance.

See [source-policy.md](source-policy.md) before adding source-derived data.
