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
cargo run -- check-conformance
cargo run -- plan-controller-live-run --output out/live-plan.json
cargo run -- eval-controller
cargo run -- preview-controller-requests --prompt-mode constrained --grammar-payload gbnf --case-limit 1
cargo run -- export-controller-dataset --output out/controller-dataset.jsonl
cargo run -- check-controller-dataset
cargo run -- export-controller-curriculum --output out/controller-curriculum.jsonl
cargo run -- check-controller-curriculum
cargo run -- check-controller-robustness
cargo run -- coverage-controller out/results.jsonl
cargo run -- gate-controller out/results.jsonl
cargo run -- report-controller-benchmark out/results.jsonl --no-fail
cargo run -- audit-controller-claim --jsonl out/results.jsonl --manifest out/results.manifest.json
cargo run -- status-controller-claim --jsonl out/results.jsonl --manifest out/results.manifest.json
cargo run -- export-controller-evidence-pack --output out/evidence-pack
cargo run -- merge-controller --output out/merged.jsonl out/canary-a.jsonl out/canary-b.jsonl
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

For OpenAI-compatible servers that accept llama.cpp-style grammar payloads, pass `--grammar-payload gbnf` during live evals. Without that flag, constrained mode includes the grammar in the prompt but does not request decoder-level grammar enforcement.

Export a prompt bundle for local grammar-constrained decoding experiments:

```bash
cargo run -- eval-controller --prompt-mode all --emit-prompts out/prompts
```

The bundle includes `glyph.gbnf`, `controller-output.schema.json`, `generic-tool-plan.schema.json`, and one JSON prompt file per eval case per selected prompt mode. Each prompt file includes the Glyph prompt, the generic JSON tool-plan baseline prompt, and the no-Glyph direct-prose baseline prompt.

Preview exact OpenAI-compatible request bodies without making model calls:

```bash
cargo run -- preview-controller-requests \
  --model-id <model-id> \
  --prompt-mode constrained \
  --grammar-payload gbnf \
  --case-limit 1 \
  --output out/request-preview.json
```

The preview includes the Glyph request, generic JSON tool-plan baseline request, and direct-prose baseline request. For constrained Glyph rows with `--grammar-payload gbnf`, the preview shows the `grammar` field that will be sent to llama.cpp-style OpenAI-compatible servers.

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

The current eval includes 72 request variants across app generation, repair, docs, data cleanup, meeting tasks, support, security review, and simple export workflows. Each workflow family includes normal, terse, noisy, and adversarial profiles.

By default it uses fixture adapters for `1b`, `3b`, `7b`, and `frontier` buckets. Fixture mode makes the benchmark harness runnable without credentials and defines the metrics that real adapters must report:

- valid program rate
- run success rate
- successful trace rate
- Glyph-over-direct-prose rate
- repair loop success rate
- generic JSON tool-plan run success rate
- generic JSON tool-plan successful trace rate
- Glyph-over-generic-JSON-tool-plan rate
- direct-prose successful trace rate
- approximate input and output tokens
- configured cost estimate
- raw model output and extracted Glyph
- raw generic JSON tool-plan output
- raw direct-prose baseline output
- parse, validation, and runtime errors

Each eval row records three model-facing attempts: a Glyph program, a generic JSON tool plan, and a no-Glyph direct-prose plan. The direct-prose baseline is intentionally scored on whether it can produce an executable trace; fixture rows fail that baseline because ordinary prose is not an executable harness program.

Prompt modes let the same model be tested under progressively weaker constraints:

- `constrained`: schema/grammar constrained Glyph generation; with `--grammar-payload gbnf`, the request carries the official Glyph GBNF decoder grammar and the model returns raw Glyph source.
- `schema-only`: JSON schema, no grammar.
- `plain`: no schema or grammar in the prompt; the model is simply asked to return Glyph source.

Run all prompt modes in fixture mode:

```bash
cargo run -- eval-controller --prompt-mode all
```

Judge a saved JSONL run against the benchmark gate:

```bash
cargo run -- verify-controller-run out/results.jsonl out/results.manifest.json
cargo run -- gate-controller out/results.jsonl
cargo run -- report-controller-benchmark out/results.jsonl --output out/benchmark-report.json
```

Fixture-only JSONL is useful for smoke tests but cannot pass the gate. Passing requires live OpenAI-compatible rows for `1b`, `3b`, `7b`, and `frontier` buckets across all prompt modes.

Run a live OpenAI-compatible comparison by providing one model per bucket:

