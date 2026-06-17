use glyph::eval::compression::compare_compression;
use glyph::eval::controller::{
    ControllerAdapterMode, ControllerEvalCaseFilter, ControllerEvalOptions, ControllerEvalReport,
    ControllerGrammarPayload, ControllerParameterClass, ControllerPromptMode,
    ControllerRequestKind, GENERIC_TOOL_PLAN_JSON_SCHEMA, build_controller_prompt,
    build_controller_prompt_with_payload, build_direct_prose_prompt, build_json_tool_plan_prompt,
    build_openai_compatible_request_body, run_controller_eval, run_controller_eval_with_observer,
    run_controller_eval_with_options, summarize_controller_eval_by_model,
};
use glyph::eval::controller_examples::controller_eval_cases;
use glyph::eval::coverage::controller_eval_coverage;
use glyph::eval::curriculum::{
    ControllerCurriculumOptions, ControllerCurriculumQualityStatus, ControllerCurriculumRecordKind,
    ControllerCurriculumRejectionStage, assess_controller_curriculum_quality,
    export_controller_curriculum,
};
use glyph::eval::dataset::{
    ControllerDatasetOptions, ControllerDatasetSplit, export_controller_dataset,
};
use glyph::eval::dataset_quality::{
    ControllerDatasetQualityStatus, assess_controller_dataset_quality,
};
use glyph::eval::evidence::{
    ControllerClaimAuditInput, ControllerClaimAuditStatus, audit_controller_claim,
};
use glyph::eval::examples::CompressionExample;
use glyph::eval::fingerprint::controller_eval_fingerprint;
use glyph::eval::gate::{ControllerGateCheckStatus, evaluate_controller_gate};
use glyph::eval::manifest::{
    ControllerEvalMergedManifestInput, ControllerEvalRunArtifacts, ControllerEvalRunCaseFilter,
    ControllerEvalRunConfig, ControllerEvalRunModel, ControllerEvalSourceManifest,
    build_controller_eval_run_manifest, build_merged_controller_eval_manifest,
};
use glyph::eval::preflight::{
    ControllerPreflightCheckStatus, ControllerPreflightModel, ControllerPreflightOptions,
    preflight_controller_eval,
};
use glyph::eval::results::merge_controller_eval_cases;
use glyph::eval::verify::{ControllerRunVerificationStatus, verify_controller_run};
use glyph::harness::mock_tools::create_mock_tool_registry;
use glyph::ir::glyph_ir::{GlyphIrStep, parse_glyph_to_ir};
use glyph::ir::validate_ir::validate_ir;
use glyph::language::grammar::{GLYPH_CONTROLLER_OUTPUT_JSON_SCHEMA, GLYPH_EBNF, GLYPH_GBNF};
use glyph::language::parser::parse_glyph;
use glyph::runtime::glyph_vm::GlyphVm;
use glyph::runtime::trace::TraceEvent;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_dir(name: &str) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time is after Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("glyph-{name}-{}-{suffix}", std::process::id()))
}

#[test]
fn parses_simple_flow() {
    let ast = parse_glyph(
        r#"
        goal "Say hello"
        flow main {
          SPEC(message="hello") -> spec
          EXPORT(spec)
        }
        "#,
    )
    .unwrap();

    assert_eq!(ast.goal.as_deref(), Some("Say hello"));
    assert_eq!(ast.flows[0].name, "main");
    assert_eq!(ast.flows[0].steps.len(), 2);
}

#[test]
fn parses_ctx_arrays_and_objects() {
    let ir = validate_ir(
        parse_glyph_to_ir(
            r#"
            ctx {
              stack: "nextjs"
              flags: { auth: true, max: 3 }
            }

            flow main {
              GEN(stack=ctx.stack, entities=["project", "task"], flags=ctx.flags) -> files
            }
            "#,
        )
        .unwrap(),
    )
    .unwrap();

    assert_eq!(ir.context["stack"], json!("nextjs"));
    assert_eq!(ir.context["flags"]["auth"], json!(true));
    assert_eq!(ir.flows[0].steps.len(), 1);
}

#[test]
fn converts_ast_to_ir_and_validates() {
    let ir = validate_ir(
        parse_glyph_to_ir(
            r#"
            flow main {
              SPEC(app="tracker") -> spec
              PLAN(spec) -> plan
            }
            "#,
        )
        .unwrap(),
    )
    .unwrap();
    let value = serde_json::to_value(ir).unwrap();

    assert_eq!(value["version"], json!("0.1"));
    assert_eq!(value["flows"][0]["steps"][1]["op"], json!("PLAN"));
    assert_eq!(
        value["flows"][0]["steps"][1]["args"]["input"],
        json!({ "var": "spec" })
    );
}

#[test]
fn rejects_invalid_ir() {
    let mut ir = parse_glyph_to_ir("flow main { SPEC() -> spec }").unwrap();
    ir.version = "9.9".to_string();

    assert!(validate_ir(ir).is_err());
}

#[test]
fn semantic_validation_rejects_bad_model_programs() {
    let unknown_var = parse_glyph_to_ir("flow main { PLAN(missing) -> plan }").unwrap();
    assert!(
        validate_ir(unknown_var)
            .unwrap_err()
            .to_string()
            .contains("Unknown variable")
    );

    let unknown_tool = parse_glyph_to_ir("flow main { NOPE() -> nope }").unwrap();
    assert!(
        validate_ir(unknown_tool)
            .unwrap_err()
            .to_string()
            .contains("Unknown tool")
    );

    let unknown_ctx = parse_glyph_to_ir(
        r#"
        ctx { stack: "nextjs" }
        flow main { GEN(stack=ctx.missing) -> files }
        "#,
    )
    .unwrap();
    assert!(
        validate_ir(unknown_ctx)
            .unwrap_err()
            .to_string()
            .contains("Unknown ctx reference")
    );

    let bad_repair = parse_glyph_to_ir(
        r#"
        flow main {
          repair files with report max 3 {
            FIX(files, report) -> files
          }
        }
        "#,
    )
    .unwrap();
    assert!(
        validate_ir(bad_repair)
            .unwrap_err()
            .to_string()
            .contains("Unknown repair target variable")
    );

    let mut empty_flow = parse_glyph_to_ir("flow main { SPEC() -> spec }").unwrap();
    empty_flow.flows[0].steps.clear();
    assert!(
        validate_ir(empty_flow)
            .unwrap_err()
            .to_string()
            .contains("must contain at least one step")
    );

    let mut duplicate_step = parse_glyph_to_ir(
        r#"
        flow main {
          SPEC(message="hello") -> spec
          SUM(spec) -> summary
        }
        "#,
    )
    .unwrap();
    let duplicate_id = match &duplicate_step.flows[0].steps[0] {
        GlyphIrStep::Tool(tool) => tool.id.clone(),
        GlyphIrStep::Repair(_) => unreachable!(),
    };
    if let GlyphIrStep::Tool(tool) = &mut duplicate_step.flows[0].steps[1] {
        tool.id = duplicate_id;
    }
    assert!(
        validate_ir(duplicate_step)
            .unwrap_err()
            .to_string()
            .contains("Duplicate step id")
    );

    let mut malformed_var = parse_glyph_to_ir("flow main { SPEC() -> spec }").unwrap();
    if let GlyphIrStep::Tool(tool) = &mut malformed_var.flows[0].steps[0] {
        tool.args.insert("input".to_string(), json!({ "var": 7 }));
    }
    assert!(
        validate_ir(malformed_var)
            .unwrap_err()
            .to_string()
            .contains("Invalid variable reference")
    );

    let zero_repair = parse_glyph_to_ir(
        r#"
        flow main {
          READ(path="app") -> files
          CHECK(files) -> report
          repair files with report max 0 {
            FIX(files, report) -> files
            CHECK(files) -> report
          }
        }
        "#,
    )
    .unwrap();
    assert!(
        validate_ir(zero_repair)
            .unwrap_err()
            .to_string()
            .contains("Repair maxIterations")
    );

    let missing_report_update = parse_glyph_to_ir(
        r#"
        flow main {
          READ(path="app") -> files
          CHECK(files) -> report
          repair files with report max 3 {
            FIX(files, report) -> files
          }
        }
        "#,
    )
    .unwrap();
    assert!(
        validate_ir(missing_report_update)
            .unwrap_err()
            .to_string()
            .contains("must assign report variable")
    );
}

