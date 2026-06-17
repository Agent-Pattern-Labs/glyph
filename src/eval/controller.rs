use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::harness::mock_tools::create_mock_tool_registry;
use crate::ir::glyph_ir::{
    GLYPH_IR_VERSION, GlyphIr, GlyphIrFlow, GlyphIrStep, GlyphRepairStep, GlyphToolStep,
    parse_glyph_to_ir,
};
use crate::ir::validate_ir::validate_ir;
use crate::language::grammar::{
    GLYPH_CONTROLLER_OUTPUT_JSON_SCHEMA, GLYPH_EBNF, GLYPH_GBNF, GLYPH_PRIMITIVES,
};
use crate::language::parser::parse_glyph;
use crate::runtime::glyph_vm::{GlyphVm, GlyphVmOptions};
use crate::runtime::trace::TraceEvent;

use super::compression::approximate_tokens;
use super::controller_examples::{ControllerEvalCase, controller_eval_cases};

pub const GENERIC_TOOL_PLAN_JSON_SCHEMA: &str =
    include_str!("../../spec/generic-tool-plan.schema.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControllerAdapterMode {
    #[serde(rename = "fixture")]
    Fixture,
    #[serde(rename = "openai-compatible")]
    OpenAiCompatible,
    #[serde(rename = "offline-responses")]
    OfflineResponses,
    #[serde(rename = "mixed")]
    Mixed,
}

impl ControllerAdapterMode {
    pub fn is_live_evidence(&self) -> bool {
        matches!(
            self,
            ControllerAdapterMode::OpenAiCompatible | ControllerAdapterMode::OfflineResponses
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ControllerPromptMode {
    Constrained,
    SchemaOnly,
    Plain,
}

impl ControllerPromptMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Constrained => "constrained",
            Self::SchemaOnly => "schema-only",
            Self::Plain => "plain",
        }
    }

    pub fn all() -> Vec<Self> {
        vec![Self::Constrained, Self::SchemaOnly, Self::Plain]
    }

    fn expects_json_output(self) -> bool {
        matches!(self, Self::Constrained | Self::SchemaOnly)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ControllerGrammarPayload {
    None,
    Gbnf,
}

impl ControllerGrammarPayload {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Gbnf => "gbnf",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ControllerRequestKind {
    Glyph,
    JsonToolPlan,
    DirectProse,
}

impl ControllerRequestKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Glyph => "glyph",
            Self::JsonToolPlan => "json-tool-plan",
            Self::DirectProse => "direct-prose",
        }
    }
}

#[derive(Debug, Clone)]
enum ControllerModelSource {
    Fixture,
    OpenAiCompatible {
        endpoint: String,
        api_key: Option<String>,
    },
    OfflineResponses {
        responses: BTreeMap<ControllerOfflineResponseKey, ControllerOfflineResponseSet>,
    },
}

