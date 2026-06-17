# Glyph Controller Benchmark Gate

This gate defines what must be true before the project can honestly claim that Glyph is best in its lane for tiny-model agentic harness control.

See [adjacent-systems.md](adjacent-systems.md) for the current comparison against nearby LLM programming, structured-generation, and agent orchestration projects.

## Objective

Glyph should let a roughly 1B-parameter controller model emit compact, executable harness programs that outperform larger unconstrained or weakly constrained models on structured orchestration tasks.

The claim is not that Glyph replaces general programming languages. The claim is that Glyph is a better control surface for harness execution when the model is small and the runtime owns validation, tracing, repair, and tool calls.

## World Model

The benchmark world is local-only and harness-driven:

- models receive natural-language workflow requests
- models produce either Glyph, another structured control representation, or prose
- GlyphVM parses, validates, executes, and traces programs
- tools are mocked unless a benchmark explicitly registers a safe domain harness
- no real shell execution or external side effects are allowed

## Required Baselines

A real benchmark run must compare Glyph against at least these baselines:

- direct prose plan: model writes a natural-language plan that is not executable
- plain Glyph prompt: model is asked for Glyph with no grammar or schema prompt
- schema-only Glyph prompt: model receives the JSON output schema but not the Glyph grammar
- constrained Glyph prompt: model receives the JSON schema and official Glyph grammar
- generic JSON tool plan: model emits a simple JSON object of tool calls with arguments
- larger direct model: larger buckets attempt the task without Glyph and are scored on executable trace production through the same direct-prose baseline

The prompt-mode comparison, generic JSON tool-plan baseline, and direct-prose baseline are supported by `cargo run -- eval-controller --prompt-mode all`. Additional baselines should be added as adapters, not as one-off scripts.

## Required Model Buckets

The benchmark must include one model per bucket:

- `1b`: the target tiny controller class
- `3b`: small controller comparison
- `7b`: medium local controller comparison
- `frontier`: teacher or high-capability reference

Use model ids that can be reproduced from the JSONL trace metadata.

## Required Probes

The eval corpus must cover:

- simple export
- bounded repair loops
- app generation plans
- docs summarization
- data cleanup
- meeting notes to tasks
- support reply drafting
- security review
- malformed or adversarial variable/context cases

The current corpus has 72 request variants. Each workflow family has normal, terse, noisy, and adversarial variants, and the executable gate requires that profile coverage before a run can pass.

## Trace Requirements

Every case result must include:

- case id and tags
- model id and parameter bucket
- adapter mode
- prompt mode
- generated Glyph and raw model output
- generated generic JSON tool plan and raw baseline model output
- generated direct-prose baseline and raw baseline model output
- parse, semantic validation, runtime, and repair-loop status
- generic JSON tool-plan parse/runtime status
- direct-prose parse/validation/runtime status
- final output count
- trace event count
- duration
- approximate input and output tokens
- estimated cost fields, even when zero
- failure reason fields for parse, validation, runtime, and generation failures

JSONL output from `--jsonl` is the benchmark trace format. Use `--stream-jsonl` for live runs so each row is flushed after its Glyph, generic JSON tool-plan, and direct-prose calls complete; this preserves partial evidence if a long benchmark is interrupted.
OpenAI-compatible live evals make three model calls per result row: Glyph, generic JSON tool-plan baseline, and direct-prose baseline.
Use `--manifest` with live runs to record the run configuration, selected cases, model buckets, prompt modes, grammar payload, git commit, dirty-tree status, artifact paths, benchmark fingerprint, aggregate summary, and coverage. The manifest is written once before model calls with `runStatus=planned`, then overwritten with `runStatus=completed` after the report is available. It stores the API-key environment variable name and whether a key was present, but never the API-key value.
Use `cargo run -- fingerprint-controller` to print the same stable SHA-256 hashes for the official grammar, schemas, primitive set, 72-case controller eval corpus, and canonical OpenAI-compatible request bodies without making model calls.
Use `cargo run -- check-controller-fingerprint-lock` to compare the current fingerprint against `spec/controller-fingerprint.lock.json`; any intentional grammar, schema, corpus, or request-contract change must update the lock in the same change.
Use `cargo run -- check-conformance` to verify that every public `.glyph` example parses, validates, executes with the mock harness, and produces trace/output evidence.
Use `cargo run -- plan-controller-live-run --artifact-dir out/live-shards --output out/live-shards/live-plan.json` to generate the staged family-by-family live-run plan before spending model calls.
Use `cargo run -- verify-controller-run <results.jsonl> <results.manifest.json>` before trusting a single run. Verification checks that the JSONL trace and manifest agree on row count, selected cases, model buckets, prompt modes, artifact path, safety flags, and the current benchmark fingerprint. It also replays stored Glyph, generic JSON tool-plan, and direct-prose outputs through the current parser, validator, and mock VM so recorded metrics must match executable behavior.
Use `cargo run -- verify-controller-shards --plan out/live-shards/live-plan.json` before merging staged shards. It verifies every planned JSONL/manifest pair against the saved plan, including expected row counts and manifest fingerprints.

