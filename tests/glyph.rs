use glyph::eval::benchmark_report::{
    ControllerBenchmarkComparisonStatus, controller_benchmark_report,
};
use glyph::eval::compression::{approximate_tokens, compare_compression};
use glyph::eval::conformance::glyph_conformance_report;
use glyph::eval::controller::{
    ControllerAdapterMode, ControllerEvalCaseFilter, ControllerEvalCaseResult,
    ControllerEvalOptions, ControllerEvalReport, ControllerGrammarPayload,
    ControllerParameterClass, ControllerPromptMode, ControllerRequestKind,
    GENERIC_TOOL_PLAN_JSON_SCHEMA, build_controller_prompt, build_controller_prompt_with_payload,
    build_direct_prose_prompt, build_json_tool_plan_prompt, build_openai_compatible_request_body,
    run_controller_eval, run_controller_eval_with_observer, run_controller_eval_with_options,
    summarize_controller_eval_by_model,
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
use glyph::eval::live_plan::{ControllerLivePlanOptions, plan_controller_live_run};
use glyph::eval::manifest::{
    ControllerEvalMergedManifestInput, ControllerEvalRunArtifacts, ControllerEvalRunCaseFilter,
    ControllerEvalRunConfig, ControllerEvalRunModel, ControllerEvalSourceManifest,
    build_controller_eval_run_manifest, build_merged_controller_eval_manifest,
};
use glyph::eval::offline_plan::{
    CONTROLLER_OFFLINE_PLAN_VERSION, ControllerOfflinePlanOptions, plan_controller_offline_run,
};
use glyph::eval::preflight::{
    ControllerPreflightCheckStatus, ControllerPreflightModel, ControllerPreflightOptions,
    preflight_controller_eval,
};
use glyph::eval::results::merge_controller_eval_cases;
use glyph::eval::robustness::{ControllerRobustnessStatus, evaluate_controller_robustness};
use glyph::eval::status::{
    ControllerClaimStatusInput, ControllerClaimStatusPhase, controller_claim_status,
    controller_claim_status_from_audit,
};
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
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_dir(name: &str) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time is after Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("glyph-{name}-{}-{suffix}", std::process::id()))
}

fn write_controller_eval_jsonl_for_test(path: PathBuf, cases: &[ControllerEvalCaseResult]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create controller eval jsonl parent");
    }
    let mut file = fs::File::create(&path).expect("create controller eval jsonl");
    for case in cases {
        writeln!(
            file,
            "{}",
            serde_json::to_string(case).expect("serialize controller eval row")
        )
        .expect("write controller eval row");
    }
}

fn parameter_class_for_test_bucket(bucket: &str) -> ControllerParameterClass {
    match bucket {
        "1b" => ControllerParameterClass::OneB,
        "3b" => ControllerParameterClass::ThreeB,
        "7b" => ControllerParameterClass::SevenB,
        "frontier" => ControllerParameterClass::Frontier,
        other => panic!("unexpected test bucket {other}"),
    }
}

