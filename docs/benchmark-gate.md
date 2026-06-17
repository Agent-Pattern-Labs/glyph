# Glyph Controller Benchmark Gate

This gate defines what must be true before the project can honestly claim that Glyph is best in its lane for tiny-model agentic harness control.

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
- larger direct model: a larger model attempts the task without Glyph and is scored on executable trace production if an adapter exists

The prompt-mode comparison and generic JSON tool-plan baseline are supported by `cargo run -- eval-controller --prompt-mode all`. Additional baselines should be added as adapters, not as one-off scripts.

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
- parse, semantic validation, runtime, and repair-loop status
- generic JSON tool-plan parse/runtime status
- final output count
- trace event count
- duration
- approximate input and output tokens
- estimated cost fields, even when zero
- failure reason fields for parse, validation, runtime, and generation failures

JSONL output from `--jsonl` is the benchmark trace format.

Run the executable gate against any JSONL trace:

```bash
cargo run -- gate-controller out/live-controller-eval.jsonl
```

The gate exits nonzero unless all required checks pass. Use `--no-fail` only when inspecting an expected failure such as fixture-only smoke output.

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
- 1B constrained-vs-larger direct baseline delta

## Best-In-Lane Gate

Do not claim best-in-lane until a real, reproducible run shows:

- `1b` constrained Glyph has at least `0.90` valid program rate
- `1b` constrained Glyph has at least `0.85` successful trace rate
- `1b` constrained Glyph rows use `grammarPayload=gbnf` so constrained means decoder-level grammar payload, not prompt-only grammar
- `1b` constrained Glyph includes normal, terse, noisy, and adversarial rows for every workflow family
- `1b` constrained Glyph beats its own plain Glyph prompt by at least `20` percentage points in successful trace rate, or plain mode is already above `0.90`
- `1b` constrained Glyph beats generic JSON tool-plan and direct prose baselines on successful trace rate
- `1b` constrained Glyph uses fewer output tokens than larger direct baselines on average
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
cargo run -- eval-controller \
  --adapter openai-compatible \
  --prompt-mode all \
  --grammar-payload gbnf \
  --endpoint http://localhost:11434/v1 \
  --model 1b=<one-billion-ish-model> \
  --model 3b=<three-billion-ish-model> \
  --model 7b=<seven-billion-ish-model> \
  --model frontier=<frontier-model> \
  --jsonl out/live-controller-eval.jsonl

cargo run -- gate-controller out/live-controller-eval.jsonl
```

Staged canary before the full run:

```bash
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
  --jsonl out/live-canary.jsonl
```

`--case`, `--tag`, `--family`, `--profile`, and `--case-limit` are for staged evidence collection only. The full gate still requires all required rows.

Prompt bundle for constrained decoding experiments:

```bash
cargo run -- eval-controller --prompt-mode all --emit-prompts out/prompts
```

## Gate Decision

- Known good: thresholds pass with reproducible JSONL traces.
- Known bad: hard correctness or trace thresholds fail.
- Unknown high risk: no live `1b` run exists.
- Continue: add baselines, traces, cases, or adapters until the gate can be judged.
