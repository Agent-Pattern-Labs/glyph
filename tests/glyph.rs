use glyph::eval::compression::compare_compression;
use glyph::eval::controller::run_controller_eval;
use glyph::eval::examples::CompressionExample;
use glyph::harness::mock_tools::create_mock_tool_registry;
use glyph::ir::glyph_ir::parse_glyph_to_ir;
use glyph::ir::validate_ir::validate_ir;
use glyph::language::grammar::{GLYPH_CONTROLLER_OUTPUT_JSON_SCHEMA, GLYPH_EBNF, GLYPH_GBNF};
use glyph::language::parser::parse_glyph;
use glyph::runtime::glyph_vm::GlyphVm;
use glyph::runtime::trace::TraceEvent;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use std::fs;

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
    assert!(report.by_model.iter().all(|summary| {
        summary.valid_program_rate == 1.0
            && summary.run_success_rate == 1.0
            && summary.successful_trace_rate == 1.0
            && summary.glyph_over_direct_plan_rate == 1.0
            && summary.repair_success_rate == Some(1.0)
    }));
}

#[test]
fn spec_artifacts_match_reference_constants() {
    assert_eq!(fs::read_to_string("spec/glyph.ebnf").unwrap(), GLYPH_EBNF);
    assert_eq!(fs::read_to_string("spec/glyph.gbnf").unwrap(), GLYPH_GBNF);
    assert_eq!(
        fs::read_to_string("spec/controller-output.schema.json").unwrap(),
        GLYPH_CONTROLLER_OUTPUT_JSON_SCHEMA
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
