use glyph_ei_bridge::{
    KILLER_REQUEST, SEMANTIC_CONTROL_CASE_COUNT, compile_meaning_aware_glyph, compile_naive_glyph,
    default_capsule_paths, judge_meaning_gate, load_capsules, run_codex_comparison_eval, run_glyph,
    run_improvement_loop, run_killer_eval, run_loop_comparison, run_semantic_control_suite,
    write_comparison_text_outputs, write_improvement_artifacts, write_loop_comparison_artifacts,
};
use pretty_assertions::assert_eq;

#[test]
fn killer_eval_proves_ei_capsules_force_clarification_gate() {
    let report = run_killer_eval().expect("killer eval runs");

    assert_eq!(report.gate.decision, "ship");
    assert!(report.meaning_aware.judgement.passed);
    assert!(!report.naive.judgement.passed);
}

#[test]
fn meaning_aware_glyph_asks_before_generating() {
    let capsules = load_capsules(&default_capsule_paths()).expect("load capsules");
    let compilation = compile_meaning_aware_glyph(KILLER_REQUEST, &capsules);
    let run = run_glyph(&compilation.glyph_source).expect("run meaning-aware Glyph");
    let judgement = judge_meaning_gate(&compilation, &run);
    let operations = run
        .trace
        .iter()
        .map(|event| event.operation.as_str())
        .collect::<Vec<_>>();

    assert!(judgement.passed, "{judgement:#?}");
    assert_eq!(
        operations,
        vec![
            "SPEC", "ASK", "PLAN", "GEN", "CHECK", "FIX", "CHECK", "REPAIR", "EXPORT"
        ]
    );
}

#[test]
fn naive_glyph_fails_the_same_meaning_gate() {
    let capsules = load_capsules(&default_capsule_paths()).expect("load capsules");
    let compilation = compile_naive_glyph(KILLER_REQUEST, &capsules);
    let run = run_glyph(&compilation.glyph_source).expect("run naive Glyph");
    let judgement = judge_meaning_gate(&compilation, &run);

    assert!(!judgement.passed);
    assert!(
        judgement
            .checks
            .iter()
            .any(|check| check.id == "ask_before_generation" && !check.passed)
    );
}

#[test]
fn codex_comparison_shows_ei_glyph_improves_control() {
    let report = run_codex_comparison_eval().expect("comparison eval runs");

    assert_eq!(report.gate.decision, "ship");
    assert!(!report.direct.judgement.passed);
    assert!(report.ei_glyph.judgement.passed);
    assert_eq!(
        report.ei_glyph.control_trace,
        vec![
            "SPEC", "ASK", "PLAN", "GEN", "CHECK", "FIX", "CHECK", "REPAIR", "EXPORT"
        ]
    );
}

#[test]
fn codex_comparison_can_export_side_by_side_text_files() {
    let report = run_codex_comparison_eval().expect("comparison eval runs");
    let output_dir = std::env::temp_dir().join(format!(
        "glyph-ei-bridge-side-by-side-{}",
        std::process::id()
    ));
    let text_outputs =
        write_comparison_text_outputs(&report, &output_dir).expect("write text outputs");

    let direct = std::fs::read_to_string(&text_outputs.direct_path).expect("read direct output");
    let prompt =
        std::fs::read_to_string(&text_outputs.ei_glyph_prompt_path).expect("read EI+Glyph prompt");
    let ei_glyph =
        std::fs::read_to_string(&text_outputs.ei_glyph_path).expect("read EI+Glyph output");

    assert!(direct.contains("Subject: Our sincere apologies"));
    assert!(prompt.contains("sarcasm_vs_sincerity"));
    assert!(prompt.contains("SPEC -> ASK -> PLAN -> GEN"));
    assert!(prompt.contains("Do not mention EI"));
    assert!(ei_glyph.contains("Subject: We're sorry for the poor experience"));
    assert!(ei_glyph.contains("We are truly sorry"));
    assert!(!ei_glyph.contains("Semantic gate"));
    assert!(!ei_glyph.contains("sarcasm_vs_sincerity"));
}