```bash
cargo run -- preflight-controller \
  --prompt-mode all \
  --grammar-payload gbnf \
  --model 1b=<one-billion-ish-model> \
  --model 3b=<three-billion-ish-model> \
  --model 7b=<seven-billion-ish-model> \
  --model frontier=<frontier-model> \
  --jsonl out/results.jsonl \
  --stream-jsonl \
  --manifest out/results.manifest.json

cargo run -- eval-controller \
  --adapter openai-compatible \
  --prompt-mode all \
  --grammar-payload gbnf \
  --endpoint http://localhost:11434/v1 \
  --model 1b=<one-billion-ish-model> \
  --model 3b=<three-billion-ish-model> \
  --model 7b=<seven-billion-ish-model> \
  --model frontier=<frontier-model> \
  --jsonl out/results.jsonl \
  --stream-jsonl \
  --manifest out/results.manifest.json
```

For remote providers, set `GLYPH_EVAL_API_KEY` or pass a different environment variable name with `--api-key-env`.
Use `preflight-controller` before live runs to check model buckets, GBNF settings, selected cases, artifact paths, and expected row/model-call counts without making model calls.
OpenAI-compatible live evals make three model calls per result row: Glyph, generic JSON tool-plan baseline, and direct-prose baseline.
Use `--stream-jsonl` for live runs so each completed case is flushed to disk before the next model call.
Use `--manifest` to write reproducibility metadata: selected cases, model buckets, prompt modes, grammar payload, git commit, dirty-tree status, artifact paths, benchmark fingerprint, aggregate report summary, and coverage. The manifest records the API-key environment variable name and whether a key was present, but never stores the key value.
`verify-controller-run` checks that the JSONL trace and manifest agree on row count, selected cases, model buckets, prompt modes, artifact path, safety flags, and the current benchmark fingerprint before the benchmark gate is trusted. The fingerprint covers grammar/schema artifacts, the eval corpus, and canonical OpenAI-compatible request bodies for Glyph, generic JSON tool-plan, and direct-prose baselines.
`report-controller-benchmark` turns a JSONL run into explicit comparison rows for 1B constrained Glyph against 1B plain Glyph, generic JSON tool plans, direct prose, larger plain models, and output-token compactness baselines.
`audit-controller-claim` composes fingerprint, conformance, dataset, curriculum, robustness, documentation, verification, coverage, and benchmark-gate checks into one claim-readiness report. It fails unless live evidence is supplied and all proof checks pass; use `--no-fail` to inspect missing evidence.
`status-controller-claim` summarizes the audit into a machine-readable phase, blocking reasons, and next actions.
`export-controller-evidence-pack` writes the fingerprint, conformance report, dataset quality report, curriculum quality report, robustness report, request preview, claim status, claim audit, optional live verification/gate/coverage/benchmark reports, and an `evidence-manifest.json` seal into one directory for review. The manifest records each generated artifact's byte count and SHA-256 hash, plus an aggregate pack hash, excluding only the manifest itself to avoid circular hashing.
`verify-controller-evidence-pack` recomputes that seal and exits nonzero if any listed artifact is missing or has changed.

Print the benchmark identity without running models:

```bash
cargo run -- fingerprint-controller
cargo run -- check-conformance
cargo run -- plan-controller-live-run --output out/live-plan.json
```

`check-conformance` treats the checked-in `.glyph` examples as public compatibility targets. Every example must parse, validate, execute on the mock harness, and produce a trace plus final output.
`plan-controller-live-run` emits a staged family-by-family live eval plan with expected row counts, model-call counts, artifact paths, and ready-to-run preflight/eval/merge/gate/status commands.

Export deterministic controller training records:

```bash
cargo run -- export-controller-dataset --output out/controller-dataset.jsonl
```

The dataset exporter turns the 72-case eval corpus into JSONL records containing the natural request, target Glyph, validated GlyphIR, normalized mock-harness trace, final outputs, variables, metadata, and a prompt/completion pair for supervised controller training. By default every eighth record is assigned to `validation`; use `--no-validation-split` or the standard `--case`, `--family`, `--profile`, and `--case-limit` filters for focused shards.

Check dataset training readiness:

```bash
cargo run -- check-controller-dataset
```

The scorecard fails if the corpus loses record count, train/validation split coverage, family/profile coverage, bounded repair examples, normalized traces, final outputs, training-pair integrity, or compact target lengths.

Export the controller curriculum for tiny-model training:

```bash
cargo run -- export-controller-curriculum --output out/controller-curriculum.jsonl
cargo run -- check-controller-curriculum
```

The curriculum expands the deterministic positive dataset with rejected-negative examples and repair examples. Each eval case contributes one correct Glyph target, three invalid candidates with parser or semantic-validator feedback, and three correction prompts whose assistant target is the canonical Glyph program.

Check parser and semantic-validator robustness against deterministic invalid mutations:

```bash
cargo run -- check-controller-robustness
```