#[derive(Debug, Clone)]
pub struct ControllerModelAdapter {
    pub id: String,
    pub parameter_class: ControllerParameterClass,
    pub mode: ControllerAdapterMode,
    pub grammar_payload: ControllerGrammarPayload,
    pub cost_per_1k_input_tokens_usd: f64,
    pub cost_per_1k_output_tokens_usd: f64,
    source: ControllerModelSource,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ControllerOfflineResponseKey {
    pub case_id: String,
    pub prompt_mode: ControllerPromptMode,
}

#[derive(Debug, Clone)]
pub struct ControllerOfflineResponseSet {
    pub glyph: String,
    pub json_tool_plan: String,
    pub direct_prose: String,
}

#[derive(Debug, Clone)]
pub struct ControllerGeneration {
    pub glyph: String,
    pub raw_output: String,
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub duration_ms: u128,
}

#[derive(Debug, Clone)]
struct JsonToolPlanEvaluation {
    parse_ok: bool,
    run_ok: bool,
    successful_trace: bool,
    trace_event_count: usize,
    final_output_count: usize,
    input_tokens: usize,
    output_tokens: usize,
    duration_ms: u128,
    generated_plan: String,
    raw_output: String,
    parse_error: Option<String>,
    run_error: Option<String>,
    generation_error: Option<String>,
}

#[derive(Debug, Clone)]
struct DirectProseEvaluation {
    attempted: bool,
    parse_ok: bool,
    validate_ok: bool,
    run_ok: bool,
    successful_trace: bool,
    trace_event_count: usize,
    final_output_count: usize,
    input_tokens: usize,
    output_tokens: usize,
    duration_ms: u128,
    generated_prose: String,
    raw_output: String,
    parse_error: Option<String>,
    validation_error: Option<String>,
    run_error: Option<String>,
    generation_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControllerEvalCaseResult {
    #[serde(rename = "caseId")]
    pub case_id: String,
    pub tags: Vec<String>,
    #[serde(rename = "modelId")]
    pub model_id: String,
    #[serde(rename = "parameterClass")]
    pub parameter_class: ControllerParameterClass,
    #[serde(rename = "adapterMode")]
    pub adapter_mode: ControllerAdapterMode,
    #[serde(rename = "promptMode")]
    pub prompt_mode: ControllerPromptMode,
    #[serde(rename = "grammarPayload")]
    pub grammar_payload: ControllerGrammarPayload,
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
    #[serde(rename = "jsonToolPlanParseOk")]
    pub json_tool_plan_parse_ok: bool,
    #[serde(rename = "jsonToolPlanRunOk")]
    pub json_tool_plan_run_ok: bool,
    #[serde(rename = "jsonToolPlanSuccessfulTrace")]
    pub json_tool_plan_successful_trace: bool,
    #[serde(rename = "glyphBeatsJsonToolPlan")]
    pub glyph_beats_json_tool_plan: bool,
    #[serde(rename = "directProseAttempted", default)]
    pub direct_prose_attempted: bool,
    #[serde(rename = "directProseParseOk", default)]
    pub direct_prose_parse_ok: bool,
    #[serde(rename = "directProseValidateOk", default)]
    pub direct_prose_validate_ok: bool,
    #[serde(rename = "directProseRunOk", default)]
    pub direct_prose_run_ok: bool,
    #[serde(rename = "directProseSuccessfulTrace", default)]
    pub direct_prose_successful_trace: bool,
    #[serde(rename = "glyphBeatsDirectProse", default)]
    pub glyph_beats_direct_prose: bool,
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
    #[serde(rename = "jsonToolPlanTraceEventCount")]
    pub json_tool_plan_trace_event_count: usize,
    #[serde(rename = "jsonToolPlanFinalOutputCount")]
    pub json_tool_plan_final_output_count: usize,
    #[serde(rename = "directProseTraceEventCount", default)]
    pub direct_prose_trace_event_count: usize,
    #[serde(rename = "directProseFinalOutputCount", default)]
    pub direct_prose_final_output_count: usize,
    #[serde(rename = "inputTokens")]
    pub input_tokens: usize,
    #[serde(rename = "outputTokens")]
    pub output_tokens: usize,
    #[serde(rename = "jsonToolPlanInputTokens")]
    pub json_tool_plan_input_tokens: usize,
    #[serde(rename = "jsonToolPlanOutputTokens")]
    pub json_tool_plan_output_tokens: usize,
    #[serde(rename = "directProseInputTokens", default)]
    pub direct_prose_input_tokens: usize,
    #[serde(rename = "directProseOutputTokens", default)]
    pub direct_prose_output_tokens: usize,
    #[serde(rename = "estimatedCostUsd")]
    pub estimated_cost_usd: f64,
    #[serde(rename = "jsonToolPlanEstimatedCostUsd")]
    pub json_tool_plan_estimated_cost_usd: f64,
    #[serde(rename = "directProseEstimatedCostUsd", default)]
    pub direct_prose_estimated_cost_usd: f64,
    #[serde(rename = "durationMs")]
    pub duration_ms: u128,
    #[serde(rename = "jsonToolPlanDurationMs")]
    pub json_tool_plan_duration_ms: u128,
    #[serde(rename = "directProseDurationMs", default)]
    pub direct_prose_duration_ms: u128,
    #[serde(rename = "generatedGlyph")]
    pub generated_glyph: String,
    #[serde(rename = "rawOutput")]
    pub raw_output: String,
    #[serde(rename = "generatedJsonToolPlan")]
    pub generated_json_tool_plan: String,
    #[serde(rename = "jsonToolPlanRawOutput")]
    pub json_tool_plan_raw_output: String,
    #[serde(rename = "generatedDirectProse", default)]
    pub generated_direct_prose: String,
    #[serde(rename = "directProseRawOutput", default)]
    pub direct_prose_raw_output: String,
    #[serde(rename = "directFailureReason")]
    pub direct_failure_reason: String,
    #[serde(rename = "parseError", skip_serializing_if = "Option::is_none")]
    pub parse_error: Option<String>,
    #[serde(rename = "validationError", skip_serializing_if = "Option::is_none")]
    pub validation_error: Option<String>,
    #[serde(rename = "runError", skip_serializing_if = "Option::is_none")]
    pub run_error: Option<String>,
    #[serde(
        rename = "jsonToolPlanParseError",
        skip_serializing_if = "Option::is_none"
    )]
    pub json_tool_plan_parse_error: Option<String>,
    #[serde(
        rename = "jsonToolPlanRunError",
        skip_serializing_if = "Option::is_none"
    )]
    pub json_tool_plan_run_error: Option<String>,
    #[serde(rename = "jsonToolPlanError", skip_serializing_if = "Option::is_none")]
    pub json_tool_plan_error: Option<String>,
    #[serde(
        rename = "directProseParseError",
        skip_serializing_if = "Option::is_none"
    )]
    pub direct_prose_parse_error: Option<String>,
    #[serde(
        rename = "directProseValidationError",
        skip_serializing_if = "Option::is_none"
    )]
    pub direct_prose_validation_error: Option<String>,
    #[serde(
        rename = "directProseRunError",
        skip_serializing_if = "Option::is_none"
    )]
    pub direct_prose_run_error: Option<String>,
    #[serde(rename = "directProseError", skip_serializing_if = "Option::is_none")]
    pub direct_prose_error: Option<String>,
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
    #[serde(rename = "promptMode")]
    pub prompt_mode: ControllerPromptMode,
    #[serde(rename = "grammarPayload")]
    pub grammar_payload: ControllerGrammarPayload,
    pub cases: usize,
    #[serde(rename = "validProgramRate")]
    pub valid_program_rate: f64,
    #[serde(rename = "runSuccessRate")]
    pub run_success_rate: f64,
    #[serde(rename = "successfulTraceRate")]
    pub successful_trace_rate: f64,
    #[serde(rename = "glyphOverDirectPlanRate")]
    pub glyph_over_direct_plan_rate: f64,
    #[serde(rename = "jsonToolPlanRunSuccessRate")]
    pub json_tool_plan_run_success_rate: f64,
    #[serde(rename = "jsonToolPlanSuccessfulTraceRate")]
    pub json_tool_plan_successful_trace_rate: f64,
    #[serde(rename = "glyphOverJsonToolPlanRate")]
    pub glyph_over_json_tool_plan_rate: f64,
    #[serde(rename = "directProseSuccessfulTraceRate")]
    pub direct_prose_successful_trace_rate: f64,
    #[serde(rename = "glyphOverDirectProseRate")]
    pub glyph_over_direct_prose_rate: f64,
    #[serde(rename = "repairSuccessRate")]
    pub repair_success_rate: Option<f64>,
    #[serde(rename = "averageInputTokens")]
    pub average_input_tokens: f64,
    #[serde(rename = "averageOutputTokens")]
    pub average_output_tokens: f64,
    #[serde(rename = "averageJsonToolPlanOutputTokens")]
    pub average_json_tool_plan_output_tokens: f64,
    #[serde(rename = "averageDirectProseOutputTokens")]
    pub average_direct_prose_output_tokens: f64,
    #[serde(rename = "totalEstimatedCostUsd")]
    pub total_estimated_cost_usd: f64,
    #[serde(rename = "totalJsonToolPlanEstimatedCostUsd")]
    pub total_json_tool_plan_estimated_cost_usd: f64,
    #[serde(rename = "totalDirectProseEstimatedCostUsd")]
    pub total_direct_prose_estimated_cost_usd: f64,
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