Run the executable gate against any JSONL trace:

```bash
cargo run -- gate-controller out/live-controller-eval.jsonl
cargo run -- report-controller-benchmark out/live-controller-eval.jsonl --output out/benchmark-report.json
```

The gate exits nonzero unless all required checks pass. The benchmark report emits explicit comparison rows for constrained 1B Glyph versus 1B plain Glyph, generic JSON tool plans, direct prose, larger plain models, and output-token compactness baselines. Use `--no-fail` only when inspecting an expected failure such as fixture-only smoke output.

Staged live runs can be merged before gate evaluation:

```bash
cargo run -- verify-controller-shards --plan out/live-shards/live-plan.json

cargo run -- merge-controller \
  --output out/live-merged.jsonl \
  --manifest out/live-merged.manifest.json \
  --source-manifest out/live-canary.manifest.json \
  --source-manifest out/live-family-crud.manifest.json \
  out/live-canary.jsonl \
  out/live-family-crud.jsonl

cargo run -- coverage-controller out/live-merged.jsonl
cargo run -- verify-controller-run out/live-merged.jsonl out/live-merged.manifest.json
cargo run -- gate-controller out/live-merged.jsonl
cargo run -- report-controller-benchmark out/live-merged.jsonl --output out/live-benchmark-report.json
```

The merge key is adapter, parameter bucket, model id, prompt mode, grammar payload, and case id. Later files replace earlier rows.
Run `verify-controller-shards` against the saved live plan before merging so missing, stale, or row-count-mismatched shard artifacts are rejected early. Pass one `--source-manifest` for each input JSONL when writing a merged manifest. The coverage command reports missing buckets, prompt modes, target case IDs, and family/profile rows. Use it after each staged merge to plan the next live shard before running the hard gate.

## Judges

Hard correctness checks:

- generated output parses as Glyph
- GlyphIR validation passes
- all variable and `ctx.*` references resolve
- all tools are known primitives or registered harness tools
- runtime execution completes without unsafe shell execution
- every successful run emits a nonempty trace
- expected repair-loop cases produce a successful bounded repair event

Comparative metrics:

- valid program rate
- run success rate
- successful trace rate
- repair-loop success rate
- average output tokens
- estimated cost
- constrained-vs-plain lift for the same model
- Glyph-vs-generic-JSON-tool-plan lift for the same model
- Glyph-vs-direct-prose lift for the same model
- 1B constrained-vs-larger plain baseline delta
- 1B constrained Glyph compactness vs larger generic JSON tool-plan output

## Best-In-Lane Gate

Do not claim best-in-lane until a real, reproducible run shows:

- `1b` constrained Glyph has at least `0.90` valid program rate
- `1b` constrained Glyph has at least `0.85` successful trace rate
- `1b` constrained Glyph rows use `grammarPayload=gbnf` so constrained means decoder-level grammar payload, not prompt-only grammar
- `1b` constrained Glyph includes normal, terse, noisy, and adversarial rows for every workflow family
- `1b` constrained Glyph beats its own plain Glyph prompt by at least `20` percentage points in successful trace rate, or plain mode is already above `0.90`
- `1b` constrained Glyph matches or beats `3b`, `7b`, and `frontier` plain-prompt rows on successful trace rate
- `1b` constrained Glyph beats generic JSON tool-plan and direct-prose baselines on successful trace rate, and every target row records a direct-prose attempt
- `1b` constrained Glyph uses fewer output tokens than generic JSON tool-plan baselines from larger models on average
- bounded repair cases pass at least `0.80` of the time
- every failure is captured as parse, validation, runtime, repair, or generation error

