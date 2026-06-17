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