#[derive(Debug, Clone)]
pub struct ControllerEvalOptions {
    pub models: Option<Vec<ControllerModelAdapter>>,
    pub prompt_modes: Vec<ControllerPromptMode>,
    pub case_filter: ControllerEvalCaseFilter,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ControllerEvalCaseFilter {
    pub case_ids: Vec<String>,
    pub tags: Vec<String>,
    pub families: Vec<String>,
    pub profiles: Vec<String>,
    pub limit: Option<usize>,
}

impl Default for ControllerEvalOptions {
    fn default() -> Self {
        Self {
            models: None,
            prompt_modes: vec![ControllerPromptMode::Constrained],
            case_filter: ControllerEvalCaseFilter::default(),
        }
    }
}

pub fn run_controller_eval() -> ControllerEvalReport {
    run_controller_eval_with_options(ControllerEvalOptions::default())
}

pub fn run_controller_eval_with_options(options: ControllerEvalOptions) -> ControllerEvalReport {
    match run_controller_eval_with_observer(options, |_| Ok::<(), std::convert::Infallible>(())) {
        Ok(report) => report,
        Err(error) => match error {},
    }
}

pub fn summarize_controller_eval_by_model(
    results: &[ControllerEvalCaseResult],
) -> Vec<ControllerEvalModelSummary> {
    summarize_by_model(results)
}

pub fn run_controller_eval_with_observer<E>(
    options: ControllerEvalOptions,
    mut observe_case: impl FnMut(&ControllerEvalCaseResult) -> Result<(), E>,
) -> Result<ControllerEvalReport, E> {
    let models = create_fixture_controller_models();
    let models = options.models.unwrap_or(models);
    let prompt_modes = if options.prompt_modes.is_empty() {
        vec![ControllerPromptMode::Constrained]
    } else {
        options.prompt_modes
    };
    let cases = select_controller_eval_cases(&options.case_filter);
    let vm = GlyphVm::new(create_mock_tool_registry());
    let mut results = Vec::new();

    for model in &models {
        for prompt_mode in &prompt_modes {
            for eval_case in &cases {
                let direct_plan_parse_ok = can_parse_glyph(&eval_case.direct_natural_language_plan);
                let generation = generate_with_model(model, eval_case, *prompt_mode);
                let mut generation_error = None;
                let generation = match generation {
                    Ok(generation) => generation,
                    Err(error) => {
                        generation_error = Some(error);
                        ControllerGeneration {
                            glyph: String::new(),
                            raw_output: String::new(),
                            input_tokens: approximate_tokens(
                                &build_controller_prompt_with_payload(
                                    eval_case,
                                    *prompt_mode,
                                    model.grammar_payload,
                                ),
                            ),
                            output_tokens: 0,
                            duration_ms: 0,
                        }
                    }
                };
                let parse_error = parse_error(&generation.glyph);
                let parse_ok = parse_error.is_none();
                let validation_error = if parse_ok {
                    validation_error(&generation.glyph)
                } else {
                    None
                };
                let validate_ok = parse_ok && validation_error.is_none();
                let mut trace = Vec::new();
                let mut final_output_count = 0usize;
                let mut run_ok = false;
                let mut run_error = None;

                if validate_ok {
                    match vm.run_source(&generation.glyph) {
                        Ok(run) => {
                            trace = run.trace;
                            final_output_count = run.outputs.len();
                            run_ok = true;
                        }
                        Err(err) => run_error = Some(err.to_string()),
                    }
                }

                let successful_trace = run_ok && !trace.is_empty() && final_output_count > 0;
                let repair_loop_succeeded = if eval_case.expects_repair_loop {
                    Some(has_successful_repair_loop(&trace))
                } else {
                    None
                };
                let json_tool_plan =
                    evaluate_json_tool_plan_baseline(model, eval_case, *prompt_mode, &vm);
                let direct_prose =
                    evaluate_direct_prose_baseline(model, eval_case, *prompt_mode, &vm);

                let result = ControllerEvalCaseResult {
                    case_id: eval_case.id.to_string(),
                    tags: eval_case.tags.clone(),
                    model_id: model.id.clone(),
                    parameter_class: model.parameter_class,
                    adapter_mode: model.mode.clone(),
                    prompt_mode: *prompt_mode,
                    grammar_payload: model.grammar_payload,
                    parse_ok,
                    validate_ok,
                    run_ok,
                    successful_trace,
                    direct_plan_parse_ok,
                    glyph_beats_direct_plan: !direct_plan_parse_ok && successful_trace,
                    json_tool_plan_parse_ok: json_tool_plan.parse_ok,
                    json_tool_plan_run_ok: json_tool_plan.run_ok,
                    json_tool_plan_successful_trace: json_tool_plan.successful_trace,
                    glyph_beats_json_tool_plan: successful_trace
                        && !json_tool_plan.successful_trace,
                    direct_prose_attempted: direct_prose.attempted,
                    direct_prose_parse_ok: direct_prose.parse_ok,
                    direct_prose_validate_ok: direct_prose.validate_ok,
                    direct_prose_run_ok: direct_prose.run_ok,
                    direct_prose_successful_trace: direct_prose.successful_trace,
                    glyph_beats_direct_prose: successful_trace && !direct_prose.successful_trace,
                    expects_repair_loop: eval_case.expects_repair_loop,
                    repair_loop_succeeded,
                    repair_iterations: count_repair_iterations(&trace),
                    trace_event_count: trace.len(),
                    final_output_count,
                    json_tool_plan_trace_event_count: json_tool_plan.trace_event_count,
                    json_tool_plan_final_output_count: json_tool_plan.final_output_count,
                    direct_prose_trace_event_count: direct_prose.trace_event_count,
                    direct_prose_final_output_count: direct_prose.final_output_count,
                    input_tokens: generation.input_tokens,
                    output_tokens: generation.output_tokens,
                    json_tool_plan_input_tokens: json_tool_plan.input_tokens,
                    json_tool_plan_output_tokens: json_tool_plan.output_tokens,
                    direct_prose_input_tokens: direct_prose.input_tokens,
                    direct_prose_output_tokens: direct_prose.output_tokens,
                    estimated_cost_usd: estimate_cost(
                        generation.input_tokens,
                        generation.output_tokens,
                        model.cost_per_1k_input_tokens_usd,
                        model.cost_per_1k_output_tokens_usd,
                    ),
                    json_tool_plan_estimated_cost_usd: estimate_cost(
                        json_tool_plan.input_tokens,
                        json_tool_plan.output_tokens,
                        model.cost_per_1k_input_tokens_usd,
                        model.cost_per_1k_output_tokens_usd,
                    ),
                    direct_prose_estimated_cost_usd: estimate_cost(
                        direct_prose.input_tokens,
                        direct_prose.output_tokens,
                        model.cost_per_1k_input_tokens_usd,
                        model.cost_per_1k_output_tokens_usd,
                    ),
                    duration_ms: generation.duration_ms,
                    json_tool_plan_duration_ms: json_tool_plan.duration_ms,
                    direct_prose_duration_ms: direct_prose.duration_ms,
                    generated_glyph: generation.glyph,
                    raw_output: generation.raw_output,
                    generated_json_tool_plan: json_tool_plan.generated_plan,
                    json_tool_plan_raw_output: json_tool_plan.raw_output,
                    generated_direct_prose: direct_prose.generated_prose,
                    direct_prose_raw_output: direct_prose.raw_output,
                    direct_failure_reason: eval_case.direct_failure_reason.to_string(),
                    parse_error,
                    validation_error,
                    run_error,
                    json_tool_plan_parse_error: json_tool_plan.parse_error,
                    json_tool_plan_run_error: json_tool_plan.run_error,
                    json_tool_plan_error: json_tool_plan.generation_error,
                    direct_prose_parse_error: direct_prose.parse_error,
                    direct_prose_validation_error: direct_prose.validation_error,
                    direct_prose_run_error: direct_prose.run_error,
                    direct_prose_error: direct_prose.generation_error,
                    error: generation_error,
                };
                observe_case(&result)?;
                results.push(result);
            }
        }
    }

    Ok(ControllerEvalReport {
        mode: report_mode(&models),
        actual_model_calls: models
            .iter()
            .filter(|model| model.mode.is_live_evidence())
            .count()
            * prompt_modes.len()
            * cases.len()
            * 3,
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
    })
}

pub fn select_controller_eval_cases(filter: &ControllerEvalCaseFilter) -> Vec<ControllerEvalCase> {
    let cases = controller_eval_cases()
        .into_iter()
        .filter(|eval_case| case_matches_filter(eval_case, filter))
        .collect::<Vec<_>>();

    match filter.limit {
        Some(limit) => cases.into_iter().take(limit).collect(),
        None => cases,
    }
}

fn case_matches_filter(eval_case: &ControllerEvalCase, filter: &ControllerEvalCaseFilter) -> bool {
    matches_any_or_empty(&filter.case_ids, |case_id| case_id == &eval_case.id)
        && matches_any_or_empty(&filter.tags, |tag| eval_case.tags.contains(tag))
        && matches_any_or_empty(&filter.families, |family| {
            eval_case
                .tags
                .iter()
                .any(|tag| tag == &format!("family:{family}"))
        })
        && matches_any_or_empty(&filter.profiles, |profile| {
            eval_case
                .tags
                .iter()
                .any(|tag| tag == &format!("profile:{profile}"))
        })
}

fn matches_any_or_empty(values: &[String], predicate: impl Fn(&String) -> bool) -> bool {
    values.is_empty() || values.iter().any(predicate)
}

fn report_mode(models: &[ControllerModelAdapter]) -> ControllerAdapterMode {
    let Some(first) = models.first() else {
        return ControllerAdapterMode::Fixture;
    };

    if models.iter().all(|model| model.mode == first.mode) {
        first.mode.clone()
    } else {
        ControllerAdapterMode::Mixed
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
        grammar_payload: ControllerGrammarPayload::None,
        cost_per_1k_input_tokens_usd: 0.0,
        cost_per_1k_output_tokens_usd: 0.0,
        source: ControllerModelSource::Fixture,
    }
}

pub fn create_openai_compatible_controller_models(
    endpoint: String,
    api_key: Option<String>,
    grammar_payload: ControllerGrammarPayload,
    model_ids: Vec<(ControllerParameterClass, String)>,
) -> Vec<ControllerModelAdapter> {
    model_ids
        .into_iter()
        .map(|(parameter_class, model_id)| ControllerModelAdapter {
            id: model_id,
            parameter_class,
            mode: ControllerAdapterMode::OpenAiCompatible,
            grammar_payload,
            cost_per_1k_input_tokens_usd: 0.0,
            cost_per_1k_output_tokens_usd: 0.0,
            source: ControllerModelSource::OpenAiCompatible {
                endpoint: endpoint.clone(),
                api_key: api_key.clone(),
            },
        })
        .collect()
}

pub fn create_offline_response_controller_model(
    model_id: String,
    parameter_class: ControllerParameterClass,
    grammar_payload: ControllerGrammarPayload,
    responses: BTreeMap<ControllerOfflineResponseKey, ControllerOfflineResponseSet>,
) -> ControllerModelAdapter {
    ControllerModelAdapter {
        id: model_id,
        parameter_class,
        mode: ControllerAdapterMode::OfflineResponses,
        grammar_payload,
        cost_per_1k_input_tokens_usd: 0.0,
        cost_per_1k_output_tokens_usd: 0.0,
        source: ControllerModelSource::OfflineResponses { responses },
    }
}

fn generate_with_model(
    model: &ControllerModelAdapter,
    eval_case: &ControllerEvalCase,
    prompt_mode: ControllerPromptMode,
) -> Result<ControllerGeneration, String> {
    match &model.source {
        ControllerModelSource::Fixture => Ok(generate_fixture(eval_case, prompt_mode)),
        ControllerModelSource::OpenAiCompatible { endpoint, api_key } => {
            generate_openai_compatible(model, eval_case, prompt_mode, endpoint, api_key.as_deref())
        }
        ControllerModelSource::OfflineResponses { responses } => {
            generate_offline_response(model, eval_case, prompt_mode, responses)
        }
    }
}

fn generate_fixture(
    eval_case: &ControllerEvalCase,
    prompt_mode: ControllerPromptMode,
) -> ControllerGeneration {
    let prompt = build_controller_prompt(eval_case, prompt_mode);
    let raw_output = if prompt_mode.expects_json_output() {
        serde_json::to_string(&json!({ "glyph": eval_case.expected_glyph }))
            .expect("fixture controller output serializes")
    } else {
        eval_case.expected_glyph.clone()
    };

    ControllerGeneration {
        glyph: eval_case.expected_glyph.clone(),
        raw_output: raw_output.clone(),
        input_tokens: approximate_tokens(&prompt),
        output_tokens: approximate_tokens(&raw_output),
        duration_ms: 0,
    }
}

fn generate_openai_compatible(
    model: &ControllerModelAdapter,
    eval_case: &ControllerEvalCase,
    prompt_mode: ControllerPromptMode,
    endpoint: &str,
    api_key: Option<&str>,
) -> Result<ControllerGeneration, String> {
    let started = std::time::Instant::now();
    let prompt = build_controller_request_prompt(
        eval_case,
        prompt_mode,
        model.grammar_payload,
        ControllerRequestKind::Glyph,
    );
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|error| error.to_string())?;
    let url = format!("{}/chat/completions", endpoint.trim_end_matches('/'));
    let body = build_openai_compatible_request_body(
        &model.id,
        eval_case,
        prompt_mode,
        model.grammar_payload,
        ControllerRequestKind::Glyph,
    );

