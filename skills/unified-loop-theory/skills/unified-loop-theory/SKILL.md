---
name: unified-loop-theory
description: Use when Codex is asked to create or run a master loop, unified loop, objective-driven control loop, recursive improvement loop, QA/eval loop, agentic QA system, release gate, synthetic test loop, production replay loop, trace capture plan, LLM judge stack, taste/product-quality review loop, or failure-diagnosis workflow. Helps Codex turn vague goals into objectives, worlds, probes, traces, judges, repairs, memory updates, gates, and the next loop iteration.
---

# Unified Loop Theory

## Overview

Use this skill as an operating protocol for turning any vague goal into a loop that can run.

```text
Objective -> World -> Probe -> Trace -> Judge -> Repair -> Memory -> Gate -> Objective
```

Do not stop at "this could be useful" or a static plan. Instantiate the loop, run the smallest meaningful probe, judge the trace, repair the right layer, update memory, and choose the next loop.

## Core Thesis

Every useful improvement loop has the same shape:

```text
Define good.
Challenge the system.
Observe what happened.
Judge the trace.
Repair the right layer.
Store the lesson.
Decide whether to ship, continue, escalate, or redefine the objective.
Repeat.
```

The system improves, but so can the loop around it. If the objective, world, probe, trace, judge, memory, or gate is weak, repair that layer instead of assuming the system itself is the only thing to change.

## Core Objects

- Objective: what good means, including desired outcome, hard constraints, quality bars, and preference direction.
- System: the app, service, model, workflow, prompt, repo, process, or product behavior being improved.
- World: the environment the system operates inside, including users, data, APIs, models, devices, networks, time, production states, and constraints.
- Probe: a challenge to the system, such as a test, scenario, task, replay, synthetic user journey, fuzz case, adversarial case, or experiment.
- Trace: the full record of what happened when the system was challenged.
- Judge: deterministic checks, metrics, rubrics, LLM judges, human review, production metrics, or taste calls that score the trace against the objective.
- Repair: the decision about what to change: system, objective, world, probe, instrumentation, judge, memory, or gate.
- Memory: stored traces, failures, fixes, examples, preferences, regressions, and decisions.
- Gate: the decision to ship, continue, warn, canary, block, or escalate.

## Master Loop Protocol

When applying this skill to a repo, app, product, or task:

1. Inspect the system.
   - Read the existing repo, docs, tests, evals, CI, telemetry, fixtures, product flows, or prior failures.
   - Identify current loops that already exist; preserve useful ones.

2. Choose the highest-leverage objective.
   - Pick one objective that matters now.
   - Prefer a concrete critical flow, failure class, user outcome, agent behavior, release risk, or product-quality bar.
   - If the user's goal is too broad, create the first narrow objective that can be probed.

3. Define the loop state.
   - Objective: what good means.
   - World: what reality must be modeled.
   - Probes: how the system will be challenged.
   - Trace: what evidence will be captured.
   - Judge: how evidence will be scored.
   - Repair: what can change.
   - Memory: what should compound.
   - Gate: how the next decision will be made.

4. Run the smallest meaningful probe.
   - Prefer executable probes over abstract checklists.
   - If execution is impossible, make instrumentation or harness setup the first probe.

5. Capture the trace.
   - Record enough evidence to judge behavior and create a future regression.
   - Include inputs, actions, state changes, logs, errors, screenshots, API calls, AI calls, latency, outputs, and final state when relevant.

6. Judge the trace.
   - Score hard correctness first.
   - Then judge task success, UX, AI quality, safety, taste, cost, latency, and real-world outcome as needed.
   - Classify certainty: known good, known bad, unknown low risk, unknown high risk, judge disagreement, taste uncertainty, or reality uncertainty.

7. Diagnose the failed layer.
   - Classify whether the failure came from code, data, dependency, AI behavior, UX, taste, objective, judge, world model, probe, trace capture, memory, or gate policy.
   - Repair the failed layer.

8. Update memory.
   - Store the trace, score, failure, decision, repair, and next probe.
   - Convert escaped failures into regressions and excellent outcomes into golden examples.

9. Gate and continue.
   - Decide: ship, continue, warn, canary, block, human-review, or redefine objective.
   - Start the next loop from the updated objective and memory.

## Recursive Repair Rule

Use the loop on the loop:

- If the objective is vague, repair the objective.
- If the world model misses reality, repair the world model.
- If probes are weak, repair the probe generator.
- If traces lack evidence, repair instrumentation.
- If judges score incorrectly, repair or calibrate the judge.
- If repairs do not stick, repair memory and regression coverage.
- If gates allow bad releases or block good ones, repair the gate policy.