#[test]
fn executes_simple_flow_and_trace() {
    let vm = GlyphVm::new(create_mock_tool_registry());
    let result = vm
        .run_source(
            r#"
            flow main {
              SPEC(message="hello") -> spec
              SUM(spec) -> summary
              EXPORT(summary)
            }
            "#,
        )
        .unwrap();

    assert_eq!(result.outputs.len(), 1);
    assert_eq!(
        result
            .trace
            .iter()
            .map(|event| event.operation.as_str())
            .collect::<Vec<_>>(),
        vec!["SPEC", "SUM", "EXPORT"]
    );
}

#[test]
fn rejects_unknown_variables_and_tools() {
    let vm = GlyphVm::new(create_mock_tool_registry());

    let missing_variable = vm
        .run_source("flow main { PLAN(missing) -> plan }")
        .unwrap_err();
    assert!(
        missing_variable
            .to_string()
            .contains("Unknown variable \"missing\"")
    );

    let unknown_tool = vm.run_source("flow main { NOPE() -> result }").unwrap_err();
    assert!(unknown_tool.to_string().contains("Unknown tool \"NOPE\""));
}

#[test]
fn executes_repair_block_with_max_iterations() {
    let vm = GlyphVm::new(create_mock_tool_registry());
    let source = fs::read_to_string("src/examples/repair_failing_tests.glyph").unwrap();
    let result = vm.run_source(&source).unwrap();

    assert_eq!(result.variables["report"]["status"], json!("pass"));
    assert_eq!(
        result
            .trace
            .iter()
            .filter(|event| event.operation == "FIX")
            .count(),
        1
    );
    assert!(
        result
            .trace
            .iter()
            .any(|event| event.operation == "REPAIR" && event.status.as_str() == "pass")
    );
}

#[test]
fn calculates_compression_stats() {
    let stats = compare_compression(
        "flow main { EXPORT(result) }",
        &CompressionExample {
            name: "sample",
            file: "sample.glyph",
            natural_language: "Export the already prepared result object as the final artifact using the local harness output mechanism so it can be inspected by the caller.",
        },
    );

    assert!(stats.glyph_chars > 0);
    assert!(stats.natural_language_approx_tokens > stats.glyph_approx_tokens);
    assert!(stats.compression_ratio > 1.0);
}

#[test]
fn controller_eval_reports_fixture_model_buckets() {
    let report = run_controller_eval();

    assert_eq!(report.actual_model_calls, 0);
    assert_eq!(report.by_model.len(), 4);
    assert_eq!(controller_eval_cases().len(), 72);
    assert_eq!(report.cases.len(), 72 * 4);
    assert!(report.by_model.iter().all(|summary| {
        summary.valid_program_rate == 1.0
            && summary.run_success_rate == 1.0
            && summary.successful_trace_rate == 1.0
            && summary.glyph_over_direct_plan_rate == 1.0
            && summary.json_tool_plan_run_success_rate == 1.0
            && summary.json_tool_plan_successful_trace_rate == 1.0
            && summary.glyph_over_json_tool_plan_rate == 0.0
            && summary.direct_prose_successful_trace_rate == 0.0
            && summary.glyph_over_direct_prose_rate == 1.0
            && summary.repair_success_rate == Some(1.0)
    }));
    assert!(
        report
            .cases
            .iter()
            .all(|case| case.json_tool_plan_successful_trace)
    );
    assert!(report.cases.iter().all(|case| {
        case.direct_prose_attempted
            && !case.direct_prose_parse_ok
            && !case.direct_prose_successful_trace
            && case.direct_prose_parse_error.is_some()
    }));
}

#[test]
fn controller_eval_can_compare_prompt_modes() {
    let report = run_controller_eval_with_options(ControllerEvalOptions {
        models: None,
        prompt_modes: ControllerPromptMode::all(),
        ..ControllerEvalOptions::default()
    });

    assert_eq!(report.actual_model_calls, 0);
    assert_eq!(report.by_model.len(), 4 * 3);
    assert_eq!(report.cases.len(), 72 * 4 * 3);
    assert!(report.by_model.iter().all(|summary| summary.cases == 72));
    assert!(report.cases.iter().any(|case| {
        case.tags.iter().any(|tag| tag == "profile:adversarial") && case.successful_trace
    }));
    assert!(report.by_model.iter().all(|summary| {
        summary.json_tool_plan_run_success_rate == 1.0
            && summary.json_tool_plan_successful_trace_rate == 1.0
            && summary.direct_prose_successful_trace_rate == 0.0
            && summary.glyph_over_direct_prose_rate == 1.0
    }));
    assert!(report.cases.iter().any(|case| {
        case.prompt_mode == ControllerPromptMode::Constrained && case.successful_trace
    }));
    assert!(report.cases.iter().any(|case| {
        case.prompt_mode == ControllerPromptMode::SchemaOnly && case.successful_trace
    }));
    assert!(
        report
            .cases
            .iter()
            .any(|case| case.prompt_mode == ControllerPromptMode::Plain && case.successful_trace)
    );
}

#[test]
fn controller_eval_can_filter_cases_for_canary_runs() {
    let report = run_controller_eval_with_options(ControllerEvalOptions {
        models: None,
        prompt_modes: vec![ControllerPromptMode::Constrained],
        case_filter: ControllerEvalCaseFilter {
            families: vec!["hello_summary".to_string()],
            profiles: vec!["adversarial".to_string()],
            limit: Some(1),
            ..ControllerEvalCaseFilter::default()
        },
    });

    assert_eq!(report.cases.len(), 4);
    assert!(report.by_model.iter().all(|summary| summary.cases == 1));
    assert!(report.cases.iter().all(|case| {
        case.case_id.starts_with("hello_summary_")
            && case.tags.iter().any(|tag| tag == "profile:adversarial")
    }));
}

