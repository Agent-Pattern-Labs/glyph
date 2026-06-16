use serde::Serialize;

use crate::harness::mock_tools::create_mock_tool_registry;
use crate::ir::glyph_ir::parse_glyph_to_ir;
use crate::ir::validate_ir::validate_ir;
use crate::language::grammar::{
    GLYPH_CONTROLLER_OUTPUT_JSON_SCHEMA, GLYPH_EBNF, GLYPH_GBNF, GLYPH_PRIMITIVES,
};
use crate::language::parser::parse_glyph;
use crate::runtime::glyph_vm::GlyphVm;
use crate::runtime::trace::TraceEvent;

use super::compression::approximate_tokens;
use super::controller_examples::{CONTROLLER_EVAL_CASES, ControllerEvalCase};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ControllerParameterClass {
    #[serde(rename = "1b")]
    OneB,
    #[serde(rename = "3b")]
    ThreeB,
    #[serde(rename = "7b")]
    SevenB,
    Frontier,
}

impl ControllerParameterClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OneB => "1b",
            Self::ThreeB => "3b",
            Self::SevenB => "7b",
            Self::Frontier => "frontier",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ControllerAdapterMode {
    Fixture,
}

#[derive(Debug, Clone)]
pub struct ControllerModelAdapter {
    pub id: String,
    pub parameter_class: ControllerParameterClass,
    pub mode: ControllerAdapterMode,
    pub cost_per_1k_input_tokens_usd: f64,
    pub cost_per_1k_output_tokens_usd: f64,
}