    let mut request = client.post(url).json(&body);

    if let Some(api_key) = api_key {
        request = request.bearer_auth(api_key);
    }

    let response = request.send().map_err(|error| error.to_string())?;
    let status = response.status();
    let body: Value = response.json().map_err(|error| error.to_string())?;

    if !status.is_success() {
        return Err(format!("HTTP {status}: {body}"));
    }

    let raw_output = extract_chat_completion_content(&body)?;
    let glyph = extract_glyph_from_model_output(&raw_output);

    Ok(ControllerGeneration {
        glyph,
        raw_output: raw_output.clone(),
        input_tokens: approximate_tokens(&prompt),
        output_tokens: approximate_tokens(&raw_output),
        duration_ms: started.elapsed().as_millis(),
    })
}

fn generate_offline_response(
    model: &ControllerModelAdapter,
    eval_case: &ControllerEvalCase,
    prompt_mode: ControllerPromptMode,
    responses: &BTreeMap<ControllerOfflineResponseKey, ControllerOfflineResponseSet>,
) -> Result<ControllerGeneration, String> {
    let started = std::time::Instant::now();
    let raw_output = offline_response_for(responses, eval_case, prompt_mode)?.glyph;
    let glyph = extract_glyph_from_model_output(&raw_output);
    let prompt = build_controller_request_prompt(
        eval_case,
        prompt_mode,
        model.grammar_payload,
        ControllerRequestKind::Glyph,
    );

    Ok(ControllerGeneration {
        glyph,
        raw_output: raw_output.clone(),
        input_tokens: approximate_tokens(&prompt),
        output_tokens: approximate_tokens(&raw_output),
        duration_ms: started.elapsed().as_millis(),
    })
}

fn offline_response_for(
    responses: &BTreeMap<ControllerOfflineResponseKey, ControllerOfflineResponseSet>,
    eval_case: &ControllerEvalCase,
    prompt_mode: ControllerPromptMode,
) -> Result<ControllerOfflineResponseSet, String> {
    responses
        .get(&ControllerOfflineResponseKey {
            case_id: eval_case.id.clone(),
            prompt_mode,
        })
        .cloned()
        .ok_or_else(|| {
            format!(
                "Missing offline responses for case {} prompt mode {}",
                eval_case.id,
                prompt_mode.as_str()
            )
        })
}