#[test]
fn improve_loop_exports_prompt_trace_output_and_verdict() {
    let report = run_improvement_loop(KILLER_REQUEST).expect("improvement loop runs");
    let output_dir =
        std::env::temp_dir().join(format!("glyph-ei-bridge-improve-{}", std::process::id()));
    let artifacts =
        write_improvement_artifacts(&report, &output_dir).expect("write improvement artifacts");

    let prompt = std::fs::read_to_string(&artifacts.writer_prompt_path).expect("read prompt");
    let capsules = std::fs::read_to_string(&artifacts.capsules_path).expect("read capsules");
    let trace = std::fs::read_to_string(&artifacts.glyph_trace_path).expect("read trace");
    let baseline = std::fs::read_to_string(&artifacts.baseline_output_path).expect("read baseline");
    let improved = std::fs::read_to_string(&artifacts.improved_output_path).expect("read improved");
    let verdict = std::fs::read_to_string(&artifacts.verdict_path).expect("read verdict");

    assert_eq!(report.gate.decision, "ship");
    assert!(!report.baseline.judgement.passed);
    assert!(report.improved.judgement.passed);
    assert!(prompt.contains("sarcasm_vs_sincerity"));
    assert!(prompt.contains("SPEC -> ASK -> PLAN -> GEN"));
    assert!(capsules.contains("\"form\": \"sarcasm\""));
    assert!(capsules.contains("\"form\": \"sincere\""));
    assert!(trace.contains("\"operation\": \"ASK\""));
    assert!(baseline.contains("obviously"));
    assert!(improved.contains("We are truly sorry"));
    assert!(!improved.contains("sarcasm_vs_sincerity"));
    assert!(verdict.contains("\"improvedPassed\": true"));
}

#[test]
fn loop_comparison_shows_ei_glyph_beats_codex_self_loop_on_control() {
    let report = run_loop_comparison(KILLER_REQUEST).expect("loop comparison runs");
    let output_dir = std::env::temp_dir().join(format!(
        "glyph-ei-bridge-loop-compare-{}",
        std::process::id()
    ));
    let artifacts =
        write_loop_comparison_artifacts(&report, &output_dir).expect("write loop artifacts");

    let side_by_side =
        std::fs::read_to_string(&artifacts.side_by_side_path).expect("read side by side");
    let self_output = std::fs::read_to_string(&artifacts.codex_self_loop_output_path)
        .expect("read self-loop output");
    let ei_prompt =
        std::fs::read_to_string(&artifacts.ei_glyph_prompt_path).expect("read EI+Glyph prompt");
    let verdict = std::fs::read_to_string(&artifacts.verdict_path).expect("read verdict");

    assert_eq!(report.gate.decision, "ship");
    assert!(!report.codex_self_loop.judgement.passed);
    assert!(report.ei_glyph_loop.judgement.passed);
    assert!(report.codex_self_loop.judgement.score < report.ei_glyph_loop.judgement.score);
    assert!(self_output.contains("We are truly sorry"));
    assert!(ei_prompt.contains("EI semantic conflict"));
    assert!(side_by_side.contains("machine-executable control trace"));
    assert!(verdict.contains("\"eiGlyphLoopPassed\": true"));
}

#[test]
fn semantic_control_suite_runs_ten_route_level_probes() {
    let report = run_semantic_control_suite().expect("semantic-control suite runs");

    assert_eq!(report.case_count, SEMANTIC_CONTROL_CASE_COUNT);
    assert_eq!(report.gate.decision, "ship");
    assert_eq!(report.ei_glyph_wins, SEMANTIC_CONTROL_CASE_COUNT);
    assert_eq!(report.codex_self_loop_wins, 0);
    assert_eq!(report.surface_ties, SEMANTIC_CONTROL_CASE_COUNT);
    assert!(
        report
            .cases
            .iter()
            .any(|case| case.scenario_id == "sc_responsibility_without_liability")
    );
    assert!(
        report
            .cases
            .iter()
            .any(|case| case.scenario_id == "sc_safe_but_unverified")
    );
}