If any threshold fails, the next repair should target the failed layer: language surface, grammar, parser, semantic validator, prompt, eval case, harness tool contract, or model training data.

## Reproducible Commands

Fixture smoke test:

```bash
cargo run -- eval-controller --prompt-mode all --jsonl out/fixture-controller-eval.jsonl
cargo run -- gate-controller out/fixture-controller-eval.jsonl --no-fail
```

The fixture gate report should fail `live_results`; fixture output verifies the harness, not the best-in-lane claim.

Live OpenAI-compatible run:

```bash
cargo run -- preflight-controller \
  --prompt-mode all \
  --grammar-payload gbnf \
  --model 1b=<one-billion-ish-model> \
  --model 3b=<three-billion-ish-model> \
  --model 7b=<seven-billion-ish-model> \
  --model frontier=<frontier-model> \
  --jsonl out/live-controller-eval.jsonl \
  --stream-jsonl \
  --manifest out/live-controller-eval.manifest.json

cargo run -- eval-controller \
  --adapter openai-compatible \
  --prompt-mode all \
  --grammar-payload gbnf \
  --endpoint http://localhost:11434/v1 \
  --model 1b=<one-billion-ish-model> \
  --model 3b=<three-billion-ish-model> \
  --model 7b=<seven-billion-ish-model> \
  --model frontier=<frontier-model> \
  --jsonl out/live-controller-eval.jsonl \
  --stream-jsonl \
  --manifest out/live-controller-eval.manifest.json

cargo run -- verify-controller-run out/live-controller-eval.jsonl out/live-controller-eval.manifest.json
cargo run -- gate-controller out/live-controller-eval.jsonl
cargo run -- report-controller-benchmark out/live-controller-eval.jsonl --output out/live-benchmark-report.json
```

Staged canary before the full run:

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
  --jsonl out/live-canary.jsonl \
  --stream-jsonl \
  --manifest out/live-canary.manifest.json

cargo run -- eval-controller \
  --adapter openai-compatible \
  --prompt-mode constrained \
  --grammar-payload gbnf \
  --family hello_summary \
  --profile adversarial \
  --case-limit 1 \
  --endpoint http://localhost:11434/v1 \
  --model 1b=<one-billion-ish-model> \
  --model 3b=<three-billion-ish-model> \
  --model 7b=<seven-billion-ish-model> \
  --model frontier=<frontier-model> \
  --jsonl out/live-canary.jsonl \
  --stream-jsonl \
  --manifest out/live-canary.manifest.json
```

```bash
cargo run -- verify-controller-run out/live-canary.jsonl out/live-canary.manifest.json
```

`--case`, `--tag`, `--family`, `--profile`, and `--case-limit` are for staged evidence collection only. The full gate still requires all required rows.

Prompt bundle for constrained decoding experiments:

```bash
cargo run -- eval-controller --prompt-mode all --emit-prompts out/prompts
cargo run -- verify-controller-prompt-bundle out/prompts
cargo run -- score-controller-responses \
  --prompt-bundle out/prompts \
  --responses out/responses \
  --model-id <local-model-id> \
  --bucket 1b \
  --jsonl out/offline-1b.jsonl \
  --manifest out/offline-1b.manifest.json
```

The prompt bundle writes `prompt-bundle-manifest.json` with prompt modes, grammar payload, case count, per-artifact SHA-256 hashes, an aggregate hash, and the controller fingerprint. `verify-controller-prompt-bundle` recomputes those hashes and exits nonzero if any prompt, grammar, or schema artifact changed. Archive it with local constrained-decoding runs so generated outputs can be tied back to the exact prompt/grammar surface.
Use `score-controller-responses` for local decoders that write files instead of serving an OpenAI-compatible endpoint. Save outputs under `responses/cases/<prompt-mode>/<case-id>.glyph.txt`, `<case-id>.json-tool-plan.txt`, and `<case-id>.direct-prose.txt`; the scorer emits normal JSONL and manifest artifacts with `adapterMode=offline-responses`, prompt bundle path, aggregate hash, and manifest hash. Then `verify-controller-run`, merge, coverage, and gate commands apply unchanged.

OpenAI-compatible request preview before live runs:

```bash
cargo run -- preview-controller-requests \
  --model-id <model-id> \
  --prompt-mode constrained \
  --grammar-payload gbnf \
  --case-limit 1 \
  --output out/request-preview.json