fn evaluate_json_tool_plan_baseline(
    model: &ControllerModelAdapter,
    eval_case: &ControllerEvalCase,
    prompt_mode: ControllerPromptMode,
    vm: &GlyphVm,
) -> JsonToolPlanEvaluation {
    let generation = generate_json_tool_plan_with_model(model, eval_case, prompt_mode);
    let mut generation_error = None;
    let generation = match generation {
        Ok(generation) => generation,
        Err(error) => {
            generation_error = Some(error);
            ControllerGeneration {
                glyph: String::new(),
                raw_output: String::new(),
                input_tokens: approximate_tokens(&build_json_tool_plan_prompt(
                    eval_case,
                    prompt_mode,
                )),
                output_tokens: 0,
                duration_ms: 0,
            }
        }
    };

    let mut parse_error = None;
    let mut run_error = None;
    let mut run_ok = false;
    let mut trace_event_count = 0usize;
    let mut final_output_count = 0usize;

    let parsed = serde_json::from_str::<Value>(&generation.glyph)
        .map_err(|error| format!("Invalid JSON tool plan: {error}"))
        .and_then(|value| json_tool_plan_to_ir(&value))
        .and_then(|ir| validate_ir(ir).map_err(|error| error.to_string()));

    match parsed {
        Ok(ir) => match vm.execute(ir, GlyphVmOptions::default()) {
            Ok(result) => {
                trace_event_count = result.trace.len();
                final_output_count = result.outputs.len();
                run_ok = true;
            }
            Err(error) => run_error = Some(error.to_string()),
        },
        Err(error) => parse_error = Some(error),
    }

    JsonToolPlanEvaluation {
        parse_ok: parse_error.is_none(),
        run_ok,
        successful_trace: run_ok && trace_event_count > 0 && final_output_count > 0,
        trace_event_count,
        final_output_count,
        input_tokens: generation.input_tokens,
        output_tokens: generation.output_tokens,
        duration_ms: generation.duration_ms,
        generated_plan: generation.glyph,
        raw_output: generation.raw_output,
        parse_error,
        run_error,
        generation_error,
    }
}

fn generate_json_tool_plan_with_model(
    model: &ControllerModelAdapter,
    eval_case: &ControllerEvalCase,
    prompt_mode: ControllerPromptMode,
) -> Result<ControllerGeneration, String> {
    match &model.source {
        ControllerModelSource::Fixture => generate_fixture_json_tool_plan(eval_case, prompt_mode),
        ControllerModelSource::OpenAiCompatible { endpoint, api_key } => {
            generate_openai_compatible_json_tool_plan(
                model,
                eval_case,
                prompt_mode,
                endpoint,
                api_key.as_deref(),
            )
        }
        ControllerModelSource::OfflineResponses { responses } => {
            generate_offline_json_tool_plan(model, eval_case, prompt_mode, responses)
        }
    }
}

fn generate_fixture_json_tool_plan(
    eval_case: &ControllerEvalCase,
    prompt_mode: ControllerPromptMode,
) -> Result<ControllerGeneration, String> {
    let prompt = build_json_tool_plan_prompt(eval_case, prompt_mode);
    let ir = validate_ir(
        parse_glyph_to_ir(&eval_case.expected_glyph).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let plan = serde_json::to_string(&glyph_ir_to_json_tool_plan(&ir))
        .map_err(|error| error.to_string())?;

    Ok(ControllerGeneration {
        glyph: plan.clone(),
        raw_output: plan.clone(),
        input_tokens: approximate_tokens(&prompt),
        output_tokens: approximate_tokens(&plan),
        duration_ms: 0,
    })
}

fn generate_openai_compatible_json_tool_plan(
    model: &ControllerModelAdapter,
    eval_case: &ControllerEvalCase,
    prompt_mode: ControllerPromptMode,
    endpoint: &str,
    api_key: Option<&str>,
) -> Result<ControllerGeneration, String> {
    let started = std::time::Instant::now();
    let prompt = build_controller_request_prompt(
        eval_case,
        prompt_mode,
        model.grammar_payload,
        ControllerRequestKind::JsonToolPlan,
    );
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|error| error.to_string())?;
    let url = format!("{}/chat/completions", endpoint.trim_end_matches('/'));
    let body = build_openai_compatible_request_body(
        &model.id,
        eval_case,
        prompt_mode,
        model.grammar_payload,
        ControllerRequestKind::JsonToolPlan,
    );

    let mut request = client.post(url).json(&body);

    if let Some(api_key) = api_key {
        request = request.bearer_auth(api_key);
    }

    let response = request.send().map_err(|error| error.to_string())?;
    let status = response.status();
    let body: Value = response.json().map_err(|error| error.to_string())?;

    if !status.is_success() {
        return Err(format!("HTTP {status}: {body}"));
    }

    let raw_output = extract_chat_completion_content(&body)?;
    let plan = extract_json_tool_plan_from_model_output(&raw_output);

    Ok(ControllerGeneration {
        glyph: plan,
        raw_output: raw_output.clone(),
        input_tokens: approximate_tokens(&prompt),
        output_tokens: approximate_tokens(&raw_output),
        duration_ms: started.elapsed().as_millis(),
    })
}

fn generate_offline_json_tool_plan(
    model: &ControllerModelAdapter,
    eval_case: &ControllerEvalCase,
    prompt_mode: ControllerPromptMode,
    responses: &BTreeMap<ControllerOfflineResponseKey, ControllerOfflineResponseSet>,
) -> Result<ControllerGeneration, String> {
    let started = std::time::Instant::now();
    let raw_output = offline_response_for(responses, eval_case, prompt_mode)?.json_tool_plan;
    let plan = extract_json_tool_plan_from_model_output(&raw_output);
    let prompt = build_controller_request_prompt(
        eval_case,
        prompt_mode,
        model.grammar_payload,
        ControllerRequestKind::JsonToolPlan,
    );

    Ok(ControllerGeneration {
        glyph: plan,
        raw_output: raw_output.clone(),
        input_tokens: approximate_tokens(&prompt),
        output_tokens: approximate_tokens(&raw_output),
        duration_ms: started.elapsed().as_millis(),
    })
}

fn evaluate_direct_prose_baseline(
    model: &ControllerModelAdapter,
    eval_case: &ControllerEvalCase,
    prompt_mode: ControllerPromptMode,
    vm: &GlyphVm,
) -> DirectProseEvaluation {
    let generation = generate_direct_prose_with_model(model, eval_case, prompt_mode);
    let mut generation_error = None;
    let generation = match generation {
        Ok(generation) => generation,
        Err(error) => {
            generation_error = Some(error);
            ControllerGeneration {
                glyph: String::new(),
                raw_output: String::new(),
                input_tokens: approximate_tokens(&build_direct_prose_prompt(eval_case)),
                output_tokens: 0,
                duration_ms: 0,
            }
        }
    };

    let parse_error = parse_error(&generation.glyph);
    let parse_ok = parse_error.is_none();
    let validation_error = if parse_ok {
        validation_error(&generation.glyph)
    } else {
        None
    };
    let validate_ok = parse_ok && validation_error.is_none();
    let mut trace_event_count = 0usize;
    let mut final_output_count = 0usize;
    let mut run_ok = false;
    let mut run_error = None;

    if validate_ok {
        match vm.run_source(&generation.glyph) {
            Ok(result) => {
                trace_event_count = result.trace.len();
                final_output_count = result.outputs.len();
                run_ok = true;
            }
            Err(error) => run_error = Some(error.to_string()),
        }
    }

    DirectProseEvaluation {
        attempted: true,
        parse_ok,
        validate_ok,
        run_ok,
        successful_trace: run_ok && trace_event_count > 0 && final_output_count > 0,
        trace_event_count,
        final_output_count,
        input_tokens: generation.input_tokens,
        output_tokens: generation.output_tokens,
        duration_ms: generation.duration_ms,
        generated_prose: generation.glyph,
        raw_output: generation.raw_output,
        parse_error,
        validation_error,
        run_error,
        generation_error,
    }
}

