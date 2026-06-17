# Glyph EI Bridge

`glyph-ei-bridge` proves the integration thesis between:

- `../../`: compact executable control language for agentic harnesses
- `../etymonoetic-interlingua`: semantic capsules for layered lexical meaning

The bridge turns EI capsule evidence into Glyph context and gates. The first killer eval checks whether a meaning-aware controller catches a conflict in:

```text
Write a sarcastic apology to a customer that still sounds sincere.
```

EI says `sarcasm` often carries contempt or playful aggression, while `sincere` requires genuine intention. A meaning-aware Glyph controller should clarify before generating, not blindly draft a sarcastic apology.

Run it:

```bash
cargo test -p glyph-ei-bridge
cargo run -p glyph-ei-bridge -- eval --output out/killer-eval.json
cargo run -p glyph-ei-bridge -- compare --output out/codex-comparison.json
cargo run -p glyph-ei-bridge -- compare --output out/codex-comparison.json --text-output-dir out/side-by-side
cargo run -p glyph-ei-bridge -- improve --output-dir out/improve
cargo run -p glyph-ei-bridge -- loop-compare --output-dir out/loop-compare
cargo run -p glyph-ei-bridge -- semantic-suite --output out/semantic-control-suite.json
```

The eval passes only when:

- EI capsules validate.
- The bridge detects `sarcasm_vs_sincerity`.
- The emitted Glyph program validates and runs in GlyphVM.
- The trace contains `ASK` before `GEN`.
- The semantic conflict is present in the `SPEC` step.
- A naive non-EI Glyph program fails the same judge.

To compare against a real direct Codex answer, paste that answer into a text file and run:

```bash
cargo run -p glyph-ei-bridge -- compare --direct-output path/to/direct-codex-output.txt --output out/codex-comparison.json
```

`--text-output-dir` writes three human-readable artifacts:

- `codex-direct-output.txt`: the direct Codex-style answer.
- `codex-ei-glyph-prompt.txt`: the prompt built from the EI semantic conflict and Glyph trace.
- `codex-ei-glyph-output.txt`: the cleaned customer-facing answer produced from that trace-informed prompt.

`improve` runs the reusable 1-8 loop:

1. Load relevant EI capsules.
2. Detect semantic tensions.
3. Compile a Glyph control program.
4. Run the Glyph trace.
5. Build a trace-informed writer prompt.
6. Prepare the improved output.
7. Judge baseline vs improved output.
8. Export the request, capsules, Glyph source, trace, prompt, outputs, report, and verdict.

Run it with the built-in killer request:

```bash
cargo run -p glyph-ei-bridge -- improve --output-dir out/improve
```

Or pass your own request:

```bash
cargo run -p glyph-ei-bridge -- improve --request "Write a sarcastic apology to a customer that still sounds sincere." --output-dir out/improve
cargo run -p glyph-ei-bridge -- improve --input path/to/request.txt --output-dir out/improve
```

`loop-compare` tests the more serious novelty question: can a Codex-style self-loop refine itself toward the goal as well as EI + Glyph?

```bash
cargo run -p glyph-ei-bridge -- loop-compare --output-dir out/loop-compare
```

This writes:

- `codex-self-loop-prompt.txt`
- `codex-self-loop-trace.json`
- `codex-self-loop-output.txt`
- `ei-glyph-prompt.txt`
- `ei-glyph-trace.json`
- `ei-glyph-output.txt`
- `side-by-side.md`
- `report.json`
- `verdict.json`

The route-level judge gives the self-loop credit if it improves the final prose. EI + Glyph only wins if it adds something stronger: external EI semantic evidence, an `ASK` before `GEN` gate, and a machine-executable Glyph control trace.

`semantic-suite` runs 10 route-level probes across hidden semantic tensions:

- sarcasm vs sincerity
- responsibility vs liability
- guarantee vs estimate
- urgent vs alarmist
- therapeutic vs diagnostic
- persuasive vs manipulative
- friendly vs firm
- concise vs complete
- certain vs uncertain
- safe vs unverified

The expected result is not that EI + Glyph always writes much better final prose. The expected result is that it creates a stronger, auditable control route before generation.
