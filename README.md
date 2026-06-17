# Etymonoetic Interlingua

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

This repository now contains an MVP: a JSON Schema, Python validator, CLI, seed examples, and tests for semantic capsules.

## MVP Quick Start

Validate the bundled seed capsules:

```bash
PYTHONPATH=src python3 -m etymonoetic_interlingua validate examples/iconoclast.json examples/radical.json
```

Inspect a compact summary:

```bash
PYTHONPATH=src python3 -m etymonoetic_interlingua show examples/iconoclast.json
```

Create a valid starter capsule:

```bash
PYTHONPATH=src python3 -m etymonoetic_interlingua new sincere --part-of-speech adjective --output examples/sincere.json
```

Print an explainable expansion:

```bash
PYTHONPATH=src python3 -m etymonoetic_interlingua expand examples/iconoclast.json --trace
```

Export JSONL training records:

```bash
PYTHONPATH=src python3 -m etymonoetic_interlingua export-training examples/iconoclast.json examples/radical.json --output training.seed.jsonl
```

Run the test suite:

```bash
PYTHONPATH=src python3 -m pytest
```

The core schema lives at:

```text
src/etymonoetic_interlingua/schemas/semantic-capsule.schema.json
```

See [docs/usage.md](docs/usage.md), [docs/mvp.md](docs/mvp.md), and [docs/schema.md](docs/schema.md) for the current implementation boundary.

## Repository Layout

```text
src/etymonoetic_interlingua/   Python validator and CLI
examples/                     Seed semantic capsules
docs/                         MVP and schema notes
tests/                        Validator and CLI tests
```

## How This Can Be Used Today

- Build a curated lexical dataset where every word has separate morphology, etymology, drift, current usage, pragmatics, provenance, and uncertainty layers.
- Create training pairs for `text -> capsule` and `capsule -> explanation`.
- Use the schema in annotation tools or eval harnesses.
- Store nuanced word meanings in agent memory or RAG systems as structured, inspectable objects.
- Test whether an AI explanation avoids treating etymology as the true meaning.

## Current Boundary

The MVP is schema-first. It does not yet include production-grade lexical citations, resource adapters, compact notation, or production model-training datasets.

The next useful step is to add a small set of cited production capsules and then build adapters for OntoLex, lemonEty, Wiktionary, WordNet, ConceptNet, and corpora.
