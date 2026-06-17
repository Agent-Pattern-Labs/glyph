# Etymonoetic Interlingua

> **Moved:** Etymonoetic Interlingua now lives in the Glyph monorepo at <https://github.com/agent-pattern-labs/glyph>. This repository is kept only as an archive of the original standalone project.

An etymonoetic interlingua is a machine-facing semantic layer for representing words as compressed, explainable meaning artifacts.

This project explores how AI systems can model lexical meaning through distinct layers:

- surface form
- morphology
- etymological lineage
- historical semantic drift
- present usage
- cultural frame
- pragmatic intent
- certainty and provenance

The goal is not to create a replacement human language. The goal is to create a semantic representation system that helps models expand, compare, audit, and reason about meaning.

## Core Principle

Etymology is not the true meaning of a word.

This project explicitly avoids the etymological fallacy. A word's origin can explain part of its historical path, but current meaning also depends on usage, discourse, social context, pragmatic stance, and cultural framing.

The preferred interpretive order is:

```text
origin -> morphology -> historical development -> current usage -> pragmatic intent
```

## Semantic Capsule Model

A semantic capsule should preserve separable layers of meaning rather than flattening a word into a synonym.

For example, `iconoclast` should not be represented only as `rebel`.

A richer capsule might include:

- source lineage: Greek `eikon`, image or likeness, and `klastes`, breaker
- morphology: image + breaker
- historical frame: religious image-breaking and opposition to icons
- semantic drift: extension from literal destruction of icons to attacking revered institutions or beliefs
- present usage: reformer, vandal, contrarian, critic, challenger of orthodoxy
- pragmatic stance: praise, criticism, irony, branding, or warning depending on context
- certainty: which claims are attested, inferred, disputed, speculative, or unknown

## Design Goals

- Keep etymology, morphology, semantic drift, and pragmatics separate.
- Track provenance and uncertainty explicitly.
- Support multilingual and cross-cultural semantic comparison.
- Make word-to-paragraph expansion explainable and traceable.
- Represent pragmatic stance instead of assuming a single stable connotation.
- Preserve modern usage even when it diverges from historical origin.

## Intended Uses

This representation layer may support:

- semantic search and retrieval
- AI interpretability
- lexical reasoning
- translation and interlingual transfer
- ontology construction
- educational word expansion
- cultural and pragmatic nuance modeling

## Status

This repository now contains an MVP: a JSON Schema, Rust validator, CLI, seed examples, cited capsules, and tests for semantic capsules.

## MVP Quick Start

During development, run commands through Cargo. After installing with `cargo install --path .`, replace `cargo run --` with `ei`.

Validate the bundled seed capsules:

```bash
cargo run -- validate examples/iconoclast.json examples/radical.json
```

Inspect a compact summary:

```bash
cargo run -- show examples/iconoclast.json
```

Create a valid starter capsule:

```bash
cargo run -- new sincere --part-of-speech adjective --output examples/sincere.json
```

Create a starter with Wiktionary provenance:

```bash
cargo run -- new sincere --part-of-speech adjective --wiktionary-source --output examples/sincere.json
```

Print an explainable expansion:

```bash
cargo run -- expand examples/iconoclast.json --trace
```

Export JSONL training records:

```bash
cargo run -- export-training examples/iconoclast.json examples/radical.json --output training.seed.jsonl
```

Run the test suite:

```bash
cargo test
```

The core schema lives at:

```text
schemas/semantic-capsule.schema.json
```

See [docs/usage.md](docs/usage.md), [docs/mvp.md](docs/mvp.md), [docs/schema.md](docs/schema.md), and [docs/source-policy.md](docs/source-policy.md) for the current implementation boundary.

## Repository Layout

```text
src/                          Rust validator, library, and CLI
schemas/                      JSON Schema contract
examples/                     Seed semantic capsules
capsules/                     Production-candidate cited capsules
docs/                         MVP and schema notes
tests/                        Rust integration tests
```

## How This Can Be Used Today

- Build a curated lexical dataset where every word has separate morphology, etymology, drift, current usage, pragmatics, provenance, and uncertainty layers.
- Create training pairs for `text -> capsule` and `capsule -> explanation`.
- Use the schema in annotation tools or eval harnesses.
- Store nuanced word meanings in agent memory or RAG systems as structured, inspectable objects.
- Test whether an AI explanation avoids treating etymology as the true meaning.
- Prototype with the 10-word cited capsule set in `capsules/en/`.

## Current Boundary

The MVP is schema-first. It now includes a small production-candidate cited capsule set, but it does not yet include automatic resource adapters, compact notation, or production model-training datasets.

The next useful step is to expand the cited set to 25 words and then build adapters for OntoLex, lemonEty, Wiktionary, WordNet, ConceptNet, and corpora.