```

The preview uses the same request-body builder as the live eval adapter and should show a `grammar` field for constrained Glyph requests when `--grammar-payload gbnf` is selected. It also includes the generic JSON tool-plan and direct-prose baseline request bodies.

Deterministic controller dataset export for supervised fine-tuning:

```bash
cargo run -- export-controller-dataset \
  --output out/controller-dataset.jsonl \
  --manifest out/controller-dataset.manifest.json
cargo run -- verify-controller-training-export out/controller-dataset.manifest.json
```

Each JSONL record includes the request, target Glyph, validated GlyphIR, normalized mock-harness trace, final outputs, variables, metadata, and a prompt/completion pair. The default split assigns every eighth record to validation. The optional manifest records the JSONL byte count, SHA-256 hash, controller fingerprint, git provenance, selected filters, and split policy. The verifier recomputes the artifact hash and current controller fingerprint before training.

Dataset quality gate:

```bash
cargo run -- check-controller-dataset
```

The scorecard checks record count, train/validation split coverage, workflow family/profile coverage, bounded repair examples, trace completeness, final outputs, training-pair integrity, and compact target lengths.

Controller curriculum export for tiny-model training:

```bash
cargo run -- export-controller-curriculum \
  --output out/controller-curriculum.jsonl \
  --manifest out/controller-curriculum.manifest.json
cargo run -- verify-controller-training-export out/controller-curriculum.manifest.json
cargo run -- check-controller-curriculum
```

The curriculum keeps the positive target Glyph records and adds rejected-negative and repair records generated from parser and semantic-validator failures. This gives a small controller examples of compact valid output, invalid output to reject, and invalid output to correct before any 1B training run. The optional manifest hashes the curriculum JSONL with the same provenance fields as the dataset export.

Parser and semantic-validator robustness:

```bash
cargo run -- check-controller-robustness
```

The robustness check mutates canonical targets with unknown tools, unknown variables, and invalid repair-loop bounds. It must pass before live benchmark evidence is trusted.

Claim-readiness audit:

```bash
cargo run -- audit-controller-claim \
  --jsonl out/live-merged.jsonl \
  --manifest out/live-merged.manifest.json

cargo run -- status-controller-claim \
  --jsonl out/live-merged.jsonl \
  --manifest out/live-merged.manifest.json \
  --require-claim-ready
```

The audit composes fingerprint, conformance, dataset, curriculum, robustness, adjacent-systems documentation, run verification, coverage, and benchmark-gate checks. The status command turns that audit into `claimAllowed`, `phase`, blocking reasons, and next actions. It should be the final local command before any public best-in-lane claim.

Export the reviewable evidence pack:

```bash
cargo run -- export-controller-evidence-pack \
  --output out/evidence-pack \
  --jsonl out/live-merged.jsonl \
  --manifest out/live-merged.manifest.json

cargo run -- verify-controller-evidence-pack out/evidence-pack
```

The pack writes `fingerprint.json`, `fingerprint-lock.json`, `conformance.json`, `dataset-quality.json`, `curriculum-quality.json`, `robustness.json`, `live-plan.json`, `request-preview.json`, `status.json`, `claim-audit.json`, `summary.json`, `README.md`, `evidence-manifest.json`, and, when live evidence is supplied, `verification.json`, `coverage.json`, `gate.json`, and `benchmark-report.json`. The manifest records each generated artifact's byte count and SHA-256 hash plus an aggregate pack hash, excluding only `evidence-manifest.json` to avoid circular hashing. `verify-controller-evidence-pack` recomputes those hashes and exits nonzero if the archived pack changed. Running the export without `--jsonl` and `--manifest` is allowed for static readiness review, but that pack is not claim-ready.

## Gate Decision

- Known good: thresholds pass with reproducible JSONL traces.
- Known bad: hard correctness or trace thresholds fail.
- Unknown high risk: no live `1b` run exists.
- Continue: add baselines, traces, cases, or adapters until the gate can be judged.