fn spawn_openai_compatible_mock_server(expected_requests: usize) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock OpenAI-compatible server");
    let address = listener.local_addr().expect("mock server local address");
    thread::spawn(move || {
        for index in 0..expected_requests {
            let (mut stream, _) = listener.accept().expect("accept mock request");
            let request = read_http_request(&mut stream);
            assert!(
                request.contains("POST /v1/chat/completions"),
                "unexpected request: {request}"
            );
            let body = json!({
                "choices": [
                    {
                        "message": {
                            "content": format!("mock local decoder response {}", index + 1)
                        }
                    }
                ]
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write mock response");
        }
    });
    format!("http://{address}/v1")
}

fn read_http_request(stream: &mut std::net::TcpStream) -> String {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    let mut header_end = None;
    let mut content_length = 0_usize;

    loop {
        let bytes = stream.read(&mut chunk).expect("read mock request");
        if bytes == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..bytes]);
        if header_end.is_none()
            && let Some(position) = buffer.windows(4).position(|window| window == b"\r\n\r\n")
        {
            header_end = Some(position + 4);
            let headers = String::from_utf8_lossy(&buffer[..position]);
            content_length = headers
                .lines()
                .find_map(|line| {
                    line.strip_prefix("content-length:")
                        .or_else(|| line.strip_prefix("Content-Length:"))
                })
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
        }
        if let Some(header_end) = header_end
            && buffer.len() >= header_end + content_length
        {
            break;
        }
    }

    String::from_utf8_lossy(&buffer).to_string()
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
            prompt_bundle_overall_sha256: None,
            prompt_bundle_manifest_sha256: None,
            response_bundle_path: None,
            response_bundle_file_count: None,
            response_bundle_bytes: None,
            response_bundle_sha256: None,
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
fn cli_checks_controller_fingerprint_lock() {
    let output = Command::new(env!("CARGO_BIN_EXE_glyph"))
        .arg("check-controller-fingerprint-lock")
        .output()
        .expect("check fingerprint lock");
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value =
        serde_json::from_slice(&output.stdout).expect("parse fingerprint lock report");
    assert_eq!(report["passed"], json!(true));
    assert_eq!(
        report["currentOverallSha256"],
        json!(controller_eval_fingerprint().overall_sha256)
    );
    assert_eq!(
        report["lockedOverallSha256"],
        json!(controller_eval_fingerprint().overall_sha256)
    );
    assert!(
        report["mismatches"]
            .as_array()
            .expect("fingerprint mismatches")
            .is_empty()
    );

    let output_dir = unique_temp_dir("fingerprint-lock");
    fs::create_dir_all(&output_dir).expect("create temp dir");
    let tampered_lock = output_dir.join("controller-fingerprint.lock.json");
    let mut lock: Value = serde_json::from_str(
        &fs::read_to_string("spec/controller-fingerprint.lock.json").expect("read lock"),
    )
    .expect("parse lock");
    lock["overallSha256"] = json!("bad");
    fs::write(
        &tampered_lock,
        format!("{}\n", serde_json::to_string_pretty(&lock).unwrap()),
    )
    .expect("write tampered lock");

    let rejected = Command::new(env!("CARGO_BIN_EXE_glyph"))
        .arg("check-controller-fingerprint-lock")
        .arg("--lock")
        .arg(&tampered_lock)
        .output()
        .expect("check tampered fingerprint lock");
    assert!(!rejected.status.success());
    assert!(
        String::from_utf8_lossy(&rejected.stderr)
            .contains("Controller fingerprint lock check did not pass")
    );
    let rejected_report: Value =
        serde_json::from_slice(&rejected.stdout).expect("parse rejected lock report");
    assert_eq!(rejected_report["passed"], json!(false));
    assert!(
        rejected_report["mismatches"]
            .as_array()
            .expect("rejected mismatches")
            .iter()
            .any(|mismatch| mismatch["section"] == "overallSha256")
    );

    let _ = fs::remove_dir_all(output_dir);
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
            prompt_bundle_overall_sha256: None,
            prompt_bundle_manifest_sha256: None,
            response_bundle_path: None,
            response_bundle_file_count: None,
            response_bundle_bytes: None,
            response_bundle_sha256: None,
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
    assert!(verification.replay.passed);
    assert!(
        verification
            .checks
            .iter()
            .all(|check| { check.status == ControllerRunVerificationStatus::Pass })
    );

    let mut tampered_cases = report.cases.clone();
    tampered_cases[0].successful_trace = false;
    let verification = verify_controller_run(&tampered_cases, &manifest_value, "out/results.jsonl");

    assert!(!verification.passed);
    assert!(!verification.replay.passed);
    assert!(verification.checks.iter().any(|check| {
        check.id == "replay_consistency" && check.status == ControllerRunVerificationStatus::Fail
    }));
    assert!(
        verification
            .replay
            .failures
            .iter()
            .any(|failure| failure.field == "successfulTrace"
                && failure.recorded == "false"
                && failure.replayed == "true")
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
fn cli_verifies_controller_shards_from_live_plan() {
    let output_dir = unique_temp_dir("controller-shards");
    fs::create_dir_all(&output_dir).expect("create shard dir");
    let jsonl_path = output_dir.join("family-hello.jsonl");
    let manifest_path = output_dir.join("family-hello.manifest.json");
    let plan_path = output_dir.join("live-plan.json");

    let eval = Command::new(env!("CARGO_BIN_EXE_glyph"))
        .arg("eval-controller")
        .arg("--family")
        .arg("hello_summary")
        .arg("--profile")
        .arg("normal")
        .arg("--case-limit")
        .arg("1")
        .arg("--jsonl")
        .arg(&jsonl_path)
        .arg("--manifest")
        .arg(&manifest_path)
        .output()
        .expect("run fixture shard");
    assert!(
        eval.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&eval.stdout),
        String::from_utf8_lossy(&eval.stderr)
    );

    let plan = json!({
        "version": "glyph-controller-live-plan/0.1",
        "totalExpectedRows": 4,
        "shards": [
            {
                "id": "family-hello",
                "family": "hello_summary",
                "jsonlPath": jsonl_path.display().to_string(),
                "manifestPath": manifest_path.display().to_string(),
                "expectedRows": 4
            }
        ]
    });
    fs::write(
        &plan_path,
        format!("{}\n", serde_json::to_string_pretty(&plan).unwrap()),
    )
    .expect("write shard plan");

    let verified = Command::new(env!("CARGO_BIN_EXE_glyph"))
        .arg("verify-controller-shards")
        .arg("--plan")
        .arg(&plan_path)
        .output()
        .expect("verify shards");
    assert!(
        verified.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verified.stdout),
        String::from_utf8_lossy(&verified.stderr)
    );
    let report: Value = serde_json::from_slice(&verified.stdout).expect("parse shard report");
    assert_eq!(report["passed"], json!(true));
    assert_eq!(report["shardCount"], json!(1));
    assert_eq!(report["verifiedShards"], json!(1));
    assert_eq!(report["expectedRows"], json!(4));
    assert_eq!(report["actualRows"], json!(4));

    let mut tampered_plan = plan;
    tampered_plan["totalExpectedRows"] = json!(5);
    tampered_plan["shards"][0]["expectedRows"] = json!(5);
    fs::write(
        &plan_path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&tampered_plan).unwrap()
        ),
    )
    .expect("write tampered shard plan");
    let rejected = Command::new(env!("CARGO_BIN_EXE_glyph"))
        .arg("verify-controller-shards")
        .arg("--plan")
        .arg(&plan_path)
        .output()
        .expect("verify tampered shards");
    assert!(!rejected.status.success());
    assert!(
        String::from_utf8_lossy(&rejected.stderr)
            .contains("Controller shard verification did not pass")
    );
    let rejected_report: Value =
        serde_json::from_slice(&rejected.stdout).expect("parse rejected shard report");
    assert_eq!(rejected_report["passed"], json!(false));
    assert!(
        rejected_report["shards"][0]["errors"]
            .as_array()
            .expect("shard errors")
            .iter()
            .any(|error| error
                .as_str()
                .expect("error string")
                .contains("actual rows 4 do not match expected rows 5"))
    );

    let _ = fs::remove_dir_all(output_dir);
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
    assert_eq!(coverage.required_comparison_rows, 864);
    assert_eq!(coverage.observed_comparison_rows, 0);
    assert_eq!(coverage.missing_comparison_rows, 864);
    assert_eq!(coverage.missing_comparison_row_examples.len(), 50);
    assert_eq!(coverage.missing_comparison_row_examples[0].bucket, "1b");
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
    assert_eq!(coverage.required_comparison_rows, 864);
    assert_eq!(coverage.observed_comparison_rows, 4);
    assert_eq!(coverage.missing_comparison_rows, 860);
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
fn cli_exports_controller_training_manifests() {
    let output_dir = unique_temp_dir("training-manifests");
    let dataset_jsonl = output_dir.join("controller-dataset.jsonl");
    let dataset_manifest = output_dir.join("controller-dataset.manifest.json");
    let curriculum_jsonl = output_dir.join("controller-curriculum.jsonl");
    let curriculum_manifest = output_dir.join("controller-curriculum.manifest.json");

    let dataset_output = Command::new(env!("CARGO_BIN_EXE_glyph"))
        .arg("export-controller-dataset")
        .arg("--output")
        .arg(&dataset_jsonl)
        .arg("--manifest")
        .arg(&dataset_manifest)
        .arg("--family")
        .arg("hello_summary")
        .arg("--profile")
        .arg("normal")
        .arg("--case-limit")
        .arg("1")
        .arg("--no-validation-split")
        .output()
        .expect("export dataset manifest");
    assert!(
        dataset_output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&dataset_output.stdout),
        String::from_utf8_lossy(&dataset_output.stderr)
    );

    let curriculum_output = Command::new(env!("CARGO_BIN_EXE_glyph"))
        .arg("export-controller-curriculum")
        .arg("--output")
        .arg(&curriculum_jsonl)
        .arg("--manifest")
        .arg(&curriculum_manifest)
        .arg("--family")
        .arg("hello_summary")
        .arg("--profile")
        .arg("normal")
        .arg("--case-limit")
        .arg("1")
        .arg("--no-validation-split")
        .output()
        .expect("export curriculum manifest");
    assert!(
        curriculum_output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&curriculum_output.stdout),
        String::from_utf8_lossy(&curriculum_output.stderr)
    );

    for (path, kind, record_count) in [
        (&dataset_manifest, "dataset", 1),
        (&curriculum_manifest, "curriculum", 7),
    ] {
        let manifest: Value =
            serde_json::from_str(&fs::read_to_string(path).expect("read manifest"))
                .expect("parse manifest");
        assert_eq!(
            manifest["version"],
            json!("glyph-controller-training-export-manifest/0.1")
        );
        assert_eq!(manifest["kind"], json!(kind));
        assert_eq!(manifest["counts"]["recordCount"], json!(record_count));
        assert!(
            manifest["artifact"]["bytes"]
                .as_u64()
                .expect("artifact bytes")
                > 0
        );
        assert_eq!(
            manifest["artifact"]["sha256"]
                .as_str()
                .expect("artifact sha")
                .len(),
            64
        );
        assert_eq!(
            manifest["controllerFingerprintSha256"]
                .as_str()
                .expect("fingerprint")
                .len(),
            64
        );
        assert_eq!(
            manifest["options"]["caseFilter"]["families"],
            json!(["hello_summary"])
        );
        assert_eq!(
            manifest["options"]["caseFilter"]["profiles"],
            json!(["normal"])
        );
        assert_eq!(manifest["options"]["caseFilter"]["limit"], json!(1));
        assert_eq!(manifest["options"]["validationStride"], json!(null));
    }

    for manifest_path in [&dataset_manifest, &curriculum_manifest] {
        let verified = Command::new(env!("CARGO_BIN_EXE_glyph"))
            .arg("verify-controller-training-export")
            .arg(manifest_path)
            .output()
            .expect("verify training export");
        assert!(
            verified.status.success(),
            "stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&verified.stdout),
            String::from_utf8_lossy(&verified.stderr)
        );
        let report: Value =
            serde_json::from_slice(&verified.stdout).expect("parse training verification");
        assert_eq!(report["passed"], json!(true));
        assert!(report["errors"].as_array().expect("errors").is_empty());
    }

    fs::write(&dataset_jsonl, "tampered\n").expect("tamper dataset jsonl");
    let tampered = Command::new(env!("CARGO_BIN_EXE_glyph"))
        .arg("verify-controller-training-export")
        .arg(&dataset_manifest)
        .output()
        .expect("verify tampered training export");
    assert!(!tampered.status.success());
    assert!(
        String::from_utf8_lossy(&tampered.stderr)
            .contains("Controller training export verification failed")
    );
    let tampered_report: Value =
        serde_json::from_slice(&tampered.stdout).expect("parse tampered training report");
    assert_eq!(tampered_report["passed"], json!(false));
    assert!(
        tampered_report["errors"]
            .as_array()
            .expect("tampered errors")
            .iter()
            .any(|error| error
                .as_str()
                .expect("error string")
                .contains("artifact sha256"))
    );

    let _ = fs::remove_dir_all(output_dir);
}

