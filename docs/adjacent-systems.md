# Adjacent Systems

Last checked: 2026-06-17.

This document defines Glyph's lane against nearby projects. The point is not to claim that Glyph is better than mature orchestration or structured-generation systems in their own lanes. The claim under test is narrower:

> Glyph should be a compact executable control language that a roughly 1B controller model can emit to operate typed harness primitives with validation, tracing, bounded repair, and measurable compression.

Until live model runs pass `docs/benchmark-gate.md`, Glyph should be described as a candidate architecture, not a proven winner.

## Glyph's Lane

Glyph is in-lane only when all of these are true:

- the model emits a small program, not a full application, full agent framework, or long prose plan
- the program targets typed harness primitives such as `SPEC`, `PLAN`, `GEN`, `CHECK`, `FIX`, `RUN`, and `EXPORT`
- the runtime parses, validates, executes, and traces the program deterministically where possible
- repair loops are explicit and bounded
- the benchmark is centered on tiny-controller viability, especially the `1b` bucket
- baselines include plain prompting, schema-only prompting, generic JSON tool plans, larger plain models, and direct prose

## Comparison Matrix

| Project | Public positioning | What overlaps | What it does better today | Why Glyph is different |
| --- | --- | --- | --- | --- |
| [LMQL](https://github.com/eth-sri/lmql) | A programming language for LLM interaction using Python-like programs, constraints, and an optimizing runtime. | Language-level control over LLM calls and constrained outputs. | Mature LLM programming surface for developers who write the program. | Glyph is designed as the model's emitted artifact, not primarily as a developer-authored prompt/program language. |
| [Guidance](https://github.com/guidance-ai/guidance) and [llguidance](https://github.com/guidance-ai/llguidance) | Python/Rust tooling for steering and constraining language model outputs, including regex/CFG-style constraints. | Constrained decoding can help a model emit valid Glyph. | More mature decoder-control machinery. | Guidance can be a generation backend for Glyph; it is not itself a typed harness-control language with GlyphIR, GlyphVM, and benchmark gates for 1B controllers. |
| [Outlines](https://github.com/dottxt-ai/outlines) | Structured generation for schemas, grammars, and constrained outputs. | Grammar/schema constrained generation for valid controller outputs. | Stronger structured-output ecosystem and schema compliance focus. | Outlines can enforce Glyph syntax, but it does not define the harness-control language, repair semantics, trace format, or tiny-controller benchmark. |
| [SGLang](https://github.com/sgl-project/sglang) | High-performance serving/runtime framework for language-model and multimodal workloads; the paper describes structured language model programs. | Runtime and language ideas for efficient LLM programs. | Serving performance and complex LLM program execution. | Glyph is intentionally smaller and model-emitted; its program operates external harness primitives rather than optimizing multi-call LLM serving. |
| [LangGraph](https://github.com/langchain-ai/langgraph) | Low-level orchestration framework for stateful, long-running agents. | Harnesses may be built with graph/state machinery. | Production-grade agent orchestration, persistence, and human-in-the-loop patterns. | Glyph is a compact controller language that could drive a LangGraph-like harness; it is not a replacement orchestration framework. |
| [AutoGen](https://github.com/microsoft/autogen) | Framework for creating multi-agent AI applications; current repo notes maintenance mode. | Multi-agent workflows and tool-using applications. | Higher-level agent application abstractions and multi-agent patterns. | Glyph avoids open-ended agent chat as the controller surface; it asks a small model for a typed executable control program. |
| [Semantic Kernel](https://github.com/microsoft/semantic-kernel) | Model-agnostic SDK for building, orchestrating, and deploying agents and multi-agent systems. | Domain harnesses can resemble SK plugins/functions. | Enterprise SDK surface, language support, integrations, and orchestration features. | Glyph is a small emitted DSL plus VM; Semantic Kernel is a developer SDK that a Glyph harness could call. |
| [DSPy](https://github.com/stanfordnlp/dspy) | Framework for programming, evaluating, and optimizing language-model pipelines. | Evaluation and optimization mindset. | Optimizers for prompts/weights and modular AI pipelines. | Glyph is the target control representation for a small model; DSPy could help optimize a model or prompt that emits Glyph. |
| Generic JSON tool plans | Any schema or function-call representation of a sequence of tool calls. | Direct baseline for typed harness calls. | Ubiquitous and easy to generate with existing structured-output APIs. | Glyph is more compact and human-scannable, with first-class flow, context, repair, grammar, IR validation, and runtime trace semantics. |

## Current Position

Based on the public docs/repositories above, these projects are adjacent rather than exact substitutes. Several are more mature in their own lanes:

- Guidance, llguidance, and Outlines are better structured-generation engines today.
- LangGraph, Semantic Kernel, and AutoGen are broader agent orchestration/application frameworks.
- DSPy is stronger for prompt/program optimization.
- SGLang is stronger for high-throughput serving and structured LLM program execution.

Glyph's defensible lane is narrower: compact, inspectable, executable harness-control programs emitted by tiny controller models. The repo is now set up to test that lane with grammar artifacts, IR validation, direct-prose and JSON-plan baselines, live-run manifests, coverage checks, a hard benchmark gate, and dataset export. It is not proven best-in-lane until live `1b` runs pass the gate.

## How Adjacent Tools Can Help Glyph

- Use Guidance/llguidance, Outlines, or llama.cpp-compatible grammars to enforce Glyph syntax at decode time.
- Use LangGraph, Semantic Kernel, or other SDKs behind GlyphKit-style harness primitives.
- Use DSPy-style optimization or larger teacher models to generate better natural-request-to-Glyph training data.
- Use SGLang-style serving ideas if controller inference throughput becomes the bottleneck.

## Evidence Standard

An adjacent project should be considered a direct competitor only if it provides all of the following in one system:

- a compact model-emitted control language for typed harness primitives
- a JSON-compatible IR
- a local deterministic VM or equivalent executor
- semantic validation before execution
- bounded repair-loop semantics
- execution traces suitable for regression and training
- constrained-decoding artifacts
- baselines against prose, generic JSON plans, and larger unconstrained models
- an explicit tiny-controller benchmark target around the `1b` class

If a project has these, it belongs in the benchmark gate as a first-class baseline rather than in this adjacent-tools list.