#[test]
fn controller_eval_observer_receives_each_row() {
    let mut observed = Vec::new();
    let report = run_controller_eval_with_observer(
        ControllerEvalOptions {
            models: None,
            prompt_modes: vec![ControllerPromptMode::Constrained],
            case_filter: ControllerEvalCaseFilter {
                families: vec!["hello_summary".to_string()],
                profiles: vec!["normal".to_string()],
                limit: Some(1),
                ..ControllerEvalCaseFilter::default()
            },
        },
        |case| {
            observed.push((
                case.model_id.clone(),
                case.prompt_mode,
                case.case_id.clone(),
            ));
            Ok::<(), std::convert::Infallible>(())
        },
    )
    .unwrap();

    assert_eq!(observed.len(), report.cases.len());
    assert_eq!(observed.len(), 4);
    assert!(observed.iter().all(|(_, prompt_mode, case_id)| {
        *prompt_mode == ControllerPromptMode::Constrained && case_id.starts_with("hello_summary_")
    }));
}

#[test]
fn controller_eval_manifest_records_provenance_without_secret_values() {
    let secret_value = "super-secret-api-key";
    let case_filter = ControllerEvalCaseFilter {
        families: vec!["hello_summary".to_string()],
        profiles: vec!["normal".to_string()],
        limit: Some(1),
        ..ControllerEvalCaseFilter::default()
    };
    let report = run_controller_eval_with_options(ControllerEvalOptions {
        models: None,
        prompt_modes: vec![ControllerPromptMode::Constrained],
        case_filter: case_filter.clone(),
    });
    let config = ControllerEvalRunConfig {
        adapter_mode: ControllerAdapterMode::OpenAiCompatible,
        endpoint: Some("http://localhost:11434/v1".to_string()),
        api_key_env: Some("GLYPH_EVAL_API_KEY".to_string()),
        api_key_provided: true,
        models: report
            .by_model
            .iter()
            .map(|summary| ControllerEvalRunModel {
                parameter_class: summary.parameter_class,
                model_id: summary.model_id.clone(),
            })
            .collect(),
        prompt_modes: vec![ControllerPromptMode::Constrained],
        grammar_payload: ControllerGrammarPayload::Gbnf,
        case_filter: ControllerEvalRunCaseFilter::from(&case_filter),
        selected_case_ids: vec!["hello_summary_normal_short".to_string()],
        selected_case_count: 1,
        artifacts: ControllerEvalRunArtifacts {
            jsonl_path: Some("out/results.jsonl".to_string()),
            manifest_path: Some("out/results.manifest.json".to_string()),
            emit_prompts_path: None,
            stream_jsonl: true,
        },
    };

    let manifest = build_controller_eval_run_manifest(
        10,
        Some(20),
        "0.1.0",
        Some("abcdef".to_string()),
        Some(false),
        config,
        Some(&report),
    );
    let value = serde_json::to_value(&manifest).unwrap();
    let serialized = serde_json::to_string(&manifest).unwrap();

    assert_eq!(value["manifestVersion"], json!("0.1"));
    assert_eq!(value["manifestKind"], json!("run"));
    assert_eq!(value["runStatus"], json!("completed"));
    assert_eq!(value["startedAtUnixSeconds"], json!(10));
    assert_eq!(value["completedAtUnixSeconds"], json!(20));
    assert_eq!(value["config"]["apiKeyEnv"], json!("GLYPH_EVAL_API_KEY"));
    assert_eq!(value["config"]["apiKeyProvided"], json!(true));
    assert_eq!(value["security"]["apiKeyValueOmitted"], json!(true));
    assert_eq!(value["security"]["realShellRunEnabled"], json!(false));
    assert_eq!(
        value["fingerprint"]["overallSha256"],
        json!(controller_eval_fingerprint().overall_sha256)
    );
    assert_eq!(value["reportSummary"]["caseRows"], json!(4));
    assert_eq!(value["coverage"]["caseRows"], json!(4));
    assert!(!serialized.contains(secret_value));
}

#[test]
fn controller_eval_fingerprint_covers_specs_and_corpus() {
    let fingerprint = controller_eval_fingerprint();

    assert_eq!(fingerprint.algorithm, "sha256");
    assert_eq!(fingerprint.overall_sha256.len(), 64);
    assert!(
        fingerprint
            .overall_sha256
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    );
    assert_eq!(fingerprint.eval_corpus.case_count, 72);
    assert!(
        fingerprint
            .eval_corpus
            .families
            .contains(&"hello_summary".to_string())
    );
    assert!(
        fingerprint
            .eval_corpus
            .profiles
            .contains(&"adversarial".to_string())
    );
    assert!(fingerprint.spec_artifacts.iter().any(|artifact| {
        artifact.name == "glyph.gbnf" && artifact.bytes > 0 && artifact.sha256.len() == 64
    }));
    assert!(fingerprint.spec_artifacts.iter().any(|artifact| {
        artifact.name == "controller-output.schema.json"
            && artifact.bytes > 0
            && artifact.sha256.len() == 64
    }));
    assert_eq!(
        fingerprint.request_contract.model_id,
        "glyph-fingerprint-model"
    );
    assert_eq!(fingerprint.request_contract.request_count, 72 * 3 * 2 * 3);
    assert_eq!(
        fingerprint.request_contract.prompt_modes,
        vec!["constrained", "schema-only", "plain"]
    );
    assert_eq!(
        fingerprint.request_contract.grammar_payloads,
        vec!["none", "gbnf"]
    );
    assert_eq!(
        fingerprint.request_contract.request_kinds,
        vec!["glyph", "json-tool-plan", "direct-prose"]
    );
    assert_eq!(fingerprint.request_contract.sha256.len(), 64);
}