#[test]
fn controller_robustness_rejects_invalid_corpus_mutations() {
    let report = evaluate_controller_robustness();

    assert!(report.passed);
    assert_eq!(report.metrics.case_count, 72);
    assert_eq!(report.metrics.mutation_count, 152);
    assert_eq!(report.metrics.rejected_mutations, 152);
    assert_eq!(report.metrics.accepted_mutation_count, 0);
    assert!(report.accepted_mutations.is_empty());
    assert_eq!(
        report
            .metrics
            .by_kind
            .get("unknown_tool")
            .expect("unknown tool metrics")
            .mutation_count,
        72
    );
    assert_eq!(
        report
            .metrics
            .by_kind
            .get("unknown_variable")
            .expect("unknown variable metrics")
            .mutation_count,
        72
    );
    assert_eq!(
        report
            .metrics
            .by_kind
            .get("invalid_repair_max")
            .expect("invalid repair metrics")
            .mutation_count,
        8
    );
    assert!(
        report
            .checks
            .iter()
            .all(|check| check.status == ControllerRobustnessStatus::Pass)
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
fn cli_probes_openai_compatible_controller_endpoint() {
    let endpoint = spawn_openai_compatible_mock_server(4);
    let output = Command::new(env!("CARGO_BIN_EXE_glyph"))
        .arg("probe-controller-endpoint")
        .arg("--endpoint")
        .arg(endpoint)
        .arg("--prompt-mode")
        .arg("constrained")
        .arg("--grammar-payload")
        .arg("gbnf")
        .arg("--model")
        .arg("1b=tiny")
        .arg("--model")
        .arg("3b=small")
        .arg("--model")
        .arg("7b=medium")
        .arg("--model")
        .arg("frontier=large")
        .arg("--case")
        .arg("hello_summary_normal_short")
        .output()
        .expect("probe controller endpoint");
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let report: Value = serde_json::from_slice(&output.stdout).expect("parse endpoint probe");
    assert_eq!(report["passed"], json!(true));
    assert_eq!(report["probeCount"], json!(4));
    assert_eq!(report["completedProbes"], json!(4));
    assert_eq!(report["failedProbes"], json!(0));
    assert_eq!(report["probeCaseId"], json!("hello_summary_normal_short"));
    assert!(
        report["probes"]
            .as_array()
            .expect("probe records")
            .iter()
            .all(|probe| probe["requestHadGrammar"] == json!(true)
                && probe["requestHadResponseFormat"] == json!(false)
                && probe["status"] == json!("pass"))
    );
}

#[test]
fn controller_live_plan_shards_full_eval_by_family() {
    let report = plan_controller_live_run(ControllerLivePlanOptions {
        artifact_dir: "out/live-test".to_string(),
        endpoint: "http://localhost:9999/v1".to_string(),
    });

    assert_eq!(report.case_count, 72);
    assert_eq!(report.family_count, 9);
    assert_eq!(report.total_expected_rows, 864);
    assert_eq!(report.total_expected_model_calls, 2592);
    assert_eq!(report.shards.len(), 9);
    assert!(
        report
            .probe_endpoint_command
            .contains("probe-controller-endpoint --endpoint http://localhost:9999/v1")
    );
    assert!(report.probe_endpoint_command.contains("--prompt-mode all"));
    assert!(
        report
            .probe_endpoint_command
            .contains("--grammar-payload gbnf")
    );
    assert!(report.shards.iter().all(|shard| {
        shard.case_count == 8
            && shard.expected_rows == 96
            && shard.expected_model_calls == 288
            && shard.profiles == vec!["adversarial", "noisy", "normal", "terse"]
            && shard.preflight_command.contains("--prompt-mode all")
            && shard.eval_command.contains("--adapter openai-compatible")
            && shard.eval_command.contains("http://localhost:9999/v1")
    }));
    assert!(
        report
            .verify_shards_command
            .contains("verify-controller-shards --plan out/live-test/live-plan.json")
    );
    assert!(
        report
            .merge_command
            .contains("--source-manifest out/live-test/family-crud_app.manifest.json")
    );
    assert!(
        report
            .benchmark_report_command
            .contains("report-controller-benchmark out/live-test/live-merged.jsonl")
    );
    assert!(
        report
            .status_command
            .contains("status-controller-claim --jsonl out/live-test/live-merged.jsonl")
    );
}

#[test]
fn controller_offline_plan_shards_full_eval_by_bucket() {
    let report = plan_controller_offline_run(ControllerOfflinePlanOptions {
        artifact_dir: "out/offline-test".to_string(),
    });

    assert_eq!(report.version, CONTROLLER_OFFLINE_PLAN_VERSION);
    assert_eq!(report.case_count, 72);
    assert_eq!(report.model_buckets, vec!["1b", "3b", "7b", "frontier"]);
    assert_eq!(
        report.prompt_modes,
        vec!["constrained", "schema-only", "plain"]
    );
    assert_eq!(report.grammar_payload, "gbnf");
    assert_eq!(report.prompt_bundle_dir, "out/offline-test/prompts");
    assert_eq!(report.total_expected_rows, 864);
    assert_eq!(report.total_expected_response_files, 2592);
    assert_eq!(
        report.merged_jsonl_path,
        "out/offline-test/offline-merged.jsonl"
    );
    assert_eq!(
        report.merged_manifest_path,
        "out/offline-test/offline-merged.manifest.json"
    );
    assert_eq!(
        report.verification_report_path,
        "out/offline-test/offline-verification.json"
    );
    assert_eq!(
        report.coverage_report_path,
        "out/offline-test/offline-coverage.json"
    );
    assert_eq!(
        report.gate_report_path,
        "out/offline-test/offline-gate.json"
    );
    assert_eq!(
        report.benchmark_report_path,
        "out/offline-test/offline-benchmark-report.json"
    );
    assert_eq!(
        report.status_report_path,
        "out/offline-test/offline-status.json"
    );
    assert_eq!(
        report.finalize_report_path,
        "out/offline-test/offline-finalize-report.json"
    );
    assert_eq!(report.shards.len(), 4);
    assert!(
        report
            .prompt_bundle_command
            .contains("--emit-prompts out/offline-test/prompts")
    );
    assert!(
        report
            .verify_prompt_bundle_command
            .contains("verify-controller-prompt-bundle out/offline-test/prompts")
    );
    assert!(
        report
            .finalize_command
            .contains("finalize-controller-offline-run out/offline-test/offline-plan.json")
    );
    assert!(
        report
            .verify_shards_command
            .contains("verify-controller-shards --plan out/offline-test/offline-plan.json")
    );

    for shard in &report.shards {
        assert_eq!(shard.expected_rows, 216);
        assert_eq!(shard.expected_response_files, 648);
        assert_eq!(
            shard.queue_path,
            format!("out/offline-test/bucket-{}.queue.jsonl", shard.bucket)
        );
        assert_eq!(
            shard.queue_manifest_path,
            format!(
                "out/offline-test/bucket-{}.queue.manifest.json",
                shard.bucket
            )
        );
        assert!(
            shard
                .queue_command
                .contains("export-controller-offline-queue")
        );
        assert!(
            shard
                .queue_command
                .contains("--prompt-bundle out/offline-test/prompts")
        );
        assert!(shard.queue_command.contains(&format!(
            "--responses out/offline-test/responses-{}",
            shard.bucket
        )));
        assert!(
            shard
                .queue_command
                .contains(&format!("--model-id <{}-local-model-id>", shard.bucket))
        );
        assert!(
            shard
                .queue_command
                .contains(&format!("--output {}", shard.queue_path))
        );
        assert!(
            shard
                .queue_command
                .contains(&format!("--manifest {}", shard.queue_manifest_path))
        );
        assert!(
            shard
                .verify_queue_command
                .contains("verify-controller-offline-queue")
        );
        assert!(
            shard
                .verify_queue_command
                .contains(&shard.queue_manifest_path)
        );
        assert!(
            shard
                .run_queue_command
                .contains("run-controller-offline-queue")
        );
        assert!(shard.run_queue_command.contains(&shard.queue_manifest_path));
        assert!(shard.run_queue_command.contains(&format!(
            "<openai-compatible-endpoint-for-{}>",
            shard.bucket
        )));
        assert!(
            shard
                .check_responses_command
                .contains("check-controller-offline-responses")
        );
        assert!(
            shard
                .check_responses_command
                .contains("--prompt-bundle out/offline-test/prompts")
        );
        assert!(shard.check_responses_command.contains(&format!(
            "--responses out/offline-test/responses-{}",
            shard.bucket
        )));
        assert!(shard.score_command.contains("score-controller-responses"));
        assert!(
            shard
                .score_command
                .contains("--prompt-bundle out/offline-test/prompts")
        );
        assert!(
            shard
                .score_command
                .contains(&format!("--bucket {}", shard.bucket))
        );
        assert!(shard.score_command.contains(&format!(
            "--responses out/offline-test/responses-{}",
            shard.bucket
        )));
        assert!(
            shard
                .score_command
                .contains(&format!("--jsonl {}", shard.jsonl_path))
        );
        assert!(
            shard
                .score_command
                .contains(&format!("--manifest {}", shard.manifest_path))
        );
    }

    assert!(
        report
            .merge_command
            .contains("--source-manifest out/offline-test/bucket-1b.manifest.json")
    );
    assert!(
        report
            .merge_command
            .contains("out/offline-test/bucket-frontier.jsonl")
    );
    assert!(
        report
            .coverage_command
            .contains("coverage-controller out/offline-test/offline-merged.jsonl")
    );
    assert!(
        report
            .verify_command
            .contains("verify-controller-run out/offline-test/offline-merged.jsonl")
    );
    assert!(
        report
            .gate_command
            .contains("gate-controller out/offline-test/offline-merged.jsonl")
    );
    assert!(
        report
            .benchmark_report_command
            .contains("report-controller-benchmark out/offline-test/offline-merged.jsonl")
    );
    assert!(
        report
            .status_command
            .contains("status-controller-claim --jsonl out/offline-test/offline-merged.jsonl")
    );
}

#[test]
fn cli_finalizes_completed_offline_plan_artifacts() {
    let output_dir = unique_temp_dir("offline-finalizer");
    fs::create_dir_all(&output_dir).expect("create offline finalizer dir");
    let plan = plan_controller_offline_run(ControllerOfflinePlanOptions {
        artifact_dir: output_dir.display().to_string(),
    });
    let plan_path = output_dir.join("offline-plan.json");
    fs::write(
        &plan_path,
        serde_json::to_string_pretty(&plan).expect("serialize offline plan"),
    )
    .expect("write offline plan");

    let report = synthetic_claim_ready_report_with_adapter(ControllerAdapterMode::OfflineResponses);
    let selected_case_ids = controller_eval_cases()
        .into_iter()
        .map(|case| case.id)
        .collect::<Vec<_>>();
    for shard in &plan.shards {
        let parameter_class = parameter_class_for_test_bucket(&shard.bucket);
        let shard_cases = report
            .cases
            .iter()
            .filter(|case| case.parameter_class == parameter_class)
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(shard_cases.len(), shard.expected_rows);
        write_controller_eval_jsonl_for_test(PathBuf::from(&shard.jsonl_path), &shard_cases);
        let shard_report = ControllerEvalReport {
            mode: ControllerAdapterMode::OfflineResponses,
            actual_model_calls: shard_cases.len() * 3,
            grammar: report.grammar.clone(),
            by_model: summarize_controller_eval_by_model(&shard_cases),
            cases: shard_cases,
        };
        let manifest = build_controller_eval_run_manifest(
            10,
            Some(20),
            env!("CARGO_PKG_VERSION"),
            Some("synthetic-offline-finalizer".to_string()),
            Some(false),
            ControllerEvalRunConfig {
                adapter_mode: ControllerAdapterMode::OfflineResponses,
                endpoint: None,
                api_key_env: None,
                api_key_provided: false,
                models: vec![ControllerEvalRunModel {
                    parameter_class,
                    model_id: shard_report
                        .cases
                        .first()
                        .expect("shard has cases")
                        .model_id
                        .clone(),
                }],
                prompt_modes: ControllerPromptMode::all(),
                grammar_payload: ControllerGrammarPayload::Gbnf,
                case_filter: ControllerEvalRunCaseFilter {
                    case_ids: selected_case_ids.clone(),
                    tags: vec![],
                    families: vec![],
                    profiles: vec![],
                    limit: None,
                },
                selected_case_ids: selected_case_ids.clone(),
                selected_case_count: selected_case_ids.len(),
                artifacts: ControllerEvalRunArtifacts {
                    jsonl_path: Some(shard.jsonl_path.clone()),
                    manifest_path: Some(shard.manifest_path.clone()),
                    emit_prompts_path: Some(plan.prompt_bundle_dir.clone()),
                    prompt_bundle_overall_sha256: Some("0".repeat(64)),
                    prompt_bundle_manifest_sha256: Some("1".repeat(64)),
                    response_bundle_path: Some(shard.response_dir.clone()),
                    response_bundle_file_count: Some(shard.expected_response_files),
                    response_bundle_bytes: Some(1),
                    response_bundle_sha256: Some("2".repeat(64)),
                    stream_jsonl: false,
                },
            },
            Some(&shard_report),
        );
        fs::write(
            &shard.manifest_path,
            serde_json::to_string_pretty(&manifest).expect("serialize shard manifest"),
        )
        .expect("write shard manifest");
    }

    let finalized = Command::new(env!("CARGO_BIN_EXE_glyph"))
        .arg("finalize-controller-offline-run")
        .arg(&plan_path)
        .output()
        .expect("finalize offline plan");
    assert!(
        finalized.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&finalized.stdout),
        String::from_utf8_lossy(&finalized.stderr)
    );
    let finalizer_report: Value =
        serde_json::from_slice(&finalized.stdout).expect("parse finalizer report");
    assert_eq!(finalizer_report["passed"], json!(true));
    assert_eq!(finalizer_report["merge"]["outputRows"], json!(864));
    assert_eq!(finalizer_report["verification"]["passed"], json!(true));
    assert_eq!(
        finalizer_report["coverage"]["coverageComplete"],
        json!(true)
    );
    assert_eq!(finalizer_report["gate"]["passed"], json!(true));
    assert_eq!(finalizer_report["benchmark"]["passed"], json!(true));
    assert_eq!(finalizer_report["status"]["claimAllowed"], json!(true));

    for path in [
        &plan.merged_jsonl_path,
        &plan.merged_manifest_path,
        &plan.verification_report_path,
        &plan.coverage_report_path,
        &plan.gate_report_path,
        &plan.benchmark_report_path,
        &plan.status_report_path,
        &plan.finalize_report_path,
    ] {
        assert!(
            PathBuf::from(path).exists(),
            "missing finalized artifact {path}"
        );
    }
    let status: Value = serde_json::from_str(
        &fs::read_to_string(&plan.status_report_path).expect("read offline status"),
    )
    .expect("parse offline status");
    assert_eq!(status["claimAllowed"], json!(true));
    let merged_manifest: Value = serde_json::from_str(
        &fs::read_to_string(&plan.merged_manifest_path).expect("read merged manifest"),
    )
    .expect("parse merged manifest");
    assert_eq!(merged_manifest["manifestKind"], json!("merged"));
    assert_eq!(
        merged_manifest["sourceManifests"]
            .as_array()
            .expect("source manifests")
            .len(),
        4
    );

    let _ = fs::remove_dir_all(output_dir);
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
            .any(|check| check.id == "fingerprint_lock"
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
            .any(|check| check.id == "controller_robustness"
                && check.status == ControllerClaimAuditStatus::Pass)
    );
    assert!(audit.robustness.passed);
    assert!(
        audit
            .checks
            .iter()
            .any(|check| check.id == "glyph_conformance"
                && check.status == ControllerClaimAuditStatus::Pass)
    );
    assert!(audit.conformance.passed);
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
fn controller_claim_status_reports_static_ready_but_live_blocked() {
    let status = controller_claim_status(ControllerClaimStatusInput {
        cases: None,
        manifest: None,
        jsonl_path: None,
    });

    assert!(!status.claim_allowed);
    assert_eq!(
        status.phase,
        ControllerClaimStatusPhase::AwaitingLiveEvidence
    );
    assert!(status.static_readiness_passed);
    assert!(!status.live_evidence_supplied);
    assert!(
        status
            .passed_checks
            .iter()
            .any(|check| check.id == "fingerprint_lock")
    );
    assert!(
        status
            .passed_checks
            .iter()
            .any(|check| check.id == "controller_curriculum")
    );
    assert!(
        status
            .passed_checks
            .iter()
            .any(|check| check.id == "controller_robustness")
    );
    assert!(
        status
            .passed_checks
            .iter()
            .any(|check| check.id == "glyph_conformance")
    );
    assert!(
        status
            .failed_checks
            .iter()
            .any(|check| check.id == "live_jsonl_supplied")
    );
    assert!(
        status
            .next_actions
            .iter()
            .any(|action| action.contains("--prompt-mode all"))
    );
    assert!(
        status
            .next_actions
            .iter()
            .any(|action| action.contains("plan-controller-live-run"))
    );
    assert!(
        status
            .next_actions
            .iter()
            .any(|action| action.contains("probe-controller-endpoint"))
    );
    assert!(
        status
            .next_actions
            .iter()
            .any(|action| action.contains("plan-controller-offline-run"))
    );
    assert!(
        status
            .next_actions
            .iter()
            .any(|action| action.contains("export-controller-offline-queue"))
    );
    assert!(
        status
            .next_actions
            .iter()
            .any(|action| action.contains("verify-controller-offline-queue"))
    );
    assert!(
        status
            .next_actions
            .iter()
            .any(|action| action.contains("run-controller-offline-queue"))
    );
    assert!(
        status
            .next_actions
            .iter()
            .any(|action| action.contains("check-controller-offline-responses"))
    );
    assert!(
        status
            .next_actions
            .iter()
            .any(|action| action.contains("finalize-controller-offline-run"))
    );
    assert!(
        status
            .next_actions
            .iter()
            .any(|action| action.contains("verify-controller-shards"))
    );
}

#[test]
fn controller_claim_status_treats_fingerprint_lock_as_static_readiness() {
    let mut audit = audit_controller_claim(ControllerClaimAuditInput {
        cases: None,
        manifest: None,
        jsonl_path: None,
    });
    let fingerprint_lock = audit
        .checks
        .iter_mut()
        .find(|check| check.id == "fingerprint_lock")
        .expect("fingerprint lock check exists");
    fingerprint_lock.status = ControllerClaimAuditStatus::Fail;
    fingerprint_lock.observed = "locked=stale, current=current".to_string();

    let status = controller_claim_status_from_audit(audit);

    assert!(!status.claim_allowed);
    assert_eq!(
        status.phase,
        ControllerClaimStatusPhase::StaticReadinessFailing
    );
    assert!(!status.static_readiness_passed);
    assert!(status.failed_checks.iter().any(|check| {
        check.id == "fingerprint_lock" && check.observed == "locked=stale, current=current"
    }));
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
        "fingerprint-lock.json",
        "dataset-quality.json",
        "curriculum-quality.json",
        "robustness.json",
        "conformance.json",
        "live-plan.json",
        "offline-plan.json",
        "request-preview.json",
        "status.json",
        "claim-audit.json",
        "summary.json",
        "README.md",
        "evidence-manifest.json",
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
    assert_eq!(summary["claimAllowed"], json!(false));
    assert_eq!(summary["phase"], json!("awaiting-live-evidence"));
    assert_eq!(summary["liveEvidenceSupplied"], json!(false));
    assert_eq!(summary["fingerprintLockPassed"], json!(true));
    assert_eq!(summary["datasetQualityPassed"], json!(true));
    assert_eq!(summary["curriculumQualityPassed"], json!(true));
    assert_eq!(summary["robustnessPassed"], json!(true));
    assert_eq!(summary["conformancePassed"], json!(true));
    assert!(
        summary["files"]
            .as_array()
            .expect("summary files")
            .iter()
            .any(|file| file == "evidence-manifest.json")
    );
    assert!(
        summary["files"]
            .as_array()
            .expect("summary files")
            .iter()
            .any(|file| file == "fingerprint-lock.json")
    );
    assert!(
        summary["files"]
            .as_array()
            .expect("summary files")
            .iter()
            .any(|file| file == "offline-plan.json")
    );

    let offline_plan: Value = serde_json::from_str(
        &fs::read_to_string(output_dir.join("offline-plan.json")).expect("read offline plan"),
    )
    .expect("parse offline plan");
    assert_eq!(
        offline_plan["version"],
        json!("glyph-controller-offline-plan/0.1")
    );
    assert_eq!(offline_plan["totalExpectedRows"], json!(864));

    let fingerprint_lock: Value = serde_json::from_str(
        &fs::read_to_string(output_dir.join("fingerprint-lock.json"))
            .expect("read fingerprint lock report"),
    )
    .expect("parse fingerprint lock report");
    assert_eq!(fingerprint_lock["passed"], json!(true));
    assert_eq!(
        fingerprint_lock["currentOverallSha256"],
        fingerprint_lock["lockedOverallSha256"]
    );

    let evidence_manifest: Value = serde_json::from_str(
        &fs::read_to_string(output_dir.join("evidence-manifest.json"))
            .expect("read evidence manifest"),
    )
    .expect("parse evidence manifest");
    assert_eq!(
        evidence_manifest["version"],
        json!("glyph-evidence-pack-manifest/0.1")
    );
    assert_eq!(evidence_manifest["algorithm"], json!("sha256"));
    assert_eq!(
        evidence_manifest["overallSha256"]
            .as_str()
            .expect("overall hash")
            .len(),
        64
    );
    assert!(
        evidence_manifest["artifactCount"]
            .as_u64()
            .expect("artifact count")
            >= 13
    );
    assert!(
        evidence_manifest["totalBytes"]
            .as_u64()
            .expect("total bytes")
            > 0
    );
    let artifact_paths = evidence_manifest["artifacts"]
        .as_array()
        .expect("manifest artifacts")
        .iter()
        .map(|artifact| artifact["path"].as_str().expect("artifact path"))
        .collect::<Vec<_>>();
    assert!(artifact_paths.contains(&"fingerprint.json"));
    assert!(artifact_paths.contains(&"fingerprint-lock.json"));
    assert!(artifact_paths.contains(&"offline-plan.json"));
    assert!(artifact_paths.contains(&"summary.json"));
    assert!(artifact_paths.contains(&"README.md"));
    assert!(!artifact_paths.contains(&"evidence-manifest.json"));
    assert!(
        evidence_manifest["excludedArtifacts"]
            .as_array()
            .expect("excluded artifacts")
            .iter()
            .any(|artifact| artifact == "evidence-manifest.json")
    );

    let stdout_summary: Value =
        serde_json::from_slice(&output.stdout).expect("parse stdout summary");
    assert_eq!(
        stdout_summary["output"],
        json!(output_dir.display().to_string())
    );

    let verified = Command::new(env!("CARGO_BIN_EXE_glyph"))
        .arg("verify-controller-evidence-pack")
        .arg(&output_dir)
        .output()
        .expect("verify evidence pack");
    assert!(
        verified.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verified.stdout),
        String::from_utf8_lossy(&verified.stderr)
    );
    let verify_report: Value =
        serde_json::from_slice(&verified.stdout).expect("parse verification report");
    assert_eq!(verify_report["passed"], json!(true));
    assert_eq!(verify_report["checkedArtifacts"], json!(13));
    assert!(
        verify_report["missingArtifacts"]
            .as_array()
            .expect("missing artifacts")
            .is_empty()
    );
    assert!(
        verify_report["mismatchedArtifacts"]
            .as_array()
            .expect("mismatched artifacts")
            .is_empty()
    );

    fs::write(
        output_dir.join("README.md"),
        "# Glyph Controller Evidence Pack\n\nTampered\n",
    )
    .expect("tamper with evidence pack");
    let tampered = Command::new(env!("CARGO_BIN_EXE_glyph"))
        .arg("verify-controller-evidence-pack")
        .arg(&output_dir)
        .output()
        .expect("verify tampered evidence pack");
    assert!(!tampered.status.success());
    assert!(
        String::from_utf8_lossy(&tampered.stderr).contains("Evidence pack verification failed")
    );
    let tampered_report: Value =
        serde_json::from_slice(&tampered.stdout).expect("parse tampered report");
    assert_eq!(tampered_report["passed"], json!(false));
    assert!(
        tampered_report["mismatchedArtifacts"]
            .as_array()
            .expect("tampered mismatches")
            .iter()
            .any(|artifact| artifact["path"] == "README.md")
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
                prompt_bundle_overall_sha256: None,
                prompt_bundle_manifest_sha256: None,
                response_bundle_path: None,
                response_bundle_file_count: None,
                response_bundle_bytes: None,
                response_bundle_sha256: None,
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
fn controller_claim_status_can_be_claim_ready_with_synthetic_live_evidence() {
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
                prompt_bundle_overall_sha256: None,
                prompt_bundle_manifest_sha256: None,
                response_bundle_path: None,
                response_bundle_file_count: None,
                response_bundle_bytes: None,
                response_bundle_sha256: None,
                stream_jsonl: true,
            },
        },
        Some(&report),
    );
    let manifest = serde_json::to_value(manifest).unwrap();
    let status = controller_claim_status(ControllerClaimStatusInput {
        cases: Some(&report.cases),
        manifest: Some(&manifest),
        jsonl_path: Some(jsonl_path),
    });

    assert!(status.claim_allowed);
    assert_eq!(status.phase, ControllerClaimStatusPhase::ClaimReady);
    assert!(status.static_readiness_passed);
    assert!(status.live_evidence_supplied);
    assert!(status.failed_checks.is_empty());
    assert!(status.audit.claim_ready);
}