## Applying To Repos And Apps

- Fit the loop into existing repo patterns instead of creating a parallel QA universe.
- If no structure exists, propose a minimal `qa-loop/`, `evals/`, or equivalent folder with objectives, scenarios, world profiles, judges, traces, memory, and gates.
- Keep the first implementation small: one objective, 5-20 probes, one trace format, a few deterministic judges, one subjective rubric if needed, and a simple gate.
- For frontend/live-app flows, include browser traces, screenshots, user actions, latency, network/API calls, and final visible state.
- For AI-powered flows, include AI profiles: good, mediocre, wrong, slow, empty, malformed, unsafe, and expensive. Judge schema validity, grounding, instruction following, usefulness, safety, tone, concision, cost, latency, fallback behavior, and product taste.
- For production learning, map incidents, support tickets, user complaints, session replays, analytics drop-offs, and excellent outcomes back into scenarios or judge calibration examples.

## Output Shape

When asked to apply the master loop, produce the current loop state and then act:

1. Objective: desired outcome, hard constraints, quality bars, preference direction.
2. World: the minimal user/data/API/AI/environment model needed.
3. Probe: the first executable challenge and the backlog of next probes.
4. Trace plan: what to capture and where to store it.
5. Judge stack: checks, metrics, rubrics, LLM judges, and human review rules.
6. Failure taxonomy: how failures should be classified.
7. Memory rules: what becomes a regression, golden example, bad example, or calibration case.
8. Gate: ship/warn/canary/review/block/continue rules.
9. First action: the smallest concrete implementation or execution step.
10. Next loop: what to do after the first trace is judged.

## Gate Decisions

Use this default policy unless the codebase or user supplies a stricter one:

```text
Known good -> ship
Known bad -> block
Unknown low risk -> ship with warning or monitor
Unknown high risk -> canary or human review
Judge disagreement -> human review
Taste uncertainty -> product/founder review
Reality uncertainty -> limited rollout with production learning
Incomplete trace -> improve instrumentation and rerun
Weak objective -> redefine objective and rerun
```

Hard failures block by default: privacy violations, security regressions, data loss, payment integrity failures, permission failures, crashes in critical flows, invalid required structured output, or critical task failure.

Soft failures warn, continue, or escalate: UX friction, latency regression, mediocre AI quality, taste misalignment, confusing copy, non-critical accessibility issues, or uncertain product direction.

## Failure Diagnosis Taxonomy

Classify failed traces using these categories:

- Code failure: logic, state, UI, backend, race condition, migration, or integration bug.
- Data failure: missing, dirty, duplicate, inconsistent, malformed, or unrealistic data.
- Dependency failure: third-party API, webhook, email, payment, storage, timeout, schema drift, or outage.
- AI failure: wrong, hallucinated, unsafe, malformed, slow, expensive, ungrounded, or bad tool use.
- UX failure: confusing flow, dead end, unclear copy, poor loading state, too many steps, or missing feedback.
- Taste failure: technically works but feels generic, noisy, cheap, misaligned, overbuilt, or off-brand.
- Objective failure: goal is vague, contradictory, stale, unowned, or missing a hard constraint.
- Judge failure: judge missed a real issue, over-penalized acceptable behavior, or lacked calibration examples.
- World-model failure: synthetic/mocked world did not represent production reality.
- Probe failure: scenario did not challenge the right behavior or missed an important path.
- Trace failure: instrumentation did not capture enough evidence.
- Memory failure: lessons did not become reusable regressions or examples.
- Gate failure: the loop shipped, blocked, warned, or escalated incorrectly.

## Memory Rules

- Every production incident becomes a scenario or regression.
- Every escaped bug becomes a regression probe.
- Every great experience becomes a golden trace.
- Every bad experience becomes a bad trace.
- Every ambiguous review becomes a labeled example.
- Every judge mistake updates judge calibration.
- Every synthetic-world miss updates the world model.
- Every user complaint becomes a probe candidate.
- Every release decision is stored with the evidence used to make it.
- Every loop iteration updates the next objective, probe, or gate.

## References

- Read `references/templates.md` when creating concrete YAML/spec artifacts for objectives, scenarios, loop state, traces, judges, gates, or folder structure.
- Read `references/objective-trace-control-loop.md` only when the user asks for the full framework, theory, maturity model, dashboard, 30-day rollout, or a more complete explanation.