fn generate_direct_prose_with_model(
    model: &ControllerModelAdapter,
    eval_case: &ControllerEvalCase,
    prompt_mode: ControllerPromptMode,
) -> Result<ControllerGeneration, String> {
    match &model.source {
        ControllerModelSource::Fixture => Ok(generate_fixture_direct_prose(eval_case)),
        ControllerModelSource::OpenAiCompatible { endpoint, api_key } => {
            generate_openai_compatible_direct_prose(
                model,
                eval_case,
                prompt_mode,
                endpoint,
                api_key.as_deref(),
            )
        }
        ControllerModelSource::OfflineResponses { responses } => {
            generate_offline_direct_prose(model, eval_case, prompt_mode, responses)
        }
    }
}

fn generate_fixture_direct_prose(eval_case: &ControllerEvalCase) -> ControllerGeneration {
    let prompt = build_direct_prose_prompt(eval_case);
    let raw_output = eval_case.direct_natural_language_plan.clone();

    ControllerGeneration {
        glyph: raw_output.clone(),
        raw_output,
        input_tokens: approximate_tokens(&prompt),
        output_tokens: approximate_tokens(&eval_case.direct_natural_language_plan),
        duration_ms: 0,
    }
}

fn generate_openai_compatible_direct_prose(
    model: &ControllerModelAdapter,
    eval_case: &ControllerEvalCase,
    prompt_mode: ControllerPromptMode,
    endpoint: &str,
    api_key: Option<&str>,
) -> Result<ControllerGeneration, String> {
    let started = std::time::Instant::now();
    let prompt = build_controller_request_prompt(
        eval_case,
        prompt_mode,
        model.grammar_payload,
        ControllerRequestKind::DirectProse,
    );
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|error| error.to_string())?;
    let url = format!("{}/chat/completions", endpoint.trim_end_matches('/'));
    let body = build_openai_compatible_request_body(
        &model.id,
        eval_case,
        prompt_mode,
        model.grammar_payload,
        ControllerRequestKind::DirectProse,
    );

    let mut request = client.post(url).json(&body);

    if let Some(api_key) = api_key {
        request = request.bearer_auth(api_key);
    }

    let response = request.send().map_err(|error| error.to_string())?;
    let status = response.status();
    let body: Value = response.json().map_err(|error| error.to_string())?;

    if !status.is_success() {
        return Err(format!("HTTP {status}: {body}"));
    }

    let raw_output = extract_chat_completion_content(&body)?;

    Ok(ControllerGeneration {
        glyph: raw_output.clone(),
        input_tokens: approximate_tokens(&prompt),
        output_tokens: approximate_tokens(&raw_output),
        raw_output,
        duration_ms: started.elapsed().as_millis(),
    })
}

fn generate_offline_direct_prose(
    model: &ControllerModelAdapter,
    eval_case: &ControllerEvalCase,
    prompt_mode: ControllerPromptMode,
    responses: &BTreeMap<ControllerOfflineResponseKey, ControllerOfflineResponseSet>,
) -> Result<ControllerGeneration, String> {
    let started = std::time::Instant::now();
    let raw_output = offline_response_for(responses, eval_case, prompt_mode)?.direct_prose;
    let prompt = build_controller_request_prompt(
        eval_case,
        prompt_mode,
        model.grammar_payload,
        ControllerRequestKind::DirectProse,
    );

    Ok(ControllerGeneration {
        glyph: raw_output.clone(),
        raw_output: raw_output.clone(),
        input_tokens: approximate_tokens(&prompt),
        output_tokens: approximate_tokens(&raw_output),
        duration_ms: started.elapsed().as_millis(),
    })
}

pub fn build_controller_prompt(
    eval_case: &ControllerEvalCase,
    prompt_mode: ControllerPromptMode,
) -> String {
    build_controller_prompt_with_payload(eval_case, prompt_mode, ControllerGrammarPayload::None)
}

pub fn build_controller_prompt_with_payload(
    eval_case: &ControllerEvalCase,
    prompt_mode: ControllerPromptMode,
    grammar_payload: ControllerGrammarPayload,
) -> String {
    if prompt_mode == ControllerPromptMode::Constrained
        && grammar_payload == ControllerGrammarPayload::Gbnf
    {
        return [
            "Convert this request into Glyph.",
            "",
            &format!("Request: {}", eval_case.request),
            "",
            "Decoder constraint:",
            "The API request supplies the official Glyph GBNF grammar. Return only Glyph source that satisfies that grammar.",
            "",
            "Rules:",
            "- Emit one complete Glyph program.",
            "- Use full primitive names only.",
            "- Always include a flow main block.",
            "- Every flow must include a top-level EXPORT step.",
            "- Use bounded repair blocks for repeated fixes.",
            "- Do not emit JSON, Markdown fences, or commentary.",
        ]
        .join("\n");
    }

    match prompt_mode {
        ControllerPromptMode::Constrained => [
            "Convert this request into Glyph.",
            "",
            &format!("Request: {}", eval_case.request),
            "",
            "Output JSON schema:",
            GLYPH_CONTROLLER_OUTPUT_JSON_SCHEMA,
            "",
            "Glyph grammar:",
            GLYPH_EBNF,
            "",
            "Rules:",
            "- Emit one complete Glyph program in the glyph field.",
            "- Use full primitive names only.",
            "- Always include a flow main block.",
            "- Every flow must include a top-level EXPORT step.",
            "- Use bounded repair blocks for repeated fixes.",
            "- Do not emit Markdown fences.",
        ]
        .join("\n"),
        ControllerPromptMode::SchemaOnly => [
            "Convert this request into Glyph.",
            "",
            &format!("Request: {}", eval_case.request),
            "",
            "Output JSON schema:",
            GLYPH_CONTROLLER_OUTPUT_JSON_SCHEMA,
            "",
            "Rules:",
            "- Emit one complete Glyph program in the glyph field.",
            "- Use full primitive names only.",
            "- Always include a flow main block.",
            "- Every flow must include a top-level EXPORT step.",
            "- Use bounded repair blocks for repeated fixes.",
            "- Do not emit Markdown fences.",
        ]
        .join("\n"),
        ControllerPromptMode::Plain => [
            "Convert this request into an executable Glyph program.",
            "",
            &format!("Request: {}", eval_case.request),
            "",
            "Return only the Glyph source. Include a flow main block with a top-level EXPORT step. Do not return JSON, Markdown, or commentary.",
        ]
        .join("\n"),
    }
}

pub fn build_json_tool_plan_prompt(
    eval_case: &ControllerEvalCase,
    prompt_mode: ControllerPromptMode,
) -> String {
    match prompt_mode {
        ControllerPromptMode::Constrained | ControllerPromptMode::SchemaOnly => [
            "Convert this request into a generic JSON tool plan.",
            "",
            &format!("Request: {}", eval_case.request),
            "",
            "Output JSON schema:",
            GENERIC_TOOL_PLAN_JSON_SCHEMA,
            "",
            "Rules:",
            "- Return one JSON object with a nonempty steps array.",
            "- Use only these primitive ops: SPEC, PLAN, GEN, CHECK, FIX, PATCH, SUM, ASK, EXPORT, RUN, READ, WRITE.",
            "- Use {\"var\":\"name\"} for variable references.",
            "- Use {\"ctx\":\"path\"} for context references.",
            "- Use repair objects for bounded repair loops.",
            "- Include an EXPORT step for the final artifact.",
            "- Do not emit Markdown fences.",
        ]
        .join("\n"),
        ControllerPromptMode::Plain => [
            "Convert this request into a generic JSON tool plan.",
            "",
            &format!("Request: {}", eval_case.request),
            "",
            "Return only JSON. Include a steps array of tool calls with op, args, optional assignTo fields, and an EXPORT step for the final artifact.",
        ]
        .join("\n"),
    }
}

