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

This repository is an early-stage research and implementation space. Initial work should prioritize schemas, examples, provenance conventions, and small testable representations before larger systems are built around them.