#[test]
fn controller_run_verification_checks_manifest_against_jsonl_rows() {
    let case_filter = ControllerEvalCaseFilter {
        families: vec!["hello_summary".to_string()],
        profiles: vec!["normal".to_string()],
        limit: Some(1),
        ..ControllerEvalCaseFilter::default()
    };
    let report = run_controller_eval_with_options(ControllerEvalOptions {
        models: None,
        prompt_modes: vec![ControllerPromptMode::Constrained],
        case_filter: case_filter.clone(),
    });
    let config = ControllerEvalRunConfig {
        adapter_mode: ControllerAdapterMode::Fixture,
        endpoint: None,
        api_key_env: None,
        api_key_provided: false,
        models: report
            .by_model
            .iter()
            .map(|summary| ControllerEvalRunModel {
                parameter_class: summary.parameter_class,
                model_id: summary.model_id.clone(),
            })
            .collect(),
        prompt_modes: vec![ControllerPromptMode::Constrained],
        grammar_payload: ControllerGrammarPayload::None,
        case_filter: ControllerEvalRunCaseFilter::from(&case_filter),
        selected_case_ids: vec!["hello_summary_normal_short".to_string()],
        selected_case_count: 1,
        artifacts: ControllerEvalRunArtifacts {
            jsonl_path: Some("out/results.jsonl".to_string()),
            manifest_path: Some("out/results.manifest.json".to_string()),
            emit_prompts_path: None,
            stream_jsonl: true,
        },
    };
    let manifest = build_controller_eval_run_manifest(
        10,
        Some(20),
        "0.1.0",
        Some("abcdef".to_string()),
        Some(false),
        config,
        Some(&report),
    );
    let manifest_value = serde_json::to_value(&manifest).unwrap();

    let verification = verify_controller_run(&report.cases, &manifest_value, "out/results.jsonl");

    assert!(verification.passed);
    assert!(
        verification
            .checks
            .iter()
            .all(|check| { check.status == ControllerRunVerificationStatus::Pass })
    );

    let mut tampered = manifest_value;
    tampered["fingerprint"]["overallSha256"] = json!("bad");
    let verification = verify_controller_run(&report.cases, &tampered, "out/results.jsonl");

    assert!(!verification.passed);
    assert!(verification.checks.iter().any(|check| {
        check.id == "fingerprint_current" && check.status == ControllerRunVerificationStatus::Fail
    }));
}

#[test]
fn controller_run_verification_accepts_merged_manifest_with_verified_sources() {
    let case_filter = ControllerEvalCaseFilter {
        families: vec!["hello_summary".to_string()],
        profiles: vec!["normal".to_string()],
        limit: Some(1),
        ..ControllerEvalCaseFilter::default()
    };
    let report = run_controller_eval_with_options(ControllerEvalOptions {
        models: None,
        prompt_modes: vec![ControllerPromptMode::Constrained],
        case_filter,
    });
    let source = ControllerEvalSourceManifest {
        manifest_path: "out/source.manifest.json".to_string(),
        jsonl_path: "out/source.jsonl".to_string(),
        fingerprint_sha256: controller_eval_fingerprint().overall_sha256,
        case_rows: report.cases.len(),
        verified: true,
    };
    let manifest = build_merged_controller_eval_manifest(
        ControllerEvalMergedManifestInput {
            started_at_unix_seconds: 10,
            completed_at_unix_seconds: 20,
            glyph_version: "0.1.0".to_string(),
            git_commit: Some("abcdef".to_string()),
            git_tree_dirty: Some(false),
            jsonl_path: "out/merged.jsonl".to_string(),
            manifest_path: "out/merged.manifest.json".to_string(),
            source_manifests: vec![source],
        },
        &report.cases,
    );
    let manifest_value = serde_json::to_value(&manifest).unwrap();

    let verification = verify_controller_run(&report.cases, &manifest_value, "out/merged.jsonl");

    assert!(verification.passed);
    assert_eq!(manifest_value["manifestKind"], json!("merged"));
    assert_eq!(
        manifest_value["sourceManifests"][0]["verified"],
        json!(true)
    );

    let mut missing_source = manifest_value;
    missing_source["sourceManifests"] = json!([]);
    let verification = verify_controller_run(&report.cases, &missing_source, "out/merged.jsonl");

    assert!(!verification.passed);
    assert!(verification.checks.iter().any(|check| {
        check.id == "source_manifests_verified"
            && check.status == ControllerRunVerificationStatus::Fail
    }));
}

#[test]
fn controller_eval_merge_dedupes_staged_results() {
    let report = run_controller_eval_with_options(ControllerEvalOptions {
        models: None,
        prompt_modes: vec![ControllerPromptMode::Constrained],
        case_filter: ControllerEvalCaseFilter {
            families: vec!["hello_summary".to_string()],
            profiles: vec!["normal".to_string()],
            limit: Some(1),
            ..ControllerEvalCaseFilter::default()
        },
    });
    let mut replacement = report.cases[0].clone();
    replacement.successful_trace = false;
    replacement.run_ok = false;
    replacement.run_error = Some("rerun failure".to_string());

    let merged = merge_controller_eval_cases(vec![report.cases.clone(), vec![replacement]]);

    assert_eq!(merged.report.input_rows, 5);
    assert_eq!(merged.report.output_rows, 4);
    assert_eq!(merged.report.replaced_rows, 1);
    assert!(
        merged
            .cases
            .iter()
            .any(|case| case.run_error.as_deref() == Some("rerun failure"))
    );
}

#[test]
fn controller_coverage_reports_missing_live_target_rows() {
    let report = run_controller_eval_with_options(ControllerEvalOptions {
        models: None,
        prompt_modes: ControllerPromptMode::all(),
        ..ControllerEvalOptions::default()
    });
    let coverage = controller_eval_coverage(&report.cases);

    assert!(!coverage.coverage_complete);
    assert_eq!(coverage.required_target_rows, 72);
    assert_eq!(coverage.target_rows, 0);
    assert_eq!(coverage.missing_target_rows, 72);
    assert_eq!(coverage.missing_buckets, vec!["1b", "3b", "7b", "frontier"]);
    assert_eq!(
        coverage.missing_prompt_modes,
        vec!["constrained", "schema-only", "plain"]
    );
}

#[test]
fn controller_coverage_tracks_partial_live_target_rows() {
    let report = run_controller_eval_with_options(ControllerEvalOptions {
        models: None,
        prompt_modes: vec![ControllerPromptMode::Constrained],
        case_filter: ControllerEvalCaseFilter {
            families: vec!["hello_summary".to_string()],
            profiles: vec!["normal".to_string()],
            limit: Some(1),
            ..ControllerEvalCaseFilter::default()
        },
    });
    let mut cases = report.cases;

    for case in &mut cases {
        case.adapter_mode = ControllerAdapterMode::OpenAiCompatible;
        case.grammar_payload = ControllerGrammarPayload::Gbnf;
    }

    let coverage = controller_eval_coverage(&cases);

    assert!(!coverage.coverage_complete);
    assert_eq!(coverage.live_case_rows, 4);
    assert_eq!(coverage.target_rows, 1);
    assert_eq!(coverage.missing_target_rows, 71);
    let hello = coverage
        .family_profiles
        .iter()
        .find(|family| family.family == "hello_summary")
        .expect("hello_summary family coverage exists");
    assert_eq!(hello.observed_target_rows, 1);
    assert!(hello.observed_profiles.contains(&"normal".to_string()));
    assert!(hello.missing_profiles.contains(&"adversarial".to_string()));
}