pub fn build_direct_prose_prompt(eval_case: &ControllerEvalCase) -> String {
    [
        "Complete this harness-control request without Glyph.",
        "",
        &format!("Request: {}", eval_case.request),
        "",
        "Rules:",
        "- Return a concise natural-language plan only.",
        "- Do not use Glyph syntax, JSON, code fences, or primitive call notation.",
        "- Do not create typed variable assignments.",
        "- The evaluator will record whether this no-Glyph output can produce an executable trace.",
    ]
    .join("\n")
}

pub fn build_controller_request_prompt(
    eval_case: &ControllerEvalCase,
    prompt_mode: ControllerPromptMode,
    grammar_payload: ControllerGrammarPayload,
    request_kind: ControllerRequestKind,
) -> String {
    match request_kind {
        ControllerRequestKind::Glyph => {
            build_controller_prompt_with_payload(eval_case, prompt_mode, grammar_payload)
        }
        ControllerRequestKind::JsonToolPlan => build_json_tool_plan_prompt(eval_case, prompt_mode),
        ControllerRequestKind::DirectProse => build_direct_prose_prompt(eval_case),
    }
}

pub fn build_openai_compatible_request_body(
    model_id: &str,
    eval_case: &ControllerEvalCase,
    prompt_mode: ControllerPromptMode,
    grammar_payload: ControllerGrammarPayload,
    request_kind: ControllerRequestKind,
) -> Value {
    let prompt =
        build_controller_request_prompt(eval_case, prompt_mode, grammar_payload, request_kind);
    let mut body = json!({
        "model": model_id,
        "temperature": 0,
        "messages": [
            {
                "role": "system",
                "content": controller_request_system_prompt(prompt_mode, grammar_payload, request_kind)
            },
            {
                "role": "user",
                "content": prompt
            }
        ]
    });

    match request_kind {
        ControllerRequestKind::Glyph => {
            if prompt_mode.expects_json_output()
                && grammar_payload != ControllerGrammarPayload::Gbnf
            {
                body["response_format"] = json!({ "type": "json_object" });
            }

            if prompt_mode == ControllerPromptMode::Constrained
                && grammar_payload == ControllerGrammarPayload::Gbnf
            {
                body["grammar"] = Value::String(GLYPH_GBNF.to_string());
            }
        }
        ControllerRequestKind::JsonToolPlan => {
            if prompt_mode.expects_json_output() {
                body["response_format"] = json!({ "type": "json_object" });
            }
        }
        ControllerRequestKind::DirectProse => {}
    }

    body
}

fn controller_system_prompt(
    prompt_mode: ControllerPromptMode,
    grammar_payload: ControllerGrammarPayload,
) -> &'static str {
    match (prompt_mode, grammar_payload) {
        (ControllerPromptMode::Constrained, ControllerGrammarPayload::Gbnf) => {
            "You are a Glyph controller. The decoder is constrained with Glyph GBNF. Return only one complete executable Glyph program."
        }
        (ControllerPromptMode::Constrained, _) => {
            "You are a Glyph controller. Return only JSON that matches the provided schema. The glyph field must contain one complete executable Glyph program that follows the provided grammar."
        }
        (ControllerPromptMode::SchemaOnly, _) => {
            "You are a Glyph controller. Return only JSON that matches the provided schema. The glyph field must contain one complete executable Glyph program."
        }
        (ControllerPromptMode::Plain, _) => {
            "You are a Glyph controller. Return only one complete executable Glyph program."
        }
    }
}

fn controller_request_system_prompt(
    prompt_mode: ControllerPromptMode,
    grammar_payload: ControllerGrammarPayload,
    request_kind: ControllerRequestKind,
) -> &'static str {
    match request_kind {
        ControllerRequestKind::Glyph => controller_system_prompt(prompt_mode, grammar_payload),
        ControllerRequestKind::JsonToolPlan => json_tool_plan_system_prompt(prompt_mode),
        ControllerRequestKind::DirectProse => direct_prose_system_prompt(prompt_mode),
    }
}

fn json_tool_plan_system_prompt(prompt_mode: ControllerPromptMode) -> &'static str {
    match prompt_mode {
        ControllerPromptMode::Constrained | ControllerPromptMode::SchemaOnly => {
            "You are a harness controller baseline. Return only JSON that matches the provided generic tool-plan schema."
        }
        ControllerPromptMode::Plain => {
            "You are a harness controller baseline. Return only one JSON tool-plan object."
        }
    }
}

fn direct_prose_system_prompt(_prompt_mode: ControllerPromptMode) -> &'static str {
    "You are a direct natural-language planning baseline. Do not use Glyph, JSON, code, or tool-call syntax."
}

fn extract_chat_completion_content(body: &Value) -> Result<String, String> {
    body.get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| "Controller response did not include choices[0].message.content".to_string())
}