#[derive(Debug, Clone)]
pub struct ControllerGeneration {
    pub glyph: String,
    pub input_tokens: usize,
    pub output_tokens: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerEvalCaseResult {
    #[serde(rename = "caseId")]
    pub case_id: String,
    #[serde(rename = "modelId")]
    pub model_id: String,
    #[serde(rename = "parameterClass")]
    pub parameter_class: ControllerParameterClass,
    #[serde(rename = "adapterMode")]
    pub adapter_mode: ControllerAdapterMode,
    #[serde(rename = "parseOk")]
    pub parse_ok: bool,
    #[serde(rename = "validateOk")]
    pub validate_ok: bool,
    #[serde(rename = "runOk")]
    pub run_ok: bool,
    #[serde(rename = "successfulTrace")]
    pub successful_trace: bool,
    #[serde(rename = "directPlanParseOk")]
    pub direct_plan_parse_ok: bool,
    #[serde(rename = "glyphBeatsDirectPlan")]
    pub glyph_beats_direct_plan: bool,
    #[serde(rename = "expectsRepairLoop")]
    pub expects_repair_loop: bool,
    #[serde(rename = "repairLoopSucceeded")]
    pub repair_loop_succeeded: Option<bool>,
    #[serde(rename = "repairIterations")]
    pub repair_iterations: usize,
    #[serde(rename = "traceEventCount")]
    pub trace_event_count: usize,
    #[serde(rename = "finalOutputCount")]
    pub final_output_count: usize,
    #[serde(rename = "inputTokens")]
    pub input_tokens: usize,
    #[serde(rename = "outputTokens")]
    pub output_tokens: usize,
    #[serde(rename = "estimatedCostUsd")]
    pub estimated_cost_usd: f64,
    #[serde(rename = "directFailureReason")]
    pub direct_failure_reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerEvalModelSummary {
    #[serde(rename = "modelId")]
    pub model_id: String,
    #[serde(rename = "parameterClass")]
    pub parameter_class: ControllerParameterClass,
    #[serde(rename = "adapterMode")]
    pub adapter_mode: ControllerAdapterMode,
    pub cases: usize,
    #[serde(rename = "validProgramRate")]
    pub valid_program_rate: f64,
    #[serde(rename = "runSuccessRate")]
    pub run_success_rate: f64,
    #[serde(rename = "successfulTraceRate")]
    pub successful_trace_rate: f64,
    #[serde(rename = "glyphOverDirectPlanRate")]
    pub glyph_over_direct_plan_rate: f64,
    #[serde(rename = "repairSuccessRate")]
    pub repair_success_rate: Option<f64>,
    #[serde(rename = "averageInputTokens")]
    pub average_input_tokens: f64,
    #[serde(rename = "averageOutputTokens")]
    pub average_output_tokens: f64,
    #[serde(rename = "totalEstimatedCostUsd")]
    pub total_estimated_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerEvalGrammarSummary {
    pub primitives: Vec<String>,
    #[serde(rename = "ebnfChars")]
    pub ebnf_chars: usize,
    #[serde(rename = "gbnfChars")]
    pub gbnf_chars: usize,
    #[serde(rename = "jsonSchemaChars")]
    pub json_schema_chars: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerEvalReport {
    pub mode: ControllerAdapterMode,
    #[serde(rename = "actualModelCalls")]
    pub actual_model_calls: usize,
    pub grammar: ControllerEvalGrammarSummary,
    pub cases: Vec<ControllerEvalCaseResult>,
    #[serde(rename = "byModel")]
    pub by_model: Vec<ControllerEvalModelSummary>,
}

pub fn run_controller_eval() -> ControllerEvalReport {
    let models = create_fixture_controller_models();
    let vm = GlyphVm::new(create_mock_tool_registry());
    let mut results = Vec::new();

    for model in &models {
        for eval_case in CONTROLLER_EVAL_CASES {
            let direct_plan_parse_ok = can_parse_glyph(eval_case.direct_natural_language_plan);
            let generation = generate_fixture(model, eval_case);
            let parse_ok = can_parse_glyph(&generation.glyph);
            let validate_ok = parse_ok && can_validate_glyph(&generation.glyph);
            let mut trace = Vec::new();
            let mut final_output_count = 0usize;
            let mut run_ok = false;
            let mut error = None;

            if validate_ok {
                match vm.run_source(&generation.glyph) {
                    Ok(run) => {
                        trace = run.trace;
                        final_output_count = run.outputs.len();
                        run_ok = true;
                    }
                    Err(err) => error = Some(err.to_string()),
                }
            }

            let successful_trace = run_ok && !trace.is_empty() && final_output_count > 0;
            let repair_loop_succeeded = if eval_case.expects_repair_loop {
                Some(has_successful_repair_loop(&trace))
            } else {
                None
            };

            results.push(ControllerEvalCaseResult {
                case_id: eval_case.id.to_string(),
                model_id: model.id.clone(),
                parameter_class: model.parameter_class,
                adapter_mode: model.mode.clone(),
                parse_ok,
                validate_ok,
                run_ok,
                successful_trace,
                direct_plan_parse_ok,
                glyph_beats_direct_plan: !direct_plan_parse_ok && successful_trace,
                expects_repair_loop: eval_case.expects_repair_loop,
                repair_loop_succeeded,
                repair_iterations: count_repair_iterations(&trace),
                trace_event_count: trace.len(),
                final_output_count,
                input_tokens: generation.input_tokens,
                output_tokens: generation.output_tokens,
                estimated_cost_usd: estimate_cost(
                    generation.input_tokens,
                    generation.output_tokens,
                    model.cost_per_1k_input_tokens_usd,
                    model.cost_per_1k_output_tokens_usd,
                ),
                direct_failure_reason: eval_case.direct_failure_reason.to_string(),
                error,
            });
        }
    }

    ControllerEvalReport {
        mode: ControllerAdapterMode::Fixture,
        actual_model_calls: 0,
        grammar: ControllerEvalGrammarSummary {
            primitives: GLYPH_PRIMITIVES
                .iter()
                .map(|value| value.to_string())
                .collect(),
            ebnf_chars: GLYPH_EBNF.len(),
            gbnf_chars: GLYPH_GBNF.len(),
            json_schema_chars: GLYPH_CONTROLLER_OUTPUT_JSON_SCHEMA.len(),
        },
        by_model: summarize_by_model(&results),
        cases: results,
    }
}

pub fn create_fixture_controller_models() -> Vec<ControllerModelAdapter> {
    vec![
        fixture_model("fixture-1b-constrained", ControllerParameterClass::OneB),
        fixture_model("fixture-3b-constrained", ControllerParameterClass::ThreeB),
        fixture_model("fixture-7b-constrained", ControllerParameterClass::SevenB),
        fixture_model(
            "fixture-frontier-constrained",
            ControllerParameterClass::Frontier,
        ),
    ]
}

fn fixture_model(id: &str, parameter_class: ControllerParameterClass) -> ControllerModelAdapter {
    ControllerModelAdapter {
        id: id.to_string(),
        parameter_class,
        mode: ControllerAdapterMode::Fixture,
        cost_per_1k_input_tokens_usd: 0.0,
        cost_per_1k_output_tokens_usd: 0.0,
    }
}

fn generate_fixture(
    _model: &ControllerModelAdapter,
    eval_case: &ControllerEvalCase,
) -> ControllerGeneration {
    ControllerGeneration {
        glyph: eval_case.expected_glyph.to_string(),
        input_tokens: approximate_tokens(eval_case.request),
        output_tokens: approximate_tokens(eval_case.expected_glyph),
    }
}

fn can_parse_glyph(source: &str) -> bool {
    parse_glyph(source).is_ok()
}

fn can_validate_glyph(source: &str) -> bool {
    parse_glyph_to_ir(source)
        .and_then(|ir| {
            validate_ir(ir).map_err(|err| {
                crate::language::errors::GlyphSyntaxError::new(err.to_string(), 1, 1)
            })
        })
        .is_ok()
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

fn estimate_cost(
    input_tokens: usize,
    output_tokens: usize,
    input_rate: f64,
    output_rate: f64,
) -> f64 {
    (input_tokens as f64 / 1000.0) * input_rate + (output_tokens as f64 / 1000.0) * output_rate
}

fn summarize_by_model(results: &[ControllerEvalCaseResult]) -> Vec<ControllerEvalModelSummary> {
    let mut model_ids = Vec::<String>::new();
    for result in results {
        if !model_ids.contains(&result.model_id) {
            model_ids.push(result.model_id.clone());
        }
    }

    model_ids
        .into_iter()
        .map(|model_id| {
            let model_results = results
                .iter()
                .filter(|result| result.model_id == model_id)
                .collect::<Vec<_>>();
            let first = model_results[0];
            let repair_results = model_results
                .iter()
                .copied()
                .filter(|result| result.expects_repair_loop)
                .collect::<Vec<_>>();

            ControllerEvalModelSummary {
                model_id,
                parameter_class: first.parameter_class,
                adapter_mode: first.adapter_mode.clone(),
                cases: model_results.len(),
                valid_program_rate: rate(&model_results, |result| {
                    result.parse_ok && result.validate_ok
                }),
                run_success_rate: rate(&model_results, |result| result.run_ok),
                successful_trace_rate: rate(&model_results, |result| result.successful_trace),
                glyph_over_direct_plan_rate: rate(&model_results, |result| {
                    result.glyph_beats_direct_plan
                }),
                repair_success_rate: if repair_results.is_empty() {
                    None
                } else {
                    Some(rate(&repair_results, |result| {
                        result.repair_loop_succeeded == Some(true)
                    }))
                },
                average_input_tokens: average(
                    &model_results
                        .iter()
                        .map(|result| result.input_tokens as f64)
                        .collect::<Vec<_>>(),
                ),
                average_output_tokens: average(
                    &model_results
                        .iter()
                        .map(|result| result.output_tokens as f64)
                        .collect::<Vec<_>>(),
                ),
                total_estimated_cost_usd: model_results
                    .iter()
                    .map(|result| result.estimated_cost_usd)
                    .sum(),
            }
        })
        .collect()
}

fn rate<T>(items: &[T], predicate: impl Fn(&T) -> bool) -> f64 {
    if items.is_empty() {
        return 0.0;
    }

    items.iter().filter(|item| predicate(item)).count() as f64 / items.len() as f64
}

fn average(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }

    values.iter().sum::<f64>() / values.len() as f64
}