#[test]
fn controller_dataset_exports_deterministic_training_record() {
    let export = export_controller_dataset(ControllerDatasetOptions {
        case_filter: ControllerEvalCaseFilter {
            families: vec!["hello_summary".to_string()],
            profiles: vec!["normal".to_string()],
            limit: Some(1),
            ..ControllerEvalCaseFilter::default()
        },
        validation_stride: None,
    })
    .expect("dataset export succeeds");

    assert_eq!(export.record_count, 1);
    assert_eq!(export.train_records, 1);
    assert_eq!(export.validation_records, 0);

    let record = &export.records[0];
    assert_eq!(record.split, ControllerDatasetSplit::Train);
    assert_eq!(record.case_id, "hello_summary_normal_short");
    assert!(record.training_example.user.contains(&record.request));
    assert!(record.training_example.assistant.contains("flow main"));
    assert_eq!(record.target_ir.flows[0].name, "main");
    assert!(!record.target_trace.is_empty());
    assert!(
        record
            .target_trace
            .iter()
            .all(|event| event.duration_ms == 0)
    );
    assert_eq!(record.final_outputs.len(), 1);
    assert_eq!(record.metadata.trace_event_count, record.target_trace.len());
    assert_eq!(
        record.metadata.final_output_count,
        record.final_outputs.len()
    );
}

#[test]
fn controller_dataset_uses_stable_validation_stride() {
    let export = export_controller_dataset(ControllerDatasetOptions::default())
        .expect("dataset export succeeds");

    assert_eq!(export.record_count, 72);
    assert_eq!(export.validation_records, 9);
    assert_eq!(export.train_records, 63);
    assert!(
        export
            .records
            .iter()
            .any(|record| record.split == ControllerDatasetSplit::Validation)
    );
}

#[test]
fn controller_dataset_quality_passes_for_full_corpus() {
    let export = export_controller_dataset(ControllerDatasetOptions::default())
        .expect("dataset export succeeds");
    let quality = assess_controller_dataset_quality(&export);

    assert!(quality.passed);
    assert_eq!(quality.metrics.record_count, 72);
    assert_eq!(quality.metrics.family_count, 9);
    assert_eq!(quality.metrics.profile_count, 4);
    assert_eq!(quality.metrics.repair_records, 8);
    assert_eq!(quality.metrics.trace_complete_records, 72);
    assert_eq!(quality.metrics.final_output_records, 72);
    assert!(quality.metrics.average_target_approx_tokens <= 140.0);
    assert!(quality.metrics.max_target_approx_tokens <= 260);
    assert!(
        quality
            .checks
            .iter()
            .all(|check| check.status == ControllerDatasetQualityStatus::Pass)
    );
}

#[test]
fn controller_dataset_quality_rejects_tiny_shards() {
    let export = export_controller_dataset(ControllerDatasetOptions {
        case_filter: ControllerEvalCaseFilter {
            families: vec!["hello_summary".to_string()],
            profiles: vec!["normal".to_string()],
            limit: Some(1),
            ..ControllerEvalCaseFilter::default()
        },
        validation_stride: None,
    })
    .expect("dataset export succeeds");
    let quality = assess_controller_dataset_quality(&export);

    assert!(!quality.passed);
    for expected in [
        "record_count",
        "split_present",
        "family_coverage",
        "profile_coverage",
        "repair_examples",
    ] {
        assert!(quality.checks.iter().any(|check| {
            check.id == expected && check.status == ControllerDatasetQualityStatus::Fail
        }));
    }
}

#[test]
fn controller_curriculum_exports_positive_repair_and_rejection_records() {
    let export = export_controller_curriculum(ControllerCurriculumOptions {
        dataset_options: ControllerDatasetOptions {
            case_filter: ControllerEvalCaseFilter {
                families: vec!["hello_summary".to_string()],
                profiles: vec!["normal".to_string()],
                limit: Some(1),
                ..ControllerEvalCaseFilter::default()
            },
            validation_stride: None,
        },
    })
    .expect("curriculum export succeeds");

    assert_eq!(export.case_count, 1);
    assert_eq!(export.record_count, 7);
    assert_eq!(export.positive_records, 1);
    assert_eq!(export.repair_records, 3);
    assert_eq!(export.rejected_negative_records, 3);

    let positive = export
        .records
        .iter()
        .find(|record| record.kind == ControllerCurriculumRecordKind::Positive)
        .expect("positive record exists");
    assert_eq!(positive.training_example.assistant, positive.target_glyph);
    assert!(positive.rejected_output.is_none());

    assert!(export.records.iter().any(|record| {
        record.kind == ControllerCurriculumRecordKind::Repair
            && record.training_example.assistant == record.target_glyph
            && record.rejected_output.is_some()
    }));
    assert!(export.records.iter().any(|record| {
        record.kind == ControllerCurriculumRecordKind::RejectedNegative
            && record.training_example.assistant.starts_with("REJECT:")
    }));
    assert!(export.records.iter().any(|record| {
        record
            .rejection
            .as_ref()
            .is_some_and(|rejection| rejection.stage == ControllerCurriculumRejectionStage::Parse)
    }));
    assert!(export.records.iter().any(|record| {
        record.rejection.as_ref().is_some_and(|rejection| {
            rejection.stage == ControllerCurriculumRejectionStage::SemanticValidation
        })
    }));
}

#[test]
fn controller_curriculum_quality_passes_for_full_corpus() {
    let export = export_controller_curriculum(ControllerCurriculumOptions::default())
        .expect("curriculum export succeeds");
    let quality = assess_controller_curriculum_quality(&export);

    assert!(quality.passed);
    assert_eq!(quality.metrics.case_count, 72);
    assert_eq!(quality.metrics.record_count, 504);
    assert_eq!(quality.metrics.positive_records, 72);
    assert_eq!(quality.metrics.repair_records, 216);
    assert_eq!(quality.metrics.rejected_negative_records, 216);
    assert!(quality.metrics.parse_rejection_records >= 72);
    assert!(quality.metrics.semantic_rejection_records >= 72);
    assert!(
        quality
            .checks
            .iter()
            .all(|check| check.status == ControllerCurriculumQualityStatus::Pass)
    );
}

#[test]
fn controller_preflight_accepts_complete_live_plan() {
    let report = preflight_controller_eval(ControllerPreflightOptions {
        adapter_mode: ControllerAdapterMode::OpenAiCompatible,
        prompt_modes: ControllerPromptMode::all(),
        grammar_payload: ControllerGrammarPayload::Gbnf,
        case_filter: ControllerEvalCaseFilter {
            families: vec!["hello_summary".to_string()],
            profiles: vec!["normal".to_string()],
            limit: Some(1),
            ..ControllerEvalCaseFilter::default()
        },
        models: complete_preflight_models(),
        jsonl_path: Some("out/results.jsonl".to_string()),
        manifest_path: Some("out/results.manifest.json".to_string()),
        stream_jsonl: true,
    });

    assert!(report.passed);
    assert_eq!(report.selected_case_count, 1);
    assert_eq!(report.expected_rows, 12);
    assert_eq!(report.expected_model_calls, 36);
    assert!(
        report
            .checks
            .iter()
            .all(|check| check.status == ControllerPreflightCheckStatus::Pass)
    );
}