The robustness check mutates every canonical controller target with unknown tools and variables, and mutates repair-loop targets with invalid repair bounds. The check passes only if all invalid mutations are rejected.

Audit claim readiness after verification and gate checks:

```bash
cargo run -- audit-controller-claim \
  --jsonl out/live-merged.jsonl \
  --manifest out/live-merged.manifest.json

cargo run -- status-controller-claim \
  --jsonl out/live-merged.jsonl \
  --manifest out/live-merged.manifest.json \
  --require-claim-ready
```

Export a reviewable evidence pack:

```bash
cargo run -- export-controller-evidence-pack \
  --output out/evidence-pack \
  --jsonl out/live-merged.jsonl \
  --manifest out/live-merged.manifest.json

cargo run -- verify-controller-evidence-pack out/evidence-pack
```

Without `--jsonl` and `--manifest`, the pack still exports static readiness artifacts and a claim audit that marks live evidence as missing.
Every evidence pack includes `evidence-manifest.json`, which hashes the generated artifacts so reviewers can archive and recheck the exact pack contents.

Generate the staged live-run plan:

```bash
cargo run -- plan-controller-live-run --output out/live-plan.json
```

Use filters for staged live canaries before the full gate run:

```bash
cargo run -- preflight-controller \
  --prompt-mode constrained \
  --grammar-payload gbnf \
  --family hello_summary \
  --profile adversarial \
  --case-limit 1 \
  --model 1b=<one-billion-ish-model> \
  --model 3b=<three-billion-ish-model> \
  --model 7b=<seven-billion-ish-model> \
  --model frontier=<frontier-model> \
  --jsonl out/canary.jsonl \
  --stream-jsonl \
  --manifest out/canary.manifest.json

cargo run -- eval-controller \
  --adapter openai-compatible \
  --prompt-mode constrained \
  --grammar-payload gbnf \
  --family hello_summary \
  --profile adversarial \
  --case-limit 1 \
  --model 1b=<one-billion-ish-model> \
  --model 3b=<three-billion-ish-model> \
  --model 7b=<seven-billion-ish-model> \
  --model frontier=<frontier-model> \
  --jsonl out/canary.jsonl \
  --stream-jsonl \
  --manifest out/canary.manifest.json
```

Filters available for staged runs and prompt export are `--case`, `--tag`, `--family`, `--profile`, and `--case-limit`.

Merge staged live JSONL files before running the gate:

```bash
cargo run -- verify-controller-run out/canary.jsonl out/canary.manifest.json

cargo run -- merge-controller \
  --output out/live-merged.jsonl \
  --manifest out/live-merged.manifest.json \
  --source-manifest out/canary.manifest.json \
  --source-manifest out/live-family-crud.manifest.json \
  out/canary.jsonl \
  out/live-family-crud.jsonl

cargo run -- coverage-controller out/live-merged.jsonl
cargo run -- verify-controller-run out/live-merged.jsonl out/live-merged.manifest.json
cargo run -- gate-controller out/live-merged.jsonl
cargo run -- report-controller-benchmark out/live-merged.jsonl --output out/live-benchmark-report.json
```

Verify every staged JSONL/manifest pair before merging. Pass one `--source-manifest` for each input JSONL when writing a merged manifest. Merge dedupes by adapter, parameter bucket, model id, prompt mode, grammar payload, and case id. Later files replace earlier rows, so failed canaries can be rerun without hand-editing JSONL.
Coverage reports missing live buckets, prompt modes, target case IDs, and family/profile rows before the stricter gate is run.

The benchmark gate for claiming Glyph is best in its lane is documented in [docs/benchmark-gate.md](docs/benchmark-gate.md). Adjacent systems and lane boundaries are tracked in [docs/adjacent-systems.md](docs/adjacent-systems.md). Until real model runs pass that gate, the repo should describe Glyph as a strong candidate architecture, not as proven superior.

## Semantic Validation

`glyph check` performs structural and semantic validation before runtime execution:

- tool names must be registered MVP primitives
- variables must be defined before use
- `ctx.foo` references must exist
- step ids must be unique and flows must contain executable steps
- `{ "var": ... }` and `{ "ctx": ... }` IR sentinels must be well-formed
- repair loop target and report variables must exist before the loop
- repair loops must use `maxIterations` from `1` to `10`
- repair loops must update both the target variable and report variable inside the loop
- assignments must use valid identifiers

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
- add grammar-constrained runner integrations beyond prompt bundle export
- add domain harnesses
- add codegen harness
- continue expanding semantic validators and repair-loop policies
- add model fallback routing
- benchmark Glyph controller vs direct generation model
- expand the controller dataset with larger teacher-generated traces
- publish the Rust crate and standalone CLI binary