pub(crate) fn extract_glyph_from_model_output(raw_output: &str) -> String {
    serde_json::from_str::<Value>(raw_output)
        .ok()
        .and_then(|value| {
            value
                .get("glyph")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| raw_output.to_string())
}

pub(crate) fn extract_json_tool_plan_from_model_output(raw_output: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(raw_output) else {
        return raw_output.to_string();
    };

    if value.get("steps").is_some() {
        return serde_json::to_string(&value).unwrap_or_else(|_| raw_output.to_string());
    }

    for key in ["toolPlan", "tool_plan", "plan"] {
        if let Some(plan) = value.get(key)
            && plan.get("steps").is_some()
        {
            return serde_json::to_string(plan).unwrap_or_else(|_| raw_output.to_string());
        }
    }

    raw_output.to_string()
}

fn glyph_ir_to_json_tool_plan(ir: &GlyphIr) -> Value {
    let steps = ir
        .flows
        .first()
        .map(|flow| {
            flow.steps
                .iter()
                .map(glyph_ir_step_to_json_tool_plan_step)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    json!({
        "goal": ir.goal,
        "context": ir.context,
        "steps": steps
    })
}

fn glyph_ir_step_to_json_tool_plan_step(step: &GlyphIrStep) -> Value {
    match step {
        GlyphIrStep::Tool(tool) => {
            let mut object = Map::new();
            object.insert("op".to_string(), Value::String(tool.op.clone()));
            object.insert("args".to_string(), Value::Object(tool.args.clone()));
            if let Some(assign_to) = &tool.assign_to {
                object.insert("assignTo".to_string(), Value::String(assign_to.clone()));
            }
            Value::Object(object)
        }
        GlyphIrStep::Repair(repair) => json!({
            "repair": {
                "targetVar": repair.target_var,
                "reportVar": repair.report_var,
                "maxIterations": repair.max_iterations,
                "steps": repair
                    .steps
                    .iter()
                    .map(glyph_ir_step_to_json_tool_plan_step)
                    .collect::<Vec<_>>()
            }
        }),
    }
}

pub(crate) fn json_tool_plan_to_ir(value: &Value) -> Result<GlyphIr, String> {
    let plan = unwrap_json_tool_plan(value);
    let object = plan
        .as_object()
        .ok_or_else(|| "JSON tool plan must be an object".to_string())?;
    let context = match object.get("context") {
        Some(Value::Object(context)) => context.clone(),
        Some(_) => return Err("JSON tool plan context must be an object".to_string()),
        None => Map::new(),
    };
    let steps = object
        .get("steps")
        .and_then(Value::as_array)
        .ok_or_else(|| "JSON tool plan must contain a steps array".to_string())?;

    let mut counter = 0usize;
    let mut next_id = || {
        counter += 1;
        format!("step_{counter}")
    };

    Ok(GlyphIr {
        version: GLYPH_IR_VERSION.to_string(),
        goal: object
            .get("goal")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        context,
        flows: vec![GlyphIrFlow {
            name: "main".to_string(),
            steps: steps
                .iter()
                .map(|step| json_tool_plan_step_to_ir(step, &mut next_id))
                .collect::<Result<Vec<_>, _>>()?,
        }],
    })
}

fn unwrap_json_tool_plan(value: &Value) -> &Value {
    if value.get("steps").is_some() {
        return value;
    }

    for key in ["toolPlan", "tool_plan", "plan"] {
        if let Some(plan) = value.get(key)
            && plan.get("steps").is_some()
        {
            return plan;
        }
    }

    value
}

fn json_tool_plan_step_to_ir(
    value: &Value,
    next_id: &mut impl FnMut() -> String,
) -> Result<GlyphIrStep, String> {
    let object = value
        .as_object()
        .ok_or_else(|| "JSON tool plan step must be an object".to_string())?;

    if let Some(repair) = object.get("repair") {
        return json_tool_plan_repair_step_to_ir(repair, next_id);
    }

    let op = object
        .get("op")
        .and_then(Value::as_str)
        .ok_or_else(|| "JSON tool plan tool step requires string op".to_string())?;
    let args = match object.get("args") {
        Some(Value::Object(args)) => args.clone(),
        Some(_) => return Err(format!("JSON tool plan args for {op} must be an object")),
        None => Map::new(),
    };
    let assign_to = object
        .get("assignTo")
        .or_else(|| object.get("assign_to"))
        .and_then(Value::as_str)
        .map(ToString::to_string);

    Ok(GlyphIrStep::Tool(GlyphToolStep {
        id: next_id(),
        op: op.to_uppercase(),
        args,
        assign_to,
    }))
}

fn json_tool_plan_repair_step_to_ir(
    value: &Value,
    next_id: &mut impl FnMut() -> String,
) -> Result<GlyphIrStep, String> {
    let object = value
        .as_object()
        .ok_or_else(|| "JSON tool plan repair value must be an object".to_string())?;
    let steps = object
        .get("steps")
        .and_then(Value::as_array)
        .ok_or_else(|| "JSON tool plan repair requires steps array".to_string())?;

    Ok(GlyphIrStep::Repair(GlyphRepairStep {
        id: next_id(),
        target_var: read_string_field(object, &["targetVar", "target_var", "target"])?,
        report_var: read_string_field(object, &["reportVar", "report_var", "report"])?,
        max_iterations: read_usize_field(object, &["maxIterations", "max_iterations", "max"])?,
        steps: steps
            .iter()
            .map(|step| json_tool_plan_step_to_ir(step, next_id))
            .collect::<Result<Vec<_>, _>>()?,
    }))
}

fn read_string_field(object: &Map<String, Value>, keys: &[&str]) -> Result<String, String> {
    keys.iter()
        .find_map(|key| object.get(*key).and_then(Value::as_str))
        .map(ToString::to_string)
        .ok_or_else(|| format!("Missing string field; expected one of {}", keys.join(", ")))
}

fn read_usize_field(object: &Map<String, Value>, keys: &[&str]) -> Result<usize, String> {
    let value = keys
        .iter()
        .find_map(|key| object.get(*key).and_then(Value::as_u64))
        .ok_or_else(|| format!("Missing integer field; expected one of {}", keys.join(", ")))?;

    usize::try_from(value).map_err(|_| format!("Integer field too large: {value}"))
}

fn parse_error(source: &str) -> Option<String> {
    parse_glyph(source).err().map(|error| error.to_string())
}

fn validation_error(source: &str) -> Option<String> {
    match parse_glyph_to_ir(source) {
        Ok(ir) => validate_ir(ir).err().map(|error| error.to_string()),
        Err(error) => Some(error.to_string()),
    }
}

fn can_parse_glyph(source: &str) -> bool {
    parse_glyph(source).is_ok()
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
    let mut groups = Vec::<(String, ControllerPromptMode, ControllerGrammarPayload)>::new();
    for result in results {
        let group = (
            result.model_id.clone(),
            result.prompt_mode,
            result.grammar_payload,
        );
        if !groups.contains(&group) {
            groups.push(group);
        }
    }

    groups
        .into_iter()
        .map(|(model_id, prompt_mode, grammar_payload)| {
            let model_results = results
                .iter()
                .filter(|result| {
                    result.model_id == model_id
                        && result.prompt_mode == prompt_mode
                        && result.grammar_payload == grammar_payload
                })
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
                prompt_mode: first.prompt_mode,
                grammar_payload: first.grammar_payload,
                cases: model_results.len(),
                valid_program_rate: rate(&model_results, |result| {
                    result.parse_ok && result.validate_ok
                }),
                run_success_rate: rate(&model_results, |result| result.run_ok),
                successful_trace_rate: rate(&model_results, |result| result.successful_trace),
                glyph_over_direct_plan_rate: rate(&model_results, |result| {
                    result.glyph_beats_direct_plan
                }),
                json_tool_plan_run_success_rate: rate(&model_results, |result| {
                    result.json_tool_plan_run_ok
                }),
                json_tool_plan_successful_trace_rate: rate(&model_results, |result| {
                    result.json_tool_plan_successful_trace
                }),
                glyph_over_json_tool_plan_rate: rate(&model_results, |result| {
                    result.glyph_beats_json_tool_plan
                }),
                direct_prose_successful_trace_rate: rate(&model_results, |result| {
                    result.direct_prose_successful_trace
                }),
                glyph_over_direct_prose_rate: rate(&model_results, |result| {
                    result.glyph_beats_direct_prose
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
                average_json_tool_plan_output_tokens: average(
                    &model_results
                        .iter()
                        .map(|result| result.json_tool_plan_output_tokens as f64)
                        .collect::<Vec<_>>(),
                ),
                average_direct_prose_output_tokens: average(
                    &model_results
                        .iter()
                        .map(|result| result.direct_prose_output_tokens as f64)
                        .collect::<Vec<_>>(),
                ),
                total_estimated_cost_usd: model_results
                    .iter()
                    .map(|result| result.estimated_cost_usd)
                    .sum(),
                total_json_tool_plan_estimated_cost_usd: model_results
                    .iter()
                    .map(|result| result.json_tool_plan_estimated_cost_usd)
                    .sum(),
                total_direct_prose_estimated_cost_usd: model_results
                    .iter()
                    .map(|result| result.direct_prose_estimated_cost_usd)
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