#[test]
fn controller_preflight_rejects_incomplete_live_plan() {
    let report = preflight_controller_eval(ControllerPreflightOptions {
        adapter_mode: ControllerAdapterMode::OpenAiCompatible,
        prompt_modes: vec![ControllerPromptMode::Constrained],
        grammar_payload: ControllerGrammarPayload::None,
        case_filter: ControllerEvalCaseFilter {
            families: vec!["hello_summary".to_string()],
            profiles: vec!["normal".to_string()],
            limit: Some(1),
            ..ControllerEvalCaseFilter::default()
        },
        models: vec![
            ControllerPreflightModel {
                parameter_class: ControllerParameterClass::OneB,
                model_id: Some("tiny".to_string()),
            },
            ControllerPreflightModel {
                parameter_class: ControllerParameterClass::ThreeB,
                model_id: None,
            },
            ControllerPreflightModel {
                parameter_class: ControllerParameterClass::SevenB,
                model_id: None,
            },
            ControllerPreflightModel {
                parameter_class: ControllerParameterClass::Frontier,
                model_id: None,
            },
        ],
        jsonl_path: None,
        manifest_path: None,
        stream_jsonl: false,
    });

    assert!(!report.passed);
    for expected in [
        "model_ids_present",
        "constrained_uses_gbnf",
        "live_jsonl_artifact",
        "live_stream_jsonl",
        "live_manifest_artifact",
    ] {
        assert!(report.checks.iter().any(|check| {
            check.id == expected && check.status == ControllerPreflightCheckStatus::Fail
        }));
    }
}

#[test]
fn controller_claim_audit_reports_missing_live_evidence() {
    let audit = audit_controller_claim(ControllerClaimAuditInput {
        cases: None,
        manifest: None,
        jsonl_path: None,
    });

    assert!(!audit.passed);
    assert!(!audit.claim_ready);
    assert!(
        audit
            .checks
            .iter()
            .any(|check| check.id == "spec_fingerprint"
                && check.status == ControllerClaimAuditStatus::Pass)
    );
    assert!(
        audit
            .checks
            .iter()
            .any(|check| check.id == "controller_dataset"
                && check.status == ControllerClaimAuditStatus::Pass)
    );
    assert!(
        audit
            .dataset_quality
            .as_ref()
            .is_some_and(|quality| quality.passed)
    );
    assert!(
        audit
            .checks
            .iter()
            .any(|check| check.id == "controller_curriculum"
                && check.status == ControllerClaimAuditStatus::Pass)
    );
    assert!(
        audit
            .curriculum_quality
            .as_ref()
            .is_some_and(|quality| quality.passed)
    );
    assert!(
        audit
            .checks
            .iter()
            .any(|check| check.id == "live_jsonl_supplied"
                && check.status == ControllerClaimAuditStatus::Fail)
    );
    assert!(
        audit.checks.iter().any(|check| check.id == "benchmark_gate"
            && check.status == ControllerClaimAuditStatus::Fail)
    );
}

#[test]
fn cli_exports_static_controller_evidence_pack() {
    let output_dir = unique_temp_dir("evidence-pack");
    let output = Command::new(env!("CARGO_BIN_EXE_glyph"))
        .arg("export-controller-evidence-pack")
        .arg("--output")
        .arg(&output_dir)
        .output()
        .expect("run evidence pack export");

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    for file in [
        "fingerprint.json",
        "dataset-quality.json",
        "curriculum-quality.json",
        "request-preview.json",
        "claim-audit.json",
        "summary.json",
        "README.md",
    ] {
        assert!(
            output_dir.join(file).is_file(),
            "expected evidence pack file {file}"
        );
    }

    let summary: Value = serde_json::from_str(
        &fs::read_to_string(output_dir.join("summary.json")).expect("read summary"),
    )
    .expect("parse summary");
    assert_eq!(summary["claimReady"], json!(false));
    assert_eq!(summary["liveEvidenceSupplied"], json!(false));
    assert_eq!(summary["datasetQualityPassed"], json!(true));
    assert_eq!(summary["curriculumQualityPassed"], json!(true));

    let stdout_summary: Value =
        serde_json::from_slice(&output.stdout).expect("parse stdout summary");
    assert_eq!(
        stdout_summary["output"],
        json!(output_dir.display().to_string())
    );

    let rejected = Command::new(env!("CARGO_BIN_EXE_glyph"))
        .arg("export-controller-evidence-pack")
        .arg("--output")
        .arg(&output_dir)
        .arg("--jsonl")
        .arg("out/missing.jsonl")
        .output()
        .expect("run invalid evidence pack export");
    assert!(!rejected.status.success());
    assert!(
        String::from_utf8_lossy(&rejected.stderr)
            .contains("--jsonl and --manifest must be supplied together")
    );

    let _ = fs::remove_dir_all(output_dir);
}

#[test]
fn controller_claim_audit_can_pass_synthetic_live_evidence() {
    let report = synthetic_claim_ready_report();
    let jsonl_path = "out/live-controller-eval.jsonl";
    let manifest = build_controller_eval_run_manifest(
        10,
        Some(20),
        "0.1.0-test",
        Some("abc123".to_string()),
        Some(false),
        ControllerEvalRunConfig {
            adapter_mode: ControllerAdapterMode::OpenAiCompatible,
            endpoint: Some("http://localhost:11434/v1".to_string()),
            api_key_env: Some("GLYPH_EVAL_API_KEY".to_string()),
            api_key_provided: false,
            models: vec![
                ControllerEvalRunModel {
                    parameter_class: ControllerParameterClass::OneB,
                    model_id: "fixture-1b-constrained".to_string(),
                },
                ControllerEvalRunModel {
                    parameter_class: ControllerParameterClass::ThreeB,
                    model_id: "fixture-3b-constrained".to_string(),
                },
                ControllerEvalRunModel {
                    parameter_class: ControllerParameterClass::SevenB,
                    model_id: "fixture-7b-constrained".to_string(),
                },
                ControllerEvalRunModel {
                    parameter_class: ControllerParameterClass::Frontier,
                    model_id: "fixture-frontier-constrained".to_string(),
                },
            ],
            prompt_modes: ControllerPromptMode::all(),
            grammar_payload: ControllerGrammarPayload::Gbnf,
            case_filter: ControllerEvalRunCaseFilter {
                case_ids: vec![],
                tags: vec![],
                families: vec![],
                profiles: vec![],
                limit: None,
            },
            selected_case_ids: controller_eval_cases()
                .into_iter()
                .map(|case| case.id)
                .collect(),
            selected_case_count: 72,
            artifacts: ControllerEvalRunArtifacts {
                jsonl_path: Some(jsonl_path.to_string()),
                manifest_path: Some("out/live-controller-eval.manifest.json".to_string()),
                emit_prompts_path: None,
                stream_jsonl: true,
            },
        },
        Some(&report),
    );
    let manifest = serde_json::to_value(manifest).unwrap();
    let audit = audit_controller_claim(ControllerClaimAuditInput {
        cases: Some(&report.cases),
        manifest: Some(&manifest),
        jsonl_path: Some(jsonl_path),
    });

    assert!(audit.passed);
    assert!(audit.claim_ready);
    assert!(
        audit
            .dataset_quality
            .as_ref()
            .is_some_and(|quality| quality.passed)
    );
    assert!(
        audit
            .verification
            .as_ref()
            .is_some_and(|report| report.passed)
    );
    assert!(
        audit
            .coverage
            .as_ref()
            .is_some_and(|report| report.coverage_complete)
    );
    assert!(audit.gate.as_ref().is_some_and(|report| report.passed));
}

