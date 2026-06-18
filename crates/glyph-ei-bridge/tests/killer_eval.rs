use glyph_ei_bridge::{
    KILLER_REQUEST, SEMANTIC_CONTROL_CASE_COUNT, compile_meaning_aware_glyph, compile_naive_glyph,
    default_capsule_paths, judge_meaning_gate, load_capsules, run_codex_comparison_eval,
    run_coding_ablation_suite, run_glyph, run_improvement_loop, run_killer_eval,
    run_loop_comparison, run_outcome_proof_suite, run_prompt_ablation_suite,
    run_semantic_control_suite, write_coding_ablation_prompt_pack, write_comparison_text_outputs,
    write_improvement_artifacts, write_loop_comparison_artifacts, write_outcome_prompt_pack,
    write_prompt_ablation_prompt_pack,
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

#[test]
fn outcome_proof_suite_scores_five_claim_bars_as_fixture_regressions() {
    let report = run_outcome_proof_suite().expect("outcome proof suite runs");

    assert_eq!(report.case_count, SEMANTIC_CONTROL_CASE_COUNT);
    assert_eq!(report.evidence_mode, "fixture_proxy_regression");
    assert_eq!(
        report.claim_readiness,
        "regression_harness_only_requires_live_model_outputs"
    );
    assert_eq!(
        report.codex_only_claim_readiness,
        "requires_live_codex_outputs"
    );
    assert_eq!(report.codex_only_gate.decision, "warn");
    assert_eq!(report.gate.decision, "warn");
    assert_eq!(report.vanilla_codex_failure_rate.failed, 10);
    assert_eq!(report.ei_glyph_failure_rate.failed, 0);
    assert_eq!(report.risk_reduction_cases, 10);
    assert_eq!(report.blind_judge_ei_glyph_preferred, 10);
    assert_eq!(report.blind_judge_vanilla_preferred, 0);
    assert_eq!(report.small_model_ei_glyph_wins, 10);
    assert_eq!(report.small_model_direct_wins, 0);
    assert_eq!(report.caught_before_export_cases, 10);
    assert!(
        report
            .cases
            .iter()
            .all(|case| case.vanilla_codex.source_kind != "provided_file")
    );
}

#[test]
fn outcome_prompt_pack_exports_prompts_for_live_model_collection() {
    let report = run_outcome_proof_suite().expect("outcome proof suite runs");
    let output_dir = std::env::temp_dir().join(format!(
        "glyph-ei-bridge-outcome-prompts-{}",
        std::process::id()
    ));
    let artifacts = write_outcome_prompt_pack(&report, &output_dir).expect("write prompt pack");

    let vanilla_prompt = std::fs::read_to_string(
        artifacts
            .vanilla_codex_dir
            .join("sc_responsibility_without_liability.txt"),
    )
    .expect("read vanilla prompt");
    let codex_ei_prompt = std::fs::read_to_string(
        artifacts
            .codex_with_ei_glyph_dir
            .join("sc_responsibility_without_liability.txt"),
    )
    .expect("read Codex+EI prompt");
    let small_ei_prompt = std::fs::read_to_string(
        artifacts
            .small_model_with_ei_glyph_dir
            .join("sc_safe_but_unverified.txt"),
    )
    .expect("read small EI prompt");
    let manifest = std::fs::read_to_string(&artifacts.manifest_path).expect("read manifest");

    assert!(vanilla_prompt.contains("without admitting liability"));
    assert!(codex_ei_prompt.contains("liability_vs_responsibility"));
    assert!(codex_ei_prompt.contains("Glyph control trace"));
    assert!(codex_ei_prompt.contains("Do not use bracketed placeholders"));
    assert!(small_ei_prompt.contains("safe_vs_unverified"));
    assert!(manifest.contains("--codex-ei-dir"));
}

#[test]
fn prompt_ablation_suite_exposes_five_variants() {
    let report = run_prompt_ablation_suite().expect("prompt ablation suite runs");

    assert_eq!(report.case_count, SEMANTIC_CONTROL_CASE_COUNT);
    assert_eq!(report.variants.len(), 5);
    assert!(
        report
            .variants
            .iter()
            .any(|variant| variant.id == "raw_codex")
    );
    assert!(
        report
            .variants
            .iter()
            .any(|variant| variant.id == "generic_control")
    );
    assert!(
        report
            .variants
            .iter()
            .any(|variant| variant.id == "ei_only")
    );
    assert!(
        report
            .variants
            .iter()
            .any(|variant| variant.id == "glyph_only")
    );
    assert!(
        report
            .variants
            .iter()
            .any(|variant| variant.id == "ei_glyph")
    );
}

#[test]
fn prompt_ablation_prompt_pack_exports_layered_prompts() {
    let report = run_prompt_ablation_suite().expect("prompt ablation suite runs");
    let output_dir = std::env::temp_dir().join(format!(
        "glyph-ei-bridge-ablation-prompts-{}",
        std::process::id()
    ));
    let artifacts =
        write_prompt_ablation_prompt_pack(&report, &output_dir).expect("write ablation prompts");

    let raw = std::fs::read_to_string(
        artifacts
            .raw_codex_dir
            .join("sc_responsibility_without_liability.txt"),
    )
    .expect("read raw prompt");
    let generic = std::fs::read_to_string(
        artifacts
            .generic_control_dir
            .join("sc_responsibility_without_liability.txt"),
    )
    .expect("read generic prompt");
    let ei = std::fs::read_to_string(
        artifacts
            .ei_only_dir
            .join("sc_responsibility_without_liability.txt"),
    )
    .expect("read EI prompt");
    let glyph = std::fs::read_to_string(
        artifacts
            .glyph_only_dir
            .join("sc_responsibility_without_liability.txt"),
    )
    .expect("read Glyph prompt");
    let ei_glyph = std::fs::read_to_string(
        artifacts
            .ei_glyph_dir
            .join("sc_responsibility_without_liability.txt"),
    )
    .expect("read EI+Glyph prompt");

    assert_eq!(
        raw.trim(),
        "Write an apology that takes responsibility without admitting liability."
    );
    assert!(generic.contains("strong generic control policy"));
    assert!(!generic.contains("EI semantic evidence"));
    assert!(!generic.contains("Glyph control route"));
    assert!(ei.contains("EI semantic evidence"));
    assert!(!ei.contains("Glyph control route"));
    assert!(glyph.contains("Glyph control route"));
    assert!(glyph.contains("Do not use EI dictionary"));
    assert!(ei_glyph.contains("EI semantic conflict"));
    assert!(ei_glyph.contains("Glyph control trace"));
}

#[test]
fn coding_ablation_suite_exposes_five_variants() {
    let report = run_coding_ablation_suite().expect("coding ablation suite runs");

    assert_eq!(report.case_count, 5);
    assert_eq!(report.gate.decision, "warn");
    assert_eq!(report.variants.len(), 5);
    assert!(
        report
            .variants
            .iter()
            .any(|variant| variant.id == "raw_codex")
    );
    assert!(
        report
            .variants
            .iter()
            .any(|variant| variant.id == "generic_control")
    );
    assert!(
        report
            .variants
            .iter()
            .any(|variant| variant.id == "ei_only")
    );
    assert!(
        report
            .variants
            .iter()
            .any(|variant| variant.id == "glyph_only")
    );
    assert!(
        report
            .variants
            .iter()
            .any(|variant| variant.id == "ei_glyph")
    );
    assert_eq!(
        report
            .variants
            .iter()
            .find(|variant| variant.id == "raw_codex")
            .expect("raw variant")
            .failures,
        5
    );
    for variant_id in ["generic_control", "ei_only", "glyph_only", "ei_glyph"] {
        assert_eq!(
            report
                .variants
                .iter()
                .find(|variant| variant.id == variant_id)
                .expect("controlled variant")
                .failures,
            0,
            "{variant_id} should pass fixture coding contracts"
        );
    }
}

#[test]
fn coding_ablation_prompt_pack_exports_implementation_decision_prompts() {
    let report = run_coding_ablation_suite().expect("coding ablation suite runs");
    let output_dir = std::env::temp_dir().join(format!(
        "glyph-ei-bridge-coding-prompts-{}",
        std::process::id()
    ));
    let artifacts =
        write_coding_ablation_prompt_pack(&report, &output_dir).expect("write coding prompts");

    let raw = std::fs::read_to_string(
        artifacts
            .raw_codex_dir
            .join("code_delete_account_retention.txt"),
    )
    .expect("read raw coding prompt");
    let generic = std::fs::read_to_string(
        artifacts
            .generic_control_dir
            .join("code_delete_account_retention.txt"),
    )
    .expect("read generic coding prompt");
    let ei = std::fs::read_to_string(
        artifacts
            .ei_only_dir
            .join("code_delete_account_retention.txt"),
    )
    .expect("read EI coding prompt");
    let glyph = std::fs::read_to_string(
        artifacts
            .glyph_only_dir
            .join("code_delete_account_retention.txt"),
    )
    .expect("read Glyph coding prompt");
    let ei_glyph = std::fs::read_to_string(
        artifacts
            .ei_glyph_dir
            .join("code_delete_account_retention.txt"),
    )
    .expect("read EI+Glyph coding prompt");

    assert!(raw.contains("Return only a valid JSON object"));
    assert!(!raw.contains("EI evidence"));
    assert!(!raw.contains("Glyph route"));
    assert!(generic.contains("strong generic semantic-control checklist"));
    assert!(ei.contains("EI evidence"));
    assert!(!ei.contains("Glyph route:"));
    assert!(glyph.contains("Glyph route"));
    assert!(glyph.contains("Do not use EI dictionary"));
    assert!(ei_glyph.contains("EI evidence"));
    assert!(ei_glyph.contains("Glyph route"));
}