#[test]
fn controller_claim_status_accepts_synthetic_offline_response_evidence() {
    let report = synthetic_claim_ready_report_with_adapter(ControllerAdapterMode::OfflineResponses);
    let jsonl_path = "out/offline-controller-eval.jsonl";
    let manifest = build_controller_eval_run_manifest(
        10,
        Some(20),
        "0.1.0-test",
        Some("abc123".to_string()),
        Some(false),
        ControllerEvalRunConfig {
            adapter_mode: ControllerAdapterMode::OfflineResponses,
            endpoint: None,
            api_key_env: None,
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
                manifest_path: Some("out/offline-controller-eval.manifest.json".to_string()),
                emit_prompts_path: Some("out/prompts".to_string()),
                prompt_bundle_overall_sha256: Some("0".repeat(64)),
                prompt_bundle_manifest_sha256: Some("1".repeat(64)),
                response_bundle_path: Some("out/responses".to_string()),
                response_bundle_file_count: Some(72 * 3 * 3),
                response_bundle_bytes: Some(1),
                response_bundle_sha256: Some("2".repeat(64)),
                stream_jsonl: false,
            },
        },
        Some(&report),
    );
    let manifest = serde_json::to_value(manifest).unwrap();
    let status = controller_claim_status(ControllerClaimStatusInput {
        cases: Some(&report.cases),
        manifest: Some(&manifest),
        jsonl_path: Some(jsonl_path),
    });

    assert!(status.claim_allowed);
    assert_eq!(status.phase, ControllerClaimStatusPhase::ClaimReady);
    assert!(status.static_readiness_passed);
    assert!(status.live_evidence_supplied);
    assert!(status.failed_checks.is_empty());
    assert!(status.audit.claim_ready);
    assert!(status.audit.gate.as_ref().is_some_and(|gate| gate.passed));
    assert!(
        status
            .audit
            .verification
            .as_ref()
            .is_some_and(|verification| verification.replay.passed)
    );
}

