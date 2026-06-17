# Unified Loop Theory Templates

Use these templates when scaffolding a concrete Unified Loop Theory / Objective Trace Loop implementation. Prefer adapting them to existing repo conventions over introducing this exact structure everywhere.

## Loop State

```yaml
loop_id: loop_<name>
version: 0.1
status: designing | running | judging | repairing | gated | complete | blocked

objective_id: obj_<name>
current_iteration: 1

system:
  name: <repo-app-product-or-process>
  scope: <what is inside the loop>
  out_of_scope:
    - <what is intentionally excluded for now>

objective:
  desired_outcome: <what good means>
  hard_constraints:
    - <thing that must never happen>
  quality_bars:
    - <measurable or judgeable bar>
  preference_direction:
    prefer:
      - <preferred behavior>
    avoid:
      - <undesired behavior>

world:
  assumptions:
    - <current assumption about reality>
  profiles:
    - <user-data-api-ai-or-environment profile>
  known_gaps:
    - <what the world model does not capture yet>

probes:
  current:
    probe_id: sc_<name>
    purpose: <why this probe matters>
  backlog:
    - <next probe>

trace:
  latest_trace_id: tr_<name>
  evidence_required:
    - <input/action/output/log/screenshot/state/etc>
  evidence_gaps:
    - <missing evidence>

judge:
  required_checks:
    - <deterministic check>
  rubrics:
    - <rubric or LLM-as-judge>
  human_review_if:
    - <condition>

repair:
  last_failed_layer: system | objective | world | probe | trace | judge | memory | gate | none
  current_repair: <change being made>

memory:
  new_lessons:
    - <lesson from latest trace>
  regressions_added:
    - <regression probe>
  golden_examples_added:
    - <golden trace>

gate:
  decision: ship | continue | warn | canary | block | human_review | redefine_objective
  reason: <evidence-backed reason>
  next_loop: <next objective/probe/repair>
```

## Objective

```yaml
objective_id: obj_<name>
name: <plain-language name>
owner: <person-or-team>
version: 0.1

mission:
  description: >
    What the user or system should accomplish.

success_outcome:
  primary: >
    The main outcome that must happen.
  secondary:
    - <secondary outcome>

hard_constraints:
  - <thing that must never happen>

quality_bars:
  - metric: <metric name>
    threshold: <pass/warn/fail threshold>

preference_direction:
  prefer:
    - <preferred behavior>
  avoid:
    - <undesired behavior>

examples:
  golden:
    - <trace-or-example-id>
  bad:
    - <trace-or-example-id>
  ambiguous:
    - <trace-or-example-id>

release_policy:
  block_if:
    - <condition>
  warn_if:
    - <condition>
  canary_if:
    - <condition>
  human_review_if:
    - <condition>
```

## Scenario

```yaml
scenario_id: sc_<name>
objective_id: obj_<name>
name: <scenario name>
risk_level: low | medium | high | critical

actor:
  type: human_replay | synthetic_user | agent | script | fuzz | adversary
  persona: <persona name>

initial_state:
  account: <state>
  data_seed: <seed>
  permissions: <permissions>
  feature_flags:
    - <flag>

world_profile:
  api_profile: normal | slow | flaky | failing | malformed
  ai_profile: good | mediocre | wrong | slow | empty | malformed | unsafe | expensive
  network_profile: fast | average | slow | offline_recovery
  device_profile: desktop | mobile | tablet

steps:
  - action: <action>
    target: <target>
    input: <input>
    expected_state: <expected state>

expected_outcome:
  task_completed: true
  final_state: <state>
  max_duration_seconds: <number>

judges:
  - hard_correctness
  - task_success
  - ux_quality
  - ai_quality
  - taste_alignment

release_policy:
  required_for_merge: true | false
  required_for_release: true | false
  human_review_on_failure: true | false
```

## Trace

```yaml
trace_id: tr_<name>
scenario_id: sc_<name>
app_version: <version-or-commit>
environment: local | test | staging | production
started_at: <timestamp>
ended_at: <timestamp>

objective_id: obj_<name>

actor:
  type: synthetic_user | human_replay | agent | script
  persona: <persona>
  account_state: <state>

world:
  data_seed: <seed>
  api_profile: <profile>
  ai_profile: <profile>
  network_profile: <profile>

steps:
  - step: 1
    action: <action>
    target: <target>
    result: <result>
    latency_ms: <number>

ai_calls:
  - call_id: ai_<id>
    model: <model>
    prompt_hash: <hash>
    response_hash: <hash>
    latency_ms: <number>
    status: success | error | timeout | malformed

state_changes:
  - object: <object>
    change: <change>

artifacts:
  screenshots:
    - <file>
  logs:
    - <file-or-query>

outcome:
  task_completed: true | false
  time_to_value_seconds: <number>
  errors: <number>
  support_needed: true | false

judge_scores:
  hard_correctness: pass | warn | fail
  task_success: <0-1-or-pass-warn-fail>
  ux_quality: <0-1-or-pass-warn-fail>
  ai_quality: <0-1-or-pass-warn-fail>
  taste_alignment: <0-1-or-pass-warn-fail>
  overall: pass | warn | fail | review

review:
  needs_human_review: true | false
  reason: <reason>
```

## Judge

```yaml
judge_id: judge_<name>
name: <judge name>
type: deterministic | metric | rubric | llm | human | production_metric
objective_id: obj_<name>

input:
  - trace
  - screenshots
  - logs
  - ai_calls
  - final_state
  - examples

rubric:
  pass: <definition>
  warn: <definition>
  fail: <definition>

scoring:
  scale: 0_to_1 | pass_warn_fail | custom
  threshold_pass: <value>
  threshold_warn: <value>
  threshold_fail: <value>

calibration:
  golden_examples:
    - <example id>
  bad_examples:
    - <example id>
  human_labeled_examples:
    - <example id>

output:
  - score
  - classification
  - reasons
  - evidence
  - suggested_fix
```

## Release Gate

```yaml
release_gate_id: gate_<name>
objective_id: obj_<name>

block_if:
  - hard_correctness = fail
  - privacy_violation = true
  - data_loss = true
  - security_check = fail
  - payment_integrity = fail
  - critical_task_success < 0.90

warn_if:
  - ux_quality < 0.80
  - taste_alignment < 0.80
  - latency_p95_regression > 20_percent
  - ai_quality < 0.85

canary_if:
  - hard_correctness = pass
  - task_success >= 0.90
  - taste_alignment between 0.65 and 0.80
  - new_ai_behavior = true

human_review_if:
  - judge_disagreement = true
  - taste_alignment < 0.75
  - new_core_flow = true
  - uncertain_high_impact_behavior = true

ship_if:
  - all_required_checks = pass
  - no_blockers = true
  - unresolved_high_risk_unknowns = 0
```

## Minimal Folder Structure

```text
qa-loop/
  objectives/
  scenarios/
  world/
    data_profiles/
    api_profiles/
    ai_profiles/
  judges/
  traces/
    golden/
    bad/
    ambiguous/
    production/
    regression/
  memory/
  gates/
```
