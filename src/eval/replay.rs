use serde::Serialize;
use serde_json::Value;

use crate::harness::mock_tools::create_mock_tool_registry;
use crate::ir::validate_ir::validate_ir;
use crate::language::parser::parse_glyph;
use crate::runtime::glyph_vm::{GlyphVm, GlyphVmOptions};
use crate::runtime::trace::TraceEvent;

use super::controller::{
    ControllerEvalCaseResult, extract_glyph_from_model_output,
    extract_json_tool_plan_from_model_output, json_tool_plan_to_ir,
};

const MAX_REPLAY_FAILURES: usize = 100;

#[derive(Debug, Clone, Serialize)]
pub struct ControllerReplayReport {
    pub passed: bool,
    #[serde(rename = "caseRows")]
    pub case_rows: usize,
    #[serde(rename = "checkedRows")]
    pub checked_rows: usize,
    #[serde(rename = "failureCount")]
    pub failure_count: usize,
    #[serde(rename = "truncatedFailures")]
    pub truncated_failures: bool,
    pub failures: Vec<ControllerReplayFailure>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerReplayFailure {
    #[serde(rename = "caseId")]
    pub case_id: String,
    #[serde(rename = "modelId")]
    pub model_id: String,
    #[serde(rename = "parameterClass")]
    pub parameter_class: String,
    #[serde(rename = "promptMode")]
    pub prompt_mode: String,
    pub field: String,
    pub recorded: String,
    pub replayed: String,
}

#[derive(Debug, Clone)]
struct GlyphReplayOutcome {
    parse_ok: bool,
    validate_ok: bool,
    run_ok: bool,
    successful_trace: bool,
    trace_event_count: usize,
    final_output_count: usize,
    repair_loop_succeeded: Option<bool>,
    repair_iterations: usize,
}

#[derive(Debug, Clone)]
struct JsonToolPlanReplayOutcome {
    parse_ok: bool,
    run_ok: bool,
    successful_trace: bool,
    trace_event_count: usize,
    final_output_count: usize,
}

pub fn replay_controller_run(cases: &[ControllerEvalCaseResult]) -> ControllerReplayReport {
    let vm = GlyphVm::new(create_mock_tool_registry());
    let mut failures = Vec::new();
    let mut failure_count = 0usize;

    for case in cases {
        compare_string(
            case,
            "generatedGlyph",
            &case.generated_glyph,
            &extract_glyph_from_model_output(&case.raw_output),
            &mut failures,
            &mut failure_count,
        );
        let glyph = replay_glyph_source(&vm, &case.generated_glyph, case.expects_repair_loop);
        compare_bool(
            case,
            "parseOk",
            case.parse_ok,
            glyph.parse_ok,
            &mut failures,
            &mut failure_count,
        );
        compare_bool(
            case,
            "validateOk",
            case.validate_ok,
            glyph.validate_ok,
            &mut failures,
            &mut failure_count,
        );
        compare_bool(
            case,
            "runOk",
            case.run_ok,
            glyph.run_ok,
            &mut failures,
            &mut failure_count,
        );
        compare_bool(
            case,
            "successfulTrace",
            case.successful_trace,
            glyph.successful_trace,
            &mut failures,
            &mut failure_count,
        );
        compare_usize(
            case,
            "traceEventCount",
            case.trace_event_count,
            glyph.trace_event_count,
            &mut failures,
            &mut failure_count,
        );
        compare_usize(
            case,
            "finalOutputCount",
            case.final_output_count,
            glyph.final_output_count,
            &mut failures,
            &mut failure_count,
        );
        compare_option_bool(
            case,
            "repairLoopSucceeded",
            case.repair_loop_succeeded,
            glyph.repair_loop_succeeded,
            &mut failures,
            &mut failure_count,
        );
        compare_usize(
            case,
            "repairIterations",
            case.repair_iterations,
            glyph.repair_iterations,
            &mut failures,
            &mut failure_count,
        );

        compare_string(
            case,
            "generatedJsonToolPlan",
            &case.generated_json_tool_plan,
            &extract_json_tool_plan_from_model_output(&case.json_tool_plan_raw_output),
            &mut failures,
            &mut failure_count,
        );
        let json_tool_plan = replay_json_tool_plan(&vm, &case.generated_json_tool_plan);
        compare_bool(
            case,
            "jsonToolPlanParseOk",
            case.json_tool_plan_parse_ok,
            json_tool_plan.parse_ok,
            &mut failures,
            &mut failure_count,
        );
        compare_bool(
            case,
            "jsonToolPlanRunOk",
            case.json_tool_plan_run_ok,
            json_tool_plan.run_ok,
            &mut failures,
            &mut failure_count,
        );
        compare_bool(
            case,
            "jsonToolPlanSuccessfulTrace",
            case.json_tool_plan_successful_trace,
            json_tool_plan.successful_trace,
            &mut failures,
            &mut failure_count,
        );
        compare_usize(
            case,
            "jsonToolPlanTraceEventCount",
            case.json_tool_plan_trace_event_count,
            json_tool_plan.trace_event_count,
            &mut failures,
            &mut failure_count,
        );
        compare_usize(
            case,
            "jsonToolPlanFinalOutputCount",
            case.json_tool_plan_final_output_count,
            json_tool_plan.final_output_count,
            &mut failures,
            &mut failure_count,
        );
        compare_bool(
            case,
            "glyphBeatsJsonToolPlan",
            case.glyph_beats_json_tool_plan,
            glyph.successful_trace && !json_tool_plan.successful_trace,
            &mut failures,
            &mut failure_count,
        );

        compare_string(
            case,
            "generatedDirectProse",
            &case.generated_direct_prose,
            &case.direct_prose_raw_output,
            &mut failures,
            &mut failure_count,
        );
        let direct_prose = replay_glyph_source(&vm, &case.generated_direct_prose, false);
        compare_bool(
            case,
            "directProseParseOk",
            case.direct_prose_parse_ok,
            direct_prose.parse_ok,
            &mut failures,
            &mut failure_count,
        );
        compare_bool(
            case,
            "directProseValidateOk",
            case.direct_prose_validate_ok,
            direct_prose.validate_ok,
            &mut failures,
            &mut failure_count,
        );
        compare_bool(
            case,
            "directProseRunOk",
            case.direct_prose_run_ok,
            direct_prose.run_ok,
            &mut failures,
            &mut failure_count,
        );
        compare_bool(
            case,
            "directProseSuccessfulTrace",
            case.direct_prose_successful_trace,
            direct_prose.successful_trace,
            &mut failures,
            &mut failure_count,
        );
        compare_usize(
            case,
            "directProseTraceEventCount",
            case.direct_prose_trace_event_count,
            direct_prose.trace_event_count,
            &mut failures,
            &mut failure_count,
        );
        compare_usize(
            case,
            "directProseFinalOutputCount",
            case.direct_prose_final_output_count,
            direct_prose.final_output_count,
            &mut failures,
            &mut failure_count,
        );
        compare_bool(
            case,
            "glyphBeatsDirectProse",
            case.glyph_beats_direct_prose,
            glyph.successful_trace && !direct_prose.successful_trace,
            &mut failures,
            &mut failure_count,
        );
    }

    ControllerReplayReport {
        passed: failure_count == 0,
        case_rows: cases.len(),
        checked_rows: cases.len(),
        failure_count,
        truncated_failures: failure_count > failures.len(),
        failures,
    }
}

fn replay_glyph_source(
    vm: &GlyphVm,
    source: &str,
    expects_repair_loop: bool,
) -> GlyphReplayOutcome {
    let parse_ok = parse_glyph(source).is_ok();
    let validate_ok = parse_ok
        && crate::ir::glyph_ir::parse_glyph_to_ir(source)
            .ok()
            .and_then(|ir| validate_ir(ir).ok())
            .is_some();
    let mut trace = Vec::new();
    let mut final_output_count = 0usize;
    let mut run_ok = false;

    if validate_ok {
        if let Ok(run) = vm.run_source(source) {
            trace = run.trace;
            final_output_count = run.outputs.len();
            run_ok = true;
        }
    }

    let successful_trace = run_ok && !trace.is_empty() && final_output_count > 0;
    GlyphReplayOutcome {
        parse_ok,
        validate_ok,
        run_ok,
        successful_trace,
        trace_event_count: trace.len(),
        final_output_count,
        repair_loop_succeeded: expects_repair_loop.then(|| has_successful_repair_loop(&trace)),
        repair_iterations: count_repair_iterations(&trace),
    }
}

fn replay_json_tool_plan(vm: &GlyphVm, source: &str) -> JsonToolPlanReplayOutcome {
    let parsed = serde_json::from_str::<Value>(source)
        .map_err(|error| format!("Invalid JSON tool plan: {error}"))
        .and_then(|value| json_tool_plan_to_ir(&value))
        .and_then(|ir| validate_ir(ir).map_err(|error| error.to_string()));

    let parse_ok = parsed.is_ok();
    let mut trace_event_count = 0usize;
    let mut final_output_count = 0usize;
    let mut run_ok = false;

    if let Ok(ir) = parsed {
        if let Ok(result) = vm.execute(ir, GlyphVmOptions::default()) {
            trace_event_count = result.trace.len();
            final_output_count = result.outputs.len();
            run_ok = true;
        }
    }

    JsonToolPlanReplayOutcome {
        parse_ok,
        run_ok,
        successful_trace: run_ok && trace_event_count > 0 && final_output_count > 0,
        trace_event_count,
        final_output_count,
    }
}

fn has_successful_repair_loop(trace: &[TraceEvent]) -> bool {
    trace
        .iter()
        .any(|event| event.operation == "REPAIR" && event.status.as_str() == "pass")
}

fn count_repair_iterations(trace: &[TraceEvent]) -> usize {
    trace
        .iter()
        .filter_map(|event| event.iteration)
        .max()
        .unwrap_or(0)
}

fn compare_bool(
    case: &ControllerEvalCaseResult,
    field: &str,
    recorded: bool,
    replayed: bool,
    failures: &mut Vec<ControllerReplayFailure>,
    failure_count: &mut usize,
) {
    if recorded != replayed {
        push_failure(
            case,
            field,
            recorded.to_string(),
            replayed.to_string(),
            failures,
            failure_count,
        );
    }
}

fn compare_option_bool(
    case: &ControllerEvalCaseResult,
    field: &str,
    recorded: Option<bool>,
    replayed: Option<bool>,
    failures: &mut Vec<ControllerReplayFailure>,
    failure_count: &mut usize,
) {
    if recorded != replayed {
        push_failure(
            case,
            field,
            format!("{recorded:?}"),
            format!("{replayed:?}"),
            failures,
            failure_count,
        );
    }
}

fn compare_usize(
    case: &ControllerEvalCaseResult,
    field: &str,
    recorded: usize,
    replayed: usize,
    failures: &mut Vec<ControllerReplayFailure>,
    failure_count: &mut usize,
) {
    if recorded != replayed {
        push_failure(
            case,
            field,
            recorded.to_string(),
            replayed.to_string(),
            failures,
            failure_count,
        );
    }
}

fn compare_string(
    case: &ControllerEvalCaseResult,
    field: &str,
    recorded: &str,
    replayed: &str,
    failures: &mut Vec<ControllerReplayFailure>,
    failure_count: &mut usize,
) {
    if recorded != replayed {
        push_failure(
            case,
            field,
            summarize_text(recorded),
            summarize_text(replayed),
            failures,
            failure_count,
        );
    }
}

fn summarize_text(value: &str) -> String {
    let mut summary = value.chars().take(120).collect::<String>();
    if value.chars().count() > 120 {
        summary.push_str("...");
    }
    summary
}

fn push_failure(
    case: &ControllerEvalCaseResult,
    field: &str,
    recorded: String,
    replayed: String,
    failures: &mut Vec<ControllerReplayFailure>,
    failure_count: &mut usize,
) {
    *failure_count += 1;
    if failures.len() >= MAX_REPLAY_FAILURES {
        return;
    }

    failures.push(ControllerReplayFailure {
        case_id: case.case_id.clone(),
        model_id: case.model_id.clone(),
        parameter_class: case.parameter_class.as_str().to_string(),
        prompt_mode: case.prompt_mode.as_str().to_string(),
        field: field.to_string(),
        recorded,
        replayed,
    });
}
