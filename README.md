# Glyph

Glyph is a compact executable control language that lets a small model operate high-level harnesses through typed commands, validation, tracing, and repair loops.

## Why Glyph Exists

Large models often spend many tokens producing plans, code, revisions, and explanations. Glyph compresses common workflows into a tiny executable control language. A small model can learn to emit Glyph programs, while GlyphVM and domain harnesses do the heavy lifting.

The core idea is that a small controller model should emit:

```glyph
flow auth_app {
  SPEC(app="auth portal", features=["login", "signup", "reset"]) -> spec
  PLAN(spec) -> plan
  GEN(plan, stack="nextjs") -> files
  CHECK(files, using=["types", "tests"]) -> report
  FIX(files, report, max=3) -> final
  EXPORT(final)
}
```

Instead of generating an entire application, long plan, or large response directly.

## What Glyph Is Not

Glyph is not a replacement for Python, TypeScript, or normal programming languages.
Glyph is not a chatbot.
Glyph is a model-friendly control layer.

## Architecture

```text
User request
  ->
Small controller model
  ->
Glyph program
  ->
GlyphIR
  ->
GlyphVM
  ->
Harness primitives
  ->
Trace, checks, repair, final output
```

## Example

```glyph
goal "Build a CRUD app for projects and tasks"

ctx {
  stack: "nextjs"
  db: "postgres"
  auth: "email"
}

flow main {
  SPEC(app="project tracker", entities=["project", "task"], auth=ctx.auth) -> spec
  PLAN(spec) -> plan
  GEN(plan, stack=ctx.stack, db=ctx.db) -> files
  CHECK(files, using=["types", "tests", "lint"]) -> report
  FIX(files, report, max=3) -> final
  EXPORT(final, format="file_bundle")
}
```

## How To Run

```bash
cargo test
cargo build
cargo run -- run src/examples/build_crud_app.glyph
```

The CLI also resolves `examples/build_crud_app.glyph` to `src/examples/build_crud_app.glyph`, so this works:

```bash
cargo run -- run examples/build_crud_app.glyph
```

Available commands:

```bash
cargo run -- parse examples/build_crud_app.glyph
cargo run -- run examples/build_crud_app.glyph
cargo run -- format examples/build_crud_app.glyph
cargo run -- check examples/build_crud_app.glyph
cargo run -- compress examples/build_crud_app.glyph
cargo run -- spec glyph-ir.schema.json
cargo run -- grammar --format gbnf
cargo run -- eval-controller
```

## Language Surface

Glyph supports:

- `goal "..."` top-level declarations
- `ctx { ... }` literal context declarations
- `flow name { ... }` blocks
- primitive calls such as `SPEC(...) -> spec`
- variable references such as `PLAN(spec)`
- context references such as `GEN(plan, stack=ctx.stack)`
- strings, numbers, booleans, arrays, and object literals
- comments with `#`
- bounded repair loops:

```glyph
repair files with report max 3 {
  FIX(files, report) -> files
  CHECK(files, using=["tests"]) -> report
}
```

## MVP Primitives

The mock harness includes:

- `SPEC`
- `PLAN`
- `GEN`
- `CHECK`
- `FIX`
- `PATCH`
- `SUM` and `SUMMARIZE`
- `ASK`
- `EXPORT`
- `RUN`
- `READ`
- `WRITE`

`RUN` is mocked and does not execute a real shell command in the MVP.

## Constrained Decoding

Glyph ships grammar artifacts for controller models:

```bash
cargo run -- grammar --format ebnf
cargo run -- grammar --format gbnf
cargo run -- grammar --format json-schema
```

- `ebnf` documents the official language grammar.
- `gbnf` is suitable for llama.cpp-style constrained decoding experiments.
- `json-schema` wraps model output as `{ "glyph": "..." }` for systems that can constrain JSON but not arbitrary source text.

The runtime still parses and validates every generated program. Grammar-constrained decoding is a generation aid, not a replacement for GlyphIR validation.

## Spec-First Design

Glyph is organized so the Rust implementation and any future implementation can target a stable language contract instead of copying runtime internals.

Canonical artifacts live in `spec/`:

- `spec/glyph.ebnf`
- `spec/glyph.gbnf`
- `spec/controller-output.schema.json`
- `spec/glyph-ir.schema.json`
- `spec/fixtures/*.glyph`
- `spec/fixtures/*.ir.json`
- `spec/fixtures/*.trace.json`

Print an artifact from the CLI:

```bash
cargo run -- spec glyph-ir.schema.json
```

Compatibility target for any implementation:

1. Parse every `spec/fixtures/*.glyph` file.
2. Emit exactly the matching `*.ir.json`.
3. Execute with the mock harness semantics and emit the matching normalized `*.trace.json`.

The Rust test suite enforces that the reference implementation stays aligned with these spec files.

## Controller Eval

The controller eval measures whether a model-sized controller can turn natural requests into executable Glyph:

```bash
cargo run -- eval-controller
```

The current eval includes fixture adapters for `1b`, `3b`, `7b`, and `frontier` buckets. These are not live model calls; they make the benchmark harness runnable without credentials and define the metrics that real adapters must report:

- valid program rate
- run success rate
- successful trace rate
- Glyph-over-direct-prose rate
- repair loop success rate
- approximate input and output tokens
- configured cost estimate

The eval cases include direct natural-language plans that fail parsing because they are not executable programs, paired with equivalent Glyph programs that parse, validate, run, emit traces, and export artifacts.

Live model adapters are future work in the Rust implementation. The current eval runner records the metrics and fixture shape needed for real 1B, 3B, 7B, and frontier comparisons.

## How To Add A New Primitive

Tools are registered through `ToolRegistry`. GlyphVM does not hard-code tool behavior.

```rust
use glyph::harness::tool_registry::ToolRegistry;
use glyph::harness::types::{ToolResult, ToolStatus};
use serde_json::json;

let mut registry = ToolRegistry::new();

registry.register("CLASSIFY", |args, _ctx| {
    Ok(ToolResult {
        status: ToolStatus::Pass,
        value: json!({
            "label": "example",
            "input": args.get("input").cloned()
        }),
        summary: "Input classified".to_string(),
        warnings: vec![],
    })
});
```

Any Glyph program can then call:

```glyph
CLASSIFY(text="hello") -> result
```

## How To Add A New Domain Harness

Domains such as code generation, documentation, support, and data cleanup should add their own tool registrations. The runtime stays the same:

1. Parse Glyph source.
2. Compile to GlyphIR.
3. Validate with Rust IR validators.
4. Resolve variables and context.
5. Call registered harness primitives.
6. Emit a trace and final outputs.

## Future Work

- train a 1B controller to emit Glyph
- generate synthetic Glyph traces from a larger teacher model
- add live model adapters for the controller eval
- add domain harnesses
- add codegen harness
- add validators
- add repair loops
- add model fallback routing
- benchmark Glyph controller vs direct generation model
- create a dataset of natural language request -> Glyph program -> harness trace -> final output
- publish the Rust crate and standalone CLI binary