#[test]
fn controller_gate_rejects_fixture_only_results() {
    let report = run_controller_eval_with_options(ControllerEvalOptions {
        models: None,
        prompt_modes: ControllerPromptMode::all(),
        ..ControllerEvalOptions::default()
    });
    let gate = evaluate_controller_gate(&report.cases);

    assert!(!gate.passed);
    assert_eq!(gate.live_case_rows, 0);
    assert!(
        gate.checks
            .iter()
            .any(|check| check.id == "live_results"
                && check.status == ControllerGateCheckStatus::Fail)
    );
}

#[test]
fn controller_gate_can_pass_synthetic_live_results() {
    let report = run_controller_eval_with_options(ControllerEvalOptions {
        models: None,
        prompt_modes: ControllerPromptMode::all(),
        ..ControllerEvalOptions::default()
    });
    let mut cases = report.cases;

    for case in &mut cases {
        case.adapter_mode = ControllerAdapterMode::OpenAiCompatible;
        if case.parameter_class == ControllerParameterClass::OneB
            && case.prompt_mode == ControllerPromptMode::Constrained
        {
            case.grammar_payload = ControllerGrammarPayload::Gbnf;
            case.json_tool_plan_run_ok = false;
            case.json_tool_plan_successful_trace = false;
            case.json_tool_plan_run_error = Some("synthetic weaker JSON baseline".to_string());
        }
    }

    let gate = evaluate_controller_gate(&cases);

    assert!(gate.passed);
    assert_eq!(gate.target_case_rows, 72);
    assert!(
        gate.checks
            .iter()
            .all(|check| check.status == ControllerGateCheckStatus::Pass)
    );
    assert_eq!(gate.metrics.larger_plain_successful_trace_rate, Some(1.0));
    assert_eq!(gate.metrics.target_direct_prose_successful_trace_rate, 0.0);
    assert!(
        gate.checks
            .iter()
            .any(|check| check.id == "larger_plain_baseline"
                && check.status == ControllerGateCheckStatus::Pass)
    );
    assert!(
        gate.checks
            .iter()
            .any(|check| check.id == "direct_prose_baseline"
                && check.status == ControllerGateCheckStatus::Pass)
    );
}

#[test]
fn controller_gate_rejects_missing_direct_prose_baseline() {
    let report = run_controller_eval_with_options(ControllerEvalOptions {
        models: None,
        prompt_modes: ControllerPromptMode::all(),
        ..ControllerEvalOptions::default()
    });
    let mut cases = report.cases;

    for case in &mut cases {
        case.adapter_mode = ControllerAdapterMode::OpenAiCompatible;
        if case.parameter_class == ControllerParameterClass::OneB
            && case.prompt_mode == ControllerPromptMode::Constrained
        {
            case.grammar_payload = ControllerGrammarPayload::Gbnf;
            case.json_tool_plan_run_ok = false;
            case.json_tool_plan_successful_trace = false;
            case.json_tool_plan_run_error = Some("synthetic weaker JSON baseline".to_string());
            case.direct_prose_attempted = false;
        }
    }

    let gate = evaluate_controller_gate(&cases);

    assert!(!gate.passed);
    assert!(
        gate.checks
            .iter()
            .any(|check| check.id == "direct_prose_baseline"
                && check.status == ControllerGateCheckStatus::Fail)
    );
}

#[test]
fn controller_gate_rejects_when_larger_plain_models_outperform_target() {
    let report = run_controller_eval_with_options(ControllerEvalOptions {
        models: None,
        prompt_modes: ControllerPromptMode::all(),
        ..ControllerEvalOptions::default()
    });
    let mut cases = report.cases;
    let mut degraded_target = false;

    for case in &mut cases {
        case.adapter_mode = ControllerAdapterMode::OpenAiCompatible;
        if case.parameter_class == ControllerParameterClass::OneB
            && case.prompt_mode == ControllerPromptMode::Constrained
        {
            case.grammar_payload = ControllerGrammarPayload::Gbnf;
            case.json_tool_plan_run_ok = false;
            case.json_tool_plan_successful_trace = false;
            case.json_tool_plan_run_error = Some("synthetic weaker JSON baseline".to_string());

            if !degraded_target {
                case.successful_trace = false;
                degraded_target = true;
            }
        }
    }

    let gate = evaluate_controller_gate(&cases);

    assert!(!gate.passed);
    assert!(gate.metrics.target_successful_trace_rate >= 0.85);
    assert_eq!(gate.metrics.larger_plain_successful_trace_rate, Some(1.0));
    assert!(
        gate.checks
            .iter()
            .any(|check| check.id == "larger_plain_baseline"
                && check.status == ControllerGateCheckStatus::Fail)
    );
}

fn complete_preflight_models() -> Vec<ControllerPreflightModel> {
    vec![
        ControllerPreflightModel {
            parameter_class: ControllerParameterClass::OneB,
            model_id: Some("tiny".to_string()),
        },
        ControllerPreflightModel {
            parameter_class: ControllerParameterClass::ThreeB,
            model_id: Some("small".to_string()),
        },
        ControllerPreflightModel {
            parameter_class: ControllerParameterClass::SevenB,
            model_id: Some("medium".to_string()),
        },
        ControllerPreflightModel {
            parameter_class: ControllerParameterClass::Frontier,
            model_id: Some("frontier".to_string()),
        },
    ]
}

fn synthetic_claim_ready_report() -> ControllerEvalReport {
    let mut report = run_controller_eval_with_options(ControllerEvalOptions {
        models: None,
        prompt_modes: ControllerPromptMode::all(),
        ..ControllerEvalOptions::default()
    });

    report.mode = ControllerAdapterMode::OpenAiCompatible;
    report.actual_model_calls = report.cases.len() * 3;

    for case in &mut report.cases {
        case.adapter_mode = ControllerAdapterMode::OpenAiCompatible;
        if case.parameter_class == ControllerParameterClass::OneB
            && case.prompt_mode == ControllerPromptMode::Constrained
        {
            case.grammar_payload = ControllerGrammarPayload::Gbnf;
            case.json_tool_plan_run_ok = false;
            case.json_tool_plan_successful_trace = false;
            case.json_tool_plan_run_error = Some("synthetic weaker JSON baseline".to_string());
        }
    }

    report.by_model = summarize_controller_eval_by_model(&report.cases);
    report
}

