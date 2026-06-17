# Semantic Control

Glyph's semantic-control role is not to make a larger model write prettier prose. The narrower claim is:

```text
Small controllers can use Glyph + Etymonoetic Interlingua to produce auditable semantic-control traces that catch hidden conflicts before generation.
```

In the current monorepo stack:

- Unified Loop Theory defines the objective, probe, trace, judge, repair, memory, and gate.
- Etymonoetic Interlingua supplies semantic capsule evidence for control-sensitive terms.
- Glyph executes the control trace and records whether gates happened before generation or export.
- `glyph-ei-bridge` turns the trace into route-level evals.

## Gate Idioms

Use existing Glyph primitives to represent richer gates without expanding the language surface prematurely:

- `ASK before GEN`: clarify contradictory or high-risk intent before drafting.
- `CHECK before EXPORT`: block final output until the draft passes the judge.
- `EVIDENCE before CLAIM`: include evidence checks in `PLAN` and `CHECK` before making claims.
- `POLICY before TOOL`: include policy checks before `RUN`, `WRITE`, or external tool use.
- `HUMAN_REVIEW before irreversible action`: require an explicit review step before export or side-effectful tools.

Example:

```glyph
PLAN(input=spec, clarification=intent, gates=["EVIDENCE before CLAIM", "CHECK before EXPORT"]) -> plan
GEN(input=plan, constraints=["avoid legal admission"]) -> draft
CHECK(target=draft, using=["liability_admission", "evidence_before_claim"]) -> report
```

The bridge evals score these as route properties. A vanilla self-loop can still produce good surface text, but it does not get credit for external EI evidence, `ASK` before `GEN`, or a machine-executable control trace.

## Outcome Proof

The next bar is outcome proof. Use:

```bash
cargo run -p glyph-ei-bridge -- outcome-suite --output out/outcome-proof-suite.json --prompt-output-dir out/outcome-prompts
```

This suite measures five claims separately:

- vanilla Codex harmful/wrong output rate
- Codex with EI + Glyph harmful/wrong output rate
- blind content-only preference between final outputs
- small-model direct vs small-model with EI + Glyph
- failures caught before export by the EI + Glyph route

The default run uses built-in fixture/proxy outputs, so the gate is `warn`, not `ship`. To make an external claim, export prompts, run them against real models, save outputs with the same scenario filenames, and rerun `outcome-suite` with `--vanilla-dir`, `--codex-ei-dir`, `--small-direct-dir`, and `--small-ei-dir`.
