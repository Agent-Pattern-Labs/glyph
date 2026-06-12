# Unified Loop Theory

Unified Loop Theory is a reusable Codex skill for turning any vague goal into an operating loop.

One loop to control all loops.

The thesis:

```text
Every useful improvement loop has the same shape.
```

The loop:

```text
Objective -> World -> Probe -> Trace -> Judge -> Repair -> Memory -> Gate -> Objective
```

In plain English:

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

This repo is meant to be the master loop for other loops: QA loops, eval loops, coding-agent loops, product-improvement loops, AI-app loops, release loops, debugging loops, and taste/quality loops.

The shareable Codex skill lives at:

```text
skills/unified-loop-theory/
```

## Quick Start

Clone this repo, copy its path, open the repo you want to improve, and ask your AI agent:

```text
Use $unified-loop-theory from <path-to-unified-loop-theory> as the master loop for this repo.

Inspect the repo, identify the highest-leverage objective, define the world/probes/traces/judges/memory/gate, run the first loop, judge the trace, repair what failed, store the lesson, and continue.
```

The output should not be a static plan. It should be a living loop that keeps improving the repo and repairs the loop itself when the objective, probe, trace, judge, memory, or gate is weak.

## How To Use With Another Repo

1. Clone this repo.

```bash
git clone git@github.com:Agent-Pattern-Labs/unified-loop-theory.git
```

2. Copy the path to this repo.

From inside this repo:

```bash
pwd | pbcopy
```

If `pbcopy` is not available, run `pwd` and copy the printed path manually.

3. Open the repo, app, product, or project you want to improve.

For example:

```bash
cd /path/to/your-target-repo
```

4. Point your AI agent at the copied Unified Loop Theory repo path.

Use a prompt like this:

```text
Use $unified-loop-theory from <path-to-unified-loop-theory> as the master loop for this repo.

1. Inspect my repo.
2. Identify the highest-leverage objective.
3. Define the world, probes, traces, judges, memory, and gate.
4. Turn that into an explicit goal.
5. Run the first loop.
6. Judge the trace.
7. Repair what failed.
8. Store the lesson.
9. Repeat until the objective is satisfied or the loop needs a better objective.
```

Concrete shape:

```text
Use $unified-loop-theory from /path/to/unified-loop-theory as the master loop for this repo.

Start by inspecting the current repo, then create and run the first loop.
```

The target repo can be anything: an app, package, AI workflow, agent project, website, internal tool, documentation system, or product prototype.

5. Keep the AI inside the loop.

Do not stop at a plan. Ask the agent to execute the first probe, capture evidence, judge the result, make the next repair, and continue.

The expected output is not just an explanation. The expected output is a living loop:

```text
current objective
current world model
current probes
latest trace
latest judgment
latest repair
memory updates
next gate decision
next loop
```

## What Makes It Recursive

The loop improves the system, but it can also improve itself.

If the objective is vague, repair the objective.
If the probe is weak, repair the probe.
If the trace is incomplete, repair instrumentation.
If the judge is wrong, repair the judge.
If the world model missed reality, repair the world model.
If memory is not compounding, repair memory.
If the gate is too loose or too strict, repair the gate.

That is the unified part: every failure becomes evidence about either the system or the loop controlling the system.