#[test]
fn controller_prompt_modes_expose_different_constraints() {
    let eval_case = controller_eval_cases()
        .into_iter()
        .next()
        .expect("controller eval corpus is nonempty");
    let constrained = build_controller_prompt(&eval_case, ControllerPromptMode::Constrained);
    let schema_only = build_controller_prompt(&eval_case, ControllerPromptMode::SchemaOnly);
    let plain = build_controller_prompt(&eval_case, ControllerPromptMode::Plain);
    let gbnf_payload = build_controller_prompt_with_payload(
        &eval_case,
        ControllerPromptMode::Constrained,
        ControllerGrammarPayload::Gbnf,
    );
    let json_plan_constrained =
        build_json_tool_plan_prompt(&eval_case, ControllerPromptMode::Constrained);
    let json_plan_plain = build_json_tool_plan_prompt(&eval_case, ControllerPromptMode::Plain);
    let direct_prose = build_direct_prose_prompt(&eval_case);

    assert!(constrained.contains("Glyph grammar:"));
    assert!(constrained.contains("Output JSON schema:"));
    assert!(!schema_only.contains("Glyph grammar:"));
    assert!(schema_only.contains("Output JSON schema:"));
    assert!(!plain.contains("Glyph grammar:"));
    assert!(!plain.contains("Output JSON schema:"));
    assert!(gbnf_payload.contains("Decoder constraint:"));
    assert!(!gbnf_payload.contains("Output JSON schema:"));
    assert!(gbnf_payload.contains("Return only Glyph source"));
    assert!(json_plan_constrained.contains("Generic Tool Plan Baseline"));
    assert!(!json_plan_plain.contains("Generic Tool Plan Baseline"));
    assert!(direct_prose.contains("natural-language plan"));
    assert!(direct_prose.contains("Do not use Glyph"));
    assert!(!direct_prose.contains("Output JSON schema:"));
}

#[test]
fn openai_request_bodies_expose_expected_constraints() {
    let eval_case = controller_eval_cases()
        .into_iter()
        .next()
        .expect("controller eval corpus is nonempty");
    let glyph_gbnf = build_openai_compatible_request_body(
        "tiny-model",
        &eval_case,
        ControllerPromptMode::Constrained,
        ControllerGrammarPayload::Gbnf,
        ControllerRequestKind::Glyph,
    );
    let glyph_schema = build_openai_compatible_request_body(
        "tiny-model",
        &eval_case,
        ControllerPromptMode::SchemaOnly,
        ControllerGrammarPayload::None,
        ControllerRequestKind::Glyph,
    );
    let json_plan = build_openai_compatible_request_body(
        "tiny-model",
        &eval_case,
        ControllerPromptMode::Constrained,
        ControllerGrammarPayload::Gbnf,
        ControllerRequestKind::JsonToolPlan,
    );
    let direct_prose = build_openai_compatible_request_body(
        "tiny-model",
        &eval_case,
        ControllerPromptMode::Constrained,
        ControllerGrammarPayload::Gbnf,
        ControllerRequestKind::DirectProse,
    );

    assert_eq!(glyph_gbnf["model"], json!("tiny-model"));
    assert_eq!(glyph_gbnf["grammar"], json!(GLYPH_GBNF));
    assert!(glyph_gbnf.get("response_format").is_none());
    assert!(
        glyph_gbnf["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("decoder is constrained")
    );
    assert!(
        glyph_gbnf["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("Decoder constraint:")
    );

    assert_eq!(
        glyph_schema["response_format"],
        json!({ "type": "json_object" })
    );
    assert!(glyph_schema.get("grammar").is_none());
    assert!(
        glyph_schema["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("Output JSON schema:")
    );

    assert_eq!(
        json_plan["response_format"],
        json!({ "type": "json_object" })
    );
    assert!(json_plan.get("grammar").is_none());
    assert!(
        json_plan["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("generic JSON tool plan")
    );

    assert!(direct_prose.get("response_format").is_none());
    assert!(direct_prose.get("grammar").is_none());
    assert!(
        direct_prose["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("direct natural-language planning baseline")
    );
    assert!(
        direct_prose["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("Do not use Glyph")
    );
}

#[test]
fn spec_artifacts_match_reference_constants() {
    assert_eq!(fs::read_to_string("spec/glyph.ebnf").unwrap(), GLYPH_EBNF);
    assert_eq!(fs::read_to_string("spec/glyph.gbnf").unwrap(), GLYPH_GBNF);
    assert_eq!(
        fs::read_to_string("spec/controller-output.schema.json").unwrap(),
        GLYPH_CONTROLLER_OUTPUT_JSON_SCHEMA
    );
    assert_eq!(
        fs::read_to_string("spec/generic-tool-plan.schema.json").unwrap(),
        GENERIC_TOOL_PLAN_JSON_SCHEMA
    );
}

#[test]
fn golden_fixtures_compile_to_expected_ir_and_trace() {
    let vm = GlyphVm::new(create_mock_tool_registry());
    let fixture_names = fixture_base_names();

    for fixture_name in fixture_names {
        let source = fs::read_to_string(format!("spec/fixtures/{fixture_name}.glyph")).unwrap();
        let expected_ir: Value = serde_json::from_str(
            &fs::read_to_string(format!("spec/fixtures/{fixture_name}.ir.json")).unwrap(),
        )
        .unwrap();
        let expected_trace: Value = serde_json::from_str(
            &fs::read_to_string(format!("spec/fixtures/{fixture_name}.trace.json")).unwrap(),
        )
        .unwrap();

        let ir = validate_ir(parse_glyph_to_ir(&source).unwrap()).unwrap();
        assert_eq!(serde_json::to_value(&ir).unwrap(), expected_ir);

        let run = vm.run_source(&source).unwrap();
        let normalized_trace = Value::Array(
            run.trace
                .iter()
                .map(normalize_trace_event)
                .collect::<Vec<_>>(),
        );
        assert_eq!(normalized_trace, expected_trace);
    }
}

fn fixture_base_names() -> Vec<String> {
    let mut names = fs::read_dir("spec/fixtures")
        .unwrap()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            entry
                .path()
                .extension()
                .and_then(|extension| (extension == "glyph").then_some(()))
                .and_then(|_| {
                    entry
                        .path()
                        .file_stem()
                        .map(|stem| stem.to_string_lossy().to_string())
                })
        })
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn normalize_trace_event(event: &TraceEvent) -> Value {
    let mut value = json!({
        "stepId": event.step_id,
        "operation": event.operation,
        "status": event.status.as_str(),
        "outputSummary": event.output_summary
    });

    if let Some(iteration) = event.iteration
        && let Some(object) = value.as_object_mut()
    {
        object.insert("iteration".to_string(), json!(iteration));
    }

    value
}