#[test]
fn controller_benchmark_report_rejects_fixture_only_results() {
    let report = run_controller_eval_with_options(ControllerEvalOptions {
        models: None,
        prompt_modes: ControllerPromptMode::all(),
        ..ControllerEvalOptions::default()
    });
    let benchmark = controller_benchmark_report(&report.cases);

    assert!(!benchmark.passed);
    assert!(!benchmark.gate_passed);
    assert_eq!(benchmark.live_case_rows, 0);
    assert!(
        benchmark.comparisons.iter().all(|comparison| {
            comparison.status == ControllerBenchmarkComparisonStatus::Missing
        })
    );
}

#[test]
fn controller_benchmark_report_passes_for_synthetic_live_evidence() {
    let report = synthetic_claim_ready_report();
    let benchmark = controller_benchmark_report(&report.cases);

    assert!(benchmark.passed);
    assert!(benchmark.gate_passed);
    assert_eq!(benchmark.target_case_rows, 72);
    assert_eq!(benchmark.comparisons.len(), 6);
    assert!(
        benchmark
            .comparisons
            .iter()
            .all(|comparison| { comparison.status == ControllerBenchmarkComparisonStatus::Pass })
    );
    assert!(benchmark.comparisons.iter().any(|comparison| {
        comparison.id == "one_b_constrained_vs_larger_plain_trace_rate"
            && comparison.delta == Some(0.0)
    }));
    assert!(!benchmark.model_summaries.is_empty());
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
            weaken_json_tool_plan_baseline(case);
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
            weaken_json_tool_plan_baseline(case);
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
            weaken_json_tool_plan_baseline(case);

            if !degraded_target {
                case.successful_trace = false;
                case.glyph_beats_json_tool_plan = false;
                case.glyph_beats_direct_prose = false;
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

#[test]
fn controller_gate_rejects_incomplete_comparison_matrix() {
    let mut report = synthetic_claim_ready_report();
    let removed = report
        .cases
        .iter()
        .position(|case| {
            case.case_id == "hello_summary_normal_short"
                && case.parameter_class == ControllerParameterClass::Frontier
                && case.prompt_mode == ControllerPromptMode::Plain
        })
        .expect("synthetic report includes frontier plain baseline row");
    report.cases.remove(removed);

    let gate = evaluate_controller_gate(&report.cases);

    assert!(!gate.passed);
    assert_eq!(gate.metrics.required_comparison_rows, 864);
    assert_eq!(gate.metrics.observed_comparison_rows, 863);
    assert_eq!(gate.metrics.missing_comparison_rows, 1);
    assert!(gate.checks.iter().any(|check| {
        check.id == "comparison_matrix_coverage" && check.status == ControllerGateCheckStatus::Fail
    }));
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
    synthetic_claim_ready_report_with_adapter(ControllerAdapterMode::OpenAiCompatible)
}

fn synthetic_claim_ready_report_with_adapter(
    adapter_mode: ControllerAdapterMode,
) -> ControllerEvalReport {
    let mut report = run_controller_eval_with_options(ControllerEvalOptions {
        models: None,
        prompt_modes: ControllerPromptMode::all(),
        ..ControllerEvalOptions::default()
    });

    report.mode = adapter_mode.clone();
    report.actual_model_calls = report.cases.len() * 3;

    for case in &mut report.cases {
        case.adapter_mode = adapter_mode.clone();
        if case.parameter_class == ControllerParameterClass::OneB
            && case.prompt_mode == ControllerPromptMode::Constrained
        {
            case.grammar_payload = ControllerGrammarPayload::Gbnf;
            weaken_json_tool_plan_baseline(case);
        }
    }

    report.by_model = summarize_controller_eval_by_model(&report.cases);
    report
}

fn weaken_json_tool_plan_baseline(case: &mut glyph::eval::controller::ControllerEvalCaseResult) {
    let plan = json!({
        "goal": "Generic JSON baseline stops before final export",
        "context": {
            "baseline": "generic_json_tool_plan",
            "weakness": "omits final executable artifact export"
        },
        "steps": [
            {
                "op": "SPEC",
                "args": {
                    "request": "Capture requirements, but do not finish the harness workflow.",
                    "baseline": "json_tool_plan"
                },
                "assignTo": "spec"
            },
            {
                "op": "PLAN",
                "args": {
                    "input": { "var": "spec" },
                    "detail": "Build an intermediate plan only."
                },
                "assignTo": "plan"
            },
            {
                "op": "GEN",
                "args": {
                    "input": { "var": "plan" },
                    "note": "Generate intermediate artifacts without export."
                },
                "assignTo": "files"
            },
            {
                "op": "CHECK",
                "args": {
                    "target": { "var": "files" },
                    "using": ["types", "tests"]
                },
                "assignTo": "report"
            }
        ]
    })
    .to_string();

    case.generated_json_tool_plan = plan.clone();
    case.json_tool_plan_raw_output = plan.clone();
    case.json_tool_plan_parse_ok = true;
    case.json_tool_plan_run_ok = true;
    case.json_tool_plan_successful_trace = false;
    case.glyph_beats_json_tool_plan = case.successful_trace;
    case.json_tool_plan_trace_event_count = 4;
    case.json_tool_plan_final_output_count = 0;
    case.json_tool_plan_output_tokens = approximate_tokens(&plan);
    case.json_tool_plan_parse_error = None;
    case.json_tool_plan_run_error = None;
    case.json_tool_plan_error = None;
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
fn cli_exports_prompt_bundle_manifest() {
    let output_dir = unique_temp_dir("prompt-bundle");
    let output = Command::new(env!("CARGO_BIN_EXE_glyph"))
        .arg("eval-controller")
        .arg("--prompt-mode")
        .arg("constrained")
        .arg("--grammar-payload")
        .arg("gbnf")
        .arg("--emit-prompts")
        .arg(&output_dir)
        .arg("--family")
        .arg("hello_summary")
        .arg("--profile")
        .arg("normal")
        .arg("--case-limit")
        .arg("1")
        .output()
        .expect("export prompt bundle");
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    for file in [
        "glyph.gbnf",
        "controller-output.schema.json",
        "generic-tool-plan.schema.json",
        "cases/constrained/hello_summary_normal_short.json",
        "prompt-bundle-manifest.json",
    ] {
        assert!(
            output_dir.join(file).is_file(),
            "expected prompt bundle file {file}"
        );
    }

    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(output_dir.join("prompt-bundle-manifest.json"))
            .expect("read prompt manifest"),
    )
    .expect("parse prompt manifest");
    assert_eq!(
        manifest["version"],
        json!("glyph-controller-prompt-bundle/0.1")
    );
    assert_eq!(manifest["promptModes"], json!(["constrained"]));
    assert_eq!(manifest["grammarPayload"], json!("gbnf"));
    assert_eq!(manifest["caseCount"], json!(1));
    assert_eq!(manifest["promptFileCount"], json!(1));
    assert_eq!(manifest["artifactCount"], json!(4));
    assert!(
        manifest["totalBytes"]
            .as_u64()
            .expect("prompt bundle bytes")
            > 0
    );
    assert_eq!(
        manifest["overallSha256"]
            .as_str()
            .expect("overall prompt hash")
            .len(),
        64
    );
    assert_eq!(
        manifest["controllerFingerprintSha256"]
            .as_str()
            .expect("prompt fingerprint")
            .len(),
        64
    );
    let artifact_paths = manifest["artifacts"]
        .as_array()
        .expect("prompt artifacts")
        .iter()
        .map(|artifact| artifact["path"].as_str().expect("artifact path"))
        .collect::<Vec<_>>();
    assert!(artifact_paths.contains(&"glyph.gbnf"));
    assert!(artifact_paths.contains(&"controller-output.schema.json"));
    assert!(artifact_paths.contains(&"generic-tool-plan.schema.json"));
    assert!(artifact_paths.contains(&"cases/constrained/hello_summary_normal_short.json"));
    assert!(!artifact_paths.contains(&"prompt-bundle-manifest.json"));
    assert!(
        manifest["excludedArtifacts"]
            .as_array()
            .expect("excluded artifacts")
            .iter()
            .any(|artifact| artifact == "prompt-bundle-manifest.json")
    );

    let prompt_file: Value = serde_json::from_str(
        &fs::read_to_string(output_dir.join("cases/constrained/hello_summary_normal_short.json"))
            .expect("read prompt file"),
    )
    .expect("parse prompt file");
    assert_eq!(prompt_file["grammarPayload"], json!("gbnf"));
    assert!(
        prompt_file["prompt"]
            .as_str()
            .expect("prompt text")
            .contains("Decoder constraint:")
    );

    let verified = Command::new(env!("CARGO_BIN_EXE_glyph"))
        .arg("verify-controller-prompt-bundle")
        .arg(&output_dir)
        .output()
        .expect("verify prompt bundle");
    assert!(
        verified.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verified.stdout),
        String::from_utf8_lossy(&verified.stderr)
    );
    let verify_report: Value =
        serde_json::from_slice(&verified.stdout).expect("parse prompt verification");
    assert_eq!(verify_report["passed"], json!(true));
    assert_eq!(verify_report["checkedArtifacts"], json!(4));
    assert!(
        verify_report["errors"]
            .as_array()
            .expect("prompt verification errors")
            .is_empty()
    );

    let queue_dir = unique_temp_dir("offline-queue");
    let queue_path = queue_dir.join("offline-queue.jsonl");
    let queue_manifest_path = queue_dir.join("offline-queue.manifest.json");
    let queue_responses_dir = queue_dir.join("responses");
    let queue = Command::new(env!("CARGO_BIN_EXE_glyph"))
        .arg("export-controller-offline-queue")
        .arg("--prompt-bundle")
        .arg(&output_dir)
        .arg("--responses")
        .arg(&queue_responses_dir)
        .arg("--output")
        .arg(&queue_path)
        .arg("--manifest")
        .arg(&queue_manifest_path)
        .output()
        .expect("export offline queue");
    assert!(
        queue.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&queue.stdout),
        String::from_utf8_lossy(&queue.stderr)
    );
    let queue_report: Value =
        serde_json::from_slice(&queue.stdout).expect("parse offline queue report");
    assert_eq!(queue_report["recordCount"], json!(3));
    assert_eq!(queue_report["promptFileCount"], json!(1));
    assert_eq!(queue_report["modelId"], json!("model-under-test"));
    assert_eq!(
        queue_report["promptBundleOverallSha256"],
        manifest["overallSha256"]
    );
    assert_eq!(
        queue_report["outputPath"],
        json!(queue_path.display().to_string())
    );

    let queue_lines = fs::read_to_string(&queue_path)
        .expect("read offline queue")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("parse queue record"))
        .collect::<Vec<_>>();
    assert_eq!(queue_lines.len(), 3);
    assert!(
        queue_lines
            .iter()
            .all(|record| record["openaiRequestBody"]["model"] == "model-under-test")
    );
    assert!(queue_lines.iter().any(|record| {
        record["requestKind"] == "glyph"
            && record["promptField"] == "prompt"
            && record["openaiRequestBody"]["messages"]
                .as_array()
                .expect("request messages")
                .len()
                == 2
            && record["responsePath"]
                .as_str()
                .expect("glyph response path")
                .ends_with("cases/constrained/hello_summary_normal_short.glyph.txt")
    }));
    assert!(queue_lines.iter().any(|record| {
        record["requestKind"] == "json-tool-plan"
            && record["promptField"] == "jsonToolPlanPrompt"
            && record["responsePath"]
                .as_str()
                .expect("json tool plan response path")
                .ends_with("cases/constrained/hello_summary_normal_short.json-tool-plan.txt")
    }));
    assert!(queue_lines.iter().any(|record| {
        record["requestKind"] == "direct-prose"
            && record["promptField"] == "directProsePrompt"
            && record["responsePath"]
                .as_str()
                .expect("direct prose response path")
                .ends_with("cases/constrained/hello_summary_normal_short.direct-prose.txt")
    }));
    let queue_manifest: Value = serde_json::from_str(
        &fs::read_to_string(&queue_manifest_path).expect("read offline queue manifest"),
    )
    .expect("parse offline queue manifest");
    assert_eq!(
        queue_manifest["version"],
        json!("glyph-controller-offline-queue-export/0.1")
    );
    assert_eq!(queue_manifest["outputSha256"], queue_report["outputSha256"]);

    let verified_queue = Command::new(env!("CARGO_BIN_EXE_glyph"))
        .arg("verify-controller-offline-queue")
        .arg(&queue_manifest_path)
        .output()
        .expect("verify offline queue");
    assert!(
        verified_queue.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verified_queue.stdout),
        String::from_utf8_lossy(&verified_queue.stderr)
    );
    let verified_queue_report: Value =
        serde_json::from_slice(&verified_queue.stdout).expect("parse queue verification");
    assert_eq!(verified_queue_report["passed"], json!(true));
    assert_eq!(verified_queue_report["checkedRecords"], json!(3));
    assert_eq!(verified_queue_report["promptBundlePassed"], json!(true));

    let endpoint = spawn_openai_compatible_mock_server(3);
    let run_queue = Command::new(env!("CARGO_BIN_EXE_glyph"))
        .arg("run-controller-offline-queue")
        .arg(&queue_manifest_path)
        .arg("--endpoint")
        .arg(&endpoint)
        .output()
        .expect("run offline queue");
    assert!(
        run_queue.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run_queue.stdout),
        String::from_utf8_lossy(&run_queue.stderr)
    );
    let run_queue_report: Value =
        serde_json::from_slice(&run_queue.stdout).expect("parse queue run report");
    assert_eq!(run_queue_report["passed"], json!(true));
    assert_eq!(run_queue_report["attemptedRecords"], json!(3));
    assert_eq!(run_queue_report["writtenResponses"], json!(3));
    assert_eq!(run_queue_report["failedRecords"], json!(0));
    assert!(
        fs::read_to_string(
            queue_responses_dir.join("cases/constrained/hello_summary_normal_short.glyph.txt")
        )
        .expect("read generated glyph response")
        .contains("mock local decoder response")
    );

    let checked_queue_responses = Command::new(env!("CARGO_BIN_EXE_glyph"))
        .arg("check-controller-offline-responses")
        .arg("--prompt-bundle")
        .arg(&output_dir)
        .arg("--responses")
        .arg(&queue_responses_dir)
        .output()
        .expect("check generated queue responses");
    assert!(
        checked_queue_responses.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&checked_queue_responses.stdout),
        String::from_utf8_lossy(&checked_queue_responses.stderr)
    );

    fs::write(&queue_path, "{}\n").expect("tamper offline queue");
    let rejected_queue = Command::new(env!("CARGO_BIN_EXE_glyph"))
        .arg("verify-controller-offline-queue")
        .arg(&queue_manifest_path)
        .output()
        .expect("verify tampered offline queue");
    assert!(!rejected_queue.status.success());
    assert!(
        String::from_utf8_lossy(&rejected_queue.stderr)
            .contains("Controller offline queue verification failed")
    );
    let rejected_queue_report: Value =
        serde_json::from_slice(&rejected_queue.stdout).expect("parse rejected queue verification");
    assert_eq!(rejected_queue_report["passed"], json!(false));
    assert!(
        rejected_queue_report["errors"]
            .as_array()
            .expect("queue verification errors")
            .iter()
            .any(|error| error
                .as_str()
                .expect("error string")
                .contains("queue sha256 does not match manifest"))
    );

    fs::write(
        output_dir.join("cases/constrained/hello_summary_normal_short.json"),
        "{}\n",
    )
    .expect("tamper prompt bundle");
    let tampered = Command::new(env!("CARGO_BIN_EXE_glyph"))
        .arg("verify-controller-prompt-bundle")
        .arg(&output_dir)
        .output()
        .expect("verify tampered prompt bundle");
    assert!(!tampered.status.success());
    assert!(
        String::from_utf8_lossy(&tampered.stderr)
            .contains("Controller prompt bundle verification failed")
    );
    let tampered_report: Value =
        serde_json::from_slice(&tampered.stdout).expect("parse tampered prompt verification");
    assert_eq!(tampered_report["passed"], json!(false));
    assert!(
        tampered_report["mismatchedArtifacts"]
            .as_array()
            .expect("prompt mismatches")
            .iter()
            .any(|artifact| artifact["path"]
                == "cases/constrained/hello_summary_normal_short.json")
    );

    let _ = fs::remove_dir_all(output_dir);
    let _ = fs::remove_dir_all(queue_dir);
}

