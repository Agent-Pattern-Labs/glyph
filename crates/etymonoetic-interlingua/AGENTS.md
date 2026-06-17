# AGENTS.md

## Project purpose

This repository is building an etymonoetic interlingua: a machine-facing semantic layer that helps AI represent words as compressed semantic artifacts.

The goal is to model language through layered meaning:

- surface form
- morphology
- etymological lineage
- historical semantic drift
- present usage
- cultural frame
- pragmatic intent
- certainty/provenance

This is not a replacement human language. It is a semantic representation system for AI.

## Core principle

Do not treat etymology as the "true" meaning of a word. Avoid the etymological fallacy.

The desired model is:

origin -> morphology -> historical development -> current usage -> pragmatic intent

## Example

"Iconoclast" should not be represented only as "rebel."

A richer representation includes:

- Greek eikon: image, likeness, icon
- Greek klastes: breaker
- historical religious image-breaking
- later extension to attacking revered institutions or beliefs
- modern meanings: reformer, vandal, contrarian, critic, challenger of orthodoxy
- pragmatic stance: praise, criticism, irony, branding, or warning depending on context

## Development guidance

Before changing code:

1. Read the README, docs, examples, tests, package files, configs, and main source directories.
2. Identify the architecture and current abstractions.
3. Preserve existing behavior unless asked to change it.
4. Prefer small, reviewable changes.
5. Add or update tests when changing logic.
6. Keep uncertainty explicit: attested, reconstructed, inferred, speculative, disputed, or unknown.
7. Keep etymology, morphology, semantic drift, and pragmatics as separate layers.
8. Make semantic expansions explainable and traceable.

## Preferred improvements

Prioritize:

- semantic capsule schemas
- provenance and uncertainty tracking
- word-to-paragraph expansion examples
- morphology and etymology decomposition
- pragmatic stance modeling
- multilingual extensibility
- clear docs explaining the project's theory of meaning