#[test]
fn cli_scores_offline_controller_responses_from_prompt_bundle() {
    let bundle_dir = unique_temp_dir("offline-prompt-bundle");
    let responses_dir = unique_temp_dir("offline-responses");
    let jsonl_path = responses_dir.join("offline.jsonl");
    let manifest_path = responses_dir.join("offline.manifest.json");
    let offline_plan_path = responses_dir.join("offline-plan.json");
    let export = Command::new(env!("CARGO_BIN_EXE_glyph"))
        .arg("eval-controller")
        .arg("--prompt-mode")
        .arg("constrained")
        .arg("--grammar-payload")
        .arg("gbnf")
        .arg("--emit-prompts")
        .arg(&bundle_dir)
        .arg("--family")
        .arg("hello_summary")
        .arg("--profile")
        .arg("normal")
        .arg("--case-limit")
        .arg("1")
        .output()
        .expect("export prompt bundle");
    assert!(
        export.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&export.stdout),
        String::from_utf8_lossy(&export.stderr)
    );

    let case_id = "hello_summary_normal_short";
    let eval_case = controller_eval_cases()
        .into_iter()
        .find(|case| case.id == case_id)
        .expect("fixture case exists");
    let response_case_dir = responses_dir.join("cases").join("constrained");
    fs::create_dir_all(&response_case_dir).expect("create offline response dir");
    fs::write(
        response_case_dir.join(format!("{case_id}.glyph.txt")),
        eval_case.expected_glyph,
    )
    .expect("write glyph response");
    fs::write(
        response_case_dir.join(format!("{case_id}.json-tool-plan.txt")),
        json!({
            "goal": "Say hello through the harness",
            "steps": [
                {
                    "op": "SPEC",
                    "args": { "message": "hello world" },
                    "assignTo": "spec"
                },
                {
                    "op": "SUM",
                    "args": { "input": { "var": "spec" } },
                    "assignTo": "summary"
                },
                {
                    "op": "EXPORT",
                    "args": { "target": { "var": "summary" } }
                }
            ]
        })
        .to_string(),
    )
    .expect("write JSON tool-plan response");

    let incomplete_check = Command::new(env!("CARGO_BIN_EXE_glyph"))
        .arg("check-controller-offline-responses")
        .arg("--prompt-bundle")
        .arg(&bundle_dir)
        .arg("--responses")
        .arg(&responses_dir)
        .output()
        .expect("check incomplete offline responses");
    assert!(!incomplete_check.status.success());
    let incomplete_report: Value =
        serde_json::from_slice(&incomplete_check.stdout).expect("parse incomplete response check");
    assert_eq!(incomplete_report["passed"], json!(false));
    assert_eq!(incomplete_report["expectedResponseFileCount"], json!(3));
    assert_eq!(incomplete_report["presentResponseFileCount"], json!(2));
    assert_eq!(incomplete_report["missingResponseFileCount"], json!(1));
    assert!(
        incomplete_report["missingResponseFiles"]
            .as_array()
            .expect("missing response files")
            .iter()
            .any(|path| path == "cases/constrained/hello_summary_normal_short.direct-prose.txt")
    );

    fs::write(
        response_case_dir.join(format!("{case_id}.direct-prose.txt")),
        "Capture hello world, summarize it, and export the summary.",
    )
    .expect("write direct prose response");

    let complete_check = Command::new(env!("CARGO_BIN_EXE_glyph"))
        .arg("check-controller-offline-responses")
        .arg("--prompt-bundle")
        .arg(&bundle_dir)
        .arg("--responses")
        .arg(&responses_dir)
        .output()
        .expect("check complete offline responses");
    assert!(
        complete_check.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&complete_check.stdout),
        String::from_utf8_lossy(&complete_check.stderr)
    );
    let complete_report: Value =
        serde_json::from_slice(&complete_check.stdout).expect("parse complete response check");
    assert_eq!(complete_report["passed"], json!(true));
    assert_eq!(complete_report["completeResponseSetCount"], json!(1));
    assert_eq!(complete_report["missingResponseFileCount"], json!(0));
    assert_eq!(complete_report["extraResponseFileCount"], json!(0));

    let extra_path = response_case_dir.join(format!("{case_id}.scratch.txt"));
    fs::write(&extra_path, "extra local decoder output").expect("write extra response file");
    let extra_check = Command::new(env!("CARGO_BIN_EXE_glyph"))
        .arg("check-controller-offline-responses")
        .arg("--prompt-bundle")
        .arg(&bundle_dir)
        .arg("--responses")
        .arg(&responses_dir)
        .output()
        .expect("check extra offline responses");
    assert!(!extra_check.status.success());
    let extra_report: Value =
        serde_json::from_slice(&extra_check.stdout).expect("parse extra response check");
    assert_eq!(extra_report["passed"], json!(false));
    assert_eq!(extra_report["extraResponseFileCount"], json!(1));
    assert!(
        extra_report["extraResponseFiles"]
            .as_array()
            .expect("extra response files")
            .iter()
            .any(|path| path == "cases/constrained/hello_summary_normal_short.scratch.txt")
    );
    fs::remove_file(extra_path).expect("remove extra response file before scoring");

    let scored = Command::new(env!("CARGO_BIN_EXE_glyph"))
        .arg("score-controller-responses")
        .arg("--prompt-bundle")
        .arg(&bundle_dir)
        .arg("--responses")
        .arg(&responses_dir)
        .arg("--model-id")
        .arg("local-tiny")
        .arg("--bucket")
        .arg("1b")
        .arg("--jsonl")
        .arg(&jsonl_path)
        .arg("--manifest")
        .arg(&manifest_path)
        .output()
        .expect("score offline responses");
    assert!(
        scored.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&scored.stdout),
        String::from_utf8_lossy(&scored.stderr)
    );
    let report: Value = serde_json::from_slice(&scored.stdout).expect("parse offline report");
    assert_eq!(report["mode"], json!("offline-responses"));
    assert_eq!(report["actualModelCalls"], json!(3));
    assert_eq!(
        report["cases"][0]["adapterMode"],
        json!("offline-responses")
    );
    assert_eq!(report["cases"][0]["parseOk"], json!(true));
    assert_eq!(report["cases"][0]["successfulTrace"], json!(true));

    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).expect("read offline manifest"))
            .expect("parse offline manifest");
    assert_eq!(
        manifest["config"]["adapterMode"],
        json!("offline-responses")
    );
    assert_eq!(
        manifest["config"]["artifacts"]["jsonlPath"],
        json!(jsonl_path.display().to_string())
    );
    assert_eq!(
        manifest["config"]["artifacts"]["promptBundleOverallSha256"]
            .as_str()
            .expect("prompt bundle overall hash")
            .len(),
        64
    );
    assert_eq!(
        manifest["config"]["artifacts"]["promptBundleManifestSha256"]
            .as_str()
            .expect("prompt bundle manifest hash")
            .len(),
        64
    );
    assert_eq!(
        manifest["config"]["artifacts"]["responseBundlePath"],
        json!(responses_dir.display().to_string())
    );
    assert_eq!(
        manifest["config"]["artifacts"]["responseBundleFileCount"],
        json!(3)
    );
    assert!(
        manifest["config"]["artifacts"]["responseBundleBytes"]
            .as_u64()
            .expect("response bundle bytes")
            > 0
    );
    assert_eq!(
        manifest["config"]["artifacts"]["responseBundleSha256"]
            .as_str()
            .expect("response bundle hash")
            .len(),
        64
    );

    let verified = Command::new(env!("CARGO_BIN_EXE_glyph"))
        .arg("verify-controller-run")
        .arg(&jsonl_path)
        .arg(&manifest_path)
        .output()
        .expect("verify offline scored run");
    assert!(
        verified.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verified.stdout),
        String::from_utf8_lossy(&verified.stderr)
    );
    let verification: Value =
        serde_json::from_slice(&verified.stdout).expect("parse offline verification");
    assert_eq!(verification["passed"], json!(true));
    assert_eq!(verification["replay"]["passed"], json!(true));

    let offline_plan = json!({
        "version": "glyph-controller-offline-plan/0.1",
        "totalExpectedRows": 1,
        "shards": [
            {
                "id": "bucket-1b",
                "bucket": "1b",
                "jsonlPath": jsonl_path.display().to_string(),
                "manifestPath": manifest_path.display().to_string(),
                "expectedRows": 1
            }
        ]
    });
    fs::write(
        &offline_plan_path,
        format!("{}\n", serde_json::to_string_pretty(&offline_plan).unwrap()),
    )
    .expect("write offline shard plan");
    let verified_shards = Command::new(env!("CARGO_BIN_EXE_glyph"))
        .arg("verify-controller-shards")
        .arg("--plan")
        .arg(&offline_plan_path)
        .output()
        .expect("verify offline shards");
    assert!(
        verified_shards.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verified_shards.stdout),
        String::from_utf8_lossy(&verified_shards.stderr)
    );
    let shard_report: Value =
        serde_json::from_slice(&verified_shards.stdout).expect("parse offline shard report");
    assert_eq!(shard_report["passed"], json!(true));
    assert_eq!(
        shard_report["planVersion"],
        json!("glyph-controller-offline-plan/0.1")
    );
    assert_eq!(shard_report["verifiedShards"], json!(1));
    assert_eq!(shard_report["shards"][0]["bucket"], json!("1b"));

    let mut tampered_manifest = manifest;
    tampered_manifest["config"]["artifacts"]["promptBundleOverallSha256"] = Value::Null;
    tampered_manifest["config"]["artifacts"]["responseBundleSha256"] = Value::Null;
    fs::write(
        &manifest_path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&tampered_manifest).unwrap()
        ),
    )
    .expect("write tampered offline manifest");
    let rejected = Command::new(env!("CARGO_BIN_EXE_glyph"))
        .arg("verify-controller-run")
        .arg(&jsonl_path)
        .arg(&manifest_path)
        .output()
        .expect("verify tampered offline run");
    assert!(!rejected.status.success());
    let rejected_report: Value =
        serde_json::from_slice(&rejected.stdout).expect("parse rejected offline verification");
    assert!(
        rejected_report["checks"]
            .as_array()
            .expect("verification checks")
            .iter()
            .any(|check| check["id"] == "offline_prompt_bundle_provenance"
                && check["status"] == "fail")
    );
    assert!(
        rejected_report["checks"]
            .as_array()
            .expect("verification checks")
            .iter()
            .any(|check| check["id"] == "offline_response_bundle_provenance"
                && check["status"] == "fail")
    );

    let _ = fs::remove_dir_all(bundle_dir);
    let _ = fs::remove_dir_all(responses_dir);
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
fn example_conformance_covers_public_glyph_programs() {
    let report = glyph_conformance_report();

    assert!(report.passed);
    assert_eq!(report.example_count, 9);
    assert_eq!(report.parse_passed, 9);
    assert_eq!(report.validation_passed, 9);
    assert_eq!(report.run_passed, 9);
    assert!(report.examples.iter().all(|example| {
        example.source_sha256.len() == 64
            && example.source_bytes > 0
            && example.flow_count > 0
            && example.trace_event_count > 0
            && example.final_output_count > 0
            && example.error.is_none()
    }));
    assert!(
        report
            .examples
            .iter()
            .any(|example| example.path == "src/examples/build_crud_app.glyph")
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
