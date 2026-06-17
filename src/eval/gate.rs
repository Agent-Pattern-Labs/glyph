use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::controller::{
    ControllerAdapterMode, ControllerEvalCaseResult, ControllerGrammarPayload,
    ControllerParameterClass, ControllerPromptMode,
};

const MIN_CASES_PER_TARGET: usize = 72;
const MIN_VALID_PROGRAM_RATE: f64 = 0.90;
const MIN_SUCCESSFUL_TRACE_RATE: f64 = 0.85;
const MIN_CONSTRAINED_LIFT: f64 = 0.20;
const MIN_PLAIN_ALREADY_STRONG_RATE: f64 = 0.90;
const MIN_REPAIR_SUCCESS_RATE: f64 = 0.80;
const REQUIRED_PROFILES: &[&str] = &["normal", "terse", "noisy", "adversarial"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ControllerGateDecision {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ControllerGateCheckStatus {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerGateReport {
    pub decision: ControllerGateDecision,
    pub passed: bool,
    #[serde(rename = "caseRows")]
    pub case_rows: usize,
    #[serde(rename = "liveCaseRows")]
    pub live_case_rows: usize,
    #[serde(rename = "targetCaseRows")]
    pub target_case_rows: usize,
    pub metrics: ControllerGateMetrics,
    pub checks: Vec<ControllerGateCheck>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerGateMetrics {
    #[serde(rename = "targetValidProgramRate")]
    pub target_valid_program_rate: f64,
    #[serde(rename = "targetSuccessfulTraceRate")]
    pub target_successful_trace_rate: f64,
    #[serde(rename = "targetJsonToolPlanSuccessfulTraceRate")]
    pub target_json_tool_plan_successful_trace_rate: f64,
    #[serde(rename = "targetDirectProseSuccessfulTraceRate")]
    pub target_direct_prose_successful_trace_rate: f64,
    #[serde(rename = "targetRepairSuccessRate")]
    pub target_repair_success_rate: Option<f64>,
    #[serde(rename = "targetAverageOutputTokens")]
    pub target_average_output_tokens: f64,
    #[serde(rename = "targetAverageJsonToolPlanOutputTokens")]
    pub target_average_json_tool_plan_output_tokens: f64,
    #[serde(rename = "targetAverageDirectProseOutputTokens")]
    pub target_average_direct_prose_output_tokens: f64,
    #[serde(rename = "oneBPlainSuccessfulTraceRate")]
    pub one_b_plain_successful_trace_rate: Option<f64>,
    #[serde(rename = "constrainedVsPlainLift")]
    pub constrained_vs_plain_lift: Option<f64>,
    #[serde(rename = "largerPlainSuccessfulTraceRate")]
    pub larger_plain_successful_trace_rate: Option<f64>,
    #[serde(rename = "largerPlainAverageOutputTokens")]
    pub larger_plain_average_output_tokens: Option<f64>,
    #[serde(rename = "largerPlainAverageJsonToolPlanOutputTokens")]
    pub larger_plain_average_json_tool_plan_output_tokens: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerGateCheck {
    pub id: String,
    pub status: ControllerGateCheckStatus,
    pub observed: String,
    pub required: String,
}

pub fn evaluate_controller_gate(cases: &[ControllerEvalCaseResult]) -> ControllerGateReport {
    let live_cases = cases
        .iter()
        .filter(|case| case.adapter_mode == ControllerAdapterMode::OpenAiCompatible)
        .collect::<Vec<_>>();
    let target_cases = live_cases
        .iter()
        .copied()
        .filter(|case| {
            case.parameter_class == ControllerParameterClass::OneB
                && case.prompt_mode == ControllerPromptMode::Constrained
        })
        .collect::<Vec<_>>();
    let one_b_plain_cases = live_cases
        .iter()
        .copied()
        .filter(|case| {
            case.parameter_class == ControllerParameterClass::OneB
                && case.prompt_mode == ControllerPromptMode::Plain
        })
        .collect::<Vec<_>>();
    let larger_plain_cases = live_cases
        .iter()
        .copied()
        .filter(|case| {
            is_larger_model(case.parameter_class) && case.prompt_mode == ControllerPromptMode::Plain
        })
        .collect::<Vec<_>>();

    let target_valid_program_rate = rate(&target_cases, |case| case.parse_ok && case.validate_ok);
    let target_successful_trace_rate = rate(&target_cases, |case| case.successful_trace);
    let target_json_tool_plan_successful_trace_rate =
        rate(&target_cases, |case| case.json_tool_plan_successful_trace);
    let target_direct_prose_successful_trace_rate =
        rate(&target_cases, |case| case.direct_prose_successful_trace);
    let target_average_output_tokens = average(
        &target_cases
            .iter()
            .map(|case| case.output_tokens as f64)
            .collect::<Vec<_>>(),
    );
    let target_average_direct_prose_output_tokens = average(
        &target_cases
            .iter()
            .map(|case| case.direct_prose_output_tokens as f64)
            .collect::<Vec<_>>(),
    );
    let target_average_json_tool_plan_output_tokens = average(
        &target_cases
            .iter()
            .map(|case| case.json_tool_plan_output_tokens as f64)
            .collect::<Vec<_>>(),
    );
    let target_repair_cases = target_cases
        .iter()
        .copied()
        .filter(|case| case.expects_repair_loop)
        .collect::<Vec<_>>();
    let target_repair_success_rate = if target_repair_cases.is_empty() {
        None
    } else {
        Some(rate(&target_repair_cases, |case| {
            case.repair_loop_succeeded == Some(true)
        }))
    };
    let one_b_plain_successful_trace_rate = if one_b_plain_cases.is_empty() {
        None
    } else {
        Some(rate(&one_b_plain_cases, |case| case.successful_trace))
    };
    let constrained_vs_plain_lift =
        one_b_plain_successful_trace_rate.map(|plain| target_successful_trace_rate - plain);
    let larger_plain_successful_trace_rate = if larger_plain_cases.is_empty() {
        None
    } else {
        Some(rate(&larger_plain_cases, |case| case.successful_trace))
    };
    let larger_plain_average_output_tokens = if larger_plain_cases.is_empty() {
        None
    } else {
        Some(average(
            &larger_plain_cases
                .iter()
                .map(|case| case.output_tokens as f64)
                .collect::<Vec<_>>(),
        ))
    };
    let larger_plain_average_json_tool_plan_output_tokens = if larger_plain_cases.is_empty() {
        None
    } else {
        Some(average(
            &larger_plain_cases
                .iter()
                .map(|case| case.json_tool_plan_output_tokens as f64)
                .collect::<Vec<_>>(),
        ))
    };

    let metrics = ControllerGateMetrics {
        target_valid_program_rate,
        target_successful_trace_rate,
        target_json_tool_plan_successful_trace_rate,
        target_direct_prose_successful_trace_rate,
        target_repair_success_rate,
        target_average_output_tokens,
        target_average_json_tool_plan_output_tokens,
        target_average_direct_prose_output_tokens,
        one_b_plain_successful_trace_rate,
        constrained_vs_plain_lift,
        larger_plain_successful_trace_rate,
        larger_plain_average_output_tokens,
        larger_plain_average_json_tool_plan_output_tokens,
    };

    let checks = vec![
        check(
            "live_results",
            !live_cases.is_empty(),
            live_cases.len().to_string(),
            "at least one openai-compatible result row".to_string(),
        ),
        check(
            "required_model_buckets",
            has_required_buckets(&live_cases),
            observed_buckets(&live_cases).join(","),
            "1b,3b,7b,frontier live rows".to_string(),
        ),
        check(
            "required_prompt_modes",
            has_required_prompt_modes(&live_cases),
            observed_prompt_modes(&live_cases).join(","),
            "constrained,schema-only,plain live rows".to_string(),
        ),
        check(
            "target_case_count",
            target_cases.len() >= MIN_CASES_PER_TARGET,
            target_cases.len().to_string(),
            format!(">= {MIN_CASES_PER_TARGET} 1b constrained live rows"),
        ),
        check(
            "target_grammar_payload",
            !target_cases.is_empty()
                && target_cases
                    .iter()
                    .all(|case| case.grammar_payload == ControllerGrammarPayload::Gbnf),
            observed_grammar_payloads(&target_cases).join(","),
            "all 1b constrained live rows use grammarPayload=gbnf".to_string(),
        ),
        check(
            "target_profile_coverage",
            has_required_profile_coverage(&target_cases),
            observed_profile_coverage(&target_cases),
            "every workflow family has normal, terse, noisy, and adversarial rows".to_string(),
        ),
        check(
            "valid_program_rate",
            target_valid_program_rate >= MIN_VALID_PROGRAM_RATE,
            format_rate(target_valid_program_rate),
            format!(">= {}", format_rate(MIN_VALID_PROGRAM_RATE)),
        ),
        check(
            "successful_trace_rate",
            target_successful_trace_rate >= MIN_SUCCESSFUL_TRACE_RATE,
            format_rate(target_successful_trace_rate),
            format!(">= {}", format_rate(MIN_SUCCESSFUL_TRACE_RATE)),
        ),
        check(
            "constrained_vs_plain",
            one_b_plain_successful_trace_rate.is_some_and(|plain| {
                target_successful_trace_rate - plain >= MIN_CONSTRAINED_LIFT
                    || plain >= MIN_PLAIN_ALREADY_STRONG_RATE
            }),
            match (one_b_plain_successful_trace_rate, constrained_vs_plain_lift) {
                (Some(plain), Some(lift)) => {
                    format!("plain={}, lift={}", format_rate(plain), format_rate(lift))
                }
                _ => "missing 1b plain rows".to_string(),
            },
            format!(
                "lift >= {} or plain >= {}",
                format_rate(MIN_CONSTRAINED_LIFT),
                format_rate(MIN_PLAIN_ALREADY_STRONG_RATE)
            ),
        ),
        check(
            "generic_json_tool_plan_baseline",
            target_successful_trace_rate > target_json_tool_plan_successful_trace_rate,
            format!(
                "glyph={}, json={}",
                format_rate(target_successful_trace_rate),
                format_rate(target_json_tool_plan_successful_trace_rate)
            ),
            "Glyph successful trace rate > generic JSON tool-plan successful trace rate"
                .to_string(),
        ),
        check(
            "direct_prose_baseline",
            !target_cases.is_empty()
                && target_cases.iter().all(|case| case.direct_prose_attempted)
                && target_successful_trace_rate > target_direct_prose_successful_trace_rate,
            format!(
                "glyph={}, direct_prose={}, attempted={}/{}",
                format_rate(target_successful_trace_rate),
                format_rate(target_direct_prose_successful_trace_rate),
                target_cases
                    .iter()
                    .filter(|case| case.direct_prose_attempted)
                    .count(),
                target_cases.len()
            ),
            "Glyph successful trace rate > direct-prose successful trace rate, with every target row carrying a direct-prose attempt".to_string(),
        ),
        check(
            "larger_plain_baseline",
            larger_plain_successful_trace_rate
                .is_some_and(|larger| target_successful_trace_rate >= larger),
            match larger_plain_successful_trace_rate {
                Some(larger) => {
                    format!(
                        "target={}, larger_plain={}",
                        format_rate(target_successful_trace_rate),
                        format_rate(larger)
                    )
                }
                None => "missing larger plain rows".to_string(),
            },
            "1b constrained successful trace rate >= 3b/7b/frontier plain successful trace rate"
                .to_string(),
        ),
        check(
            "larger_json_tool_plan_compactness",
            larger_plain_average_json_tool_plan_output_tokens.is_some_and(|larger_json| {
                target_average_output_tokens > 0.0 && target_average_output_tokens < larger_json
            }),
            match larger_plain_average_json_tool_plan_output_tokens {
                Some(larger_json) => {
                    format!(
                        "target_glyph={}, larger_json={}",
                        format_number(target_average_output_tokens),
                        format_number(larger_json)
                    )
                }
                None => "missing larger plain rows".to_string(),
            },
            "1b constrained Glyph output tokens < larger models' generic JSON tool-plan output tokens"
                .to_string(),
        ),
        check(
            "output_compactness",
            target_average_output_tokens > 0.0
                && target_average_output_tokens < target_average_json_tool_plan_output_tokens,
            format!(
                "glyph={}, json={}",
                format_number(target_average_output_tokens),
                format_number(target_average_json_tool_plan_output_tokens)
            ),
            "Glyph average output tokens < generic JSON tool-plan average output tokens"
                .to_string(),
        ),
        check(
            "repair_success_rate",
            target_repair_success_rate.is_some_and(|rate| rate >= MIN_REPAIR_SUCCESS_RATE),
            target_repair_success_rate
                .map(format_rate)
                .unwrap_or_else(|| "missing repair cases".to_string()),
            format!(">= {}", format_rate(MIN_REPAIR_SUCCESS_RATE)),
        ),
        check(
            "failure_classification",
            failures_are_classified(cases),
            classified_failure_summary(cases),
            "every failed parse/validation/runtime/generation path has an error field".to_string(),
        ),
    ];

    let passed = checks
        .iter()
        .all(|check| check.status == ControllerGateCheckStatus::Pass);

    ControllerGateReport {
        decision: if passed {
            ControllerGateDecision::Pass
        } else {
            ControllerGateDecision::Fail
        },
        passed,
        case_rows: cases.len(),
        live_case_rows: live_cases.len(),
        target_case_rows: target_cases.len(),
        metrics,
        checks,
    }
}

fn check(id: &str, passed: bool, observed: String, required: String) -> ControllerGateCheck {
    ControllerGateCheck {
        id: id.to_string(),
        status: if passed {
            ControllerGateCheckStatus::Pass
        } else {
            ControllerGateCheckStatus::Fail
        },
        observed,
        required,
    }
}

fn has_required_buckets(cases: &[&ControllerEvalCaseResult]) -> bool {
    let observed = observed_buckets(cases);
    ["1b", "3b", "7b", "frontier"]
        .iter()
        .all(|bucket| observed.contains(&bucket.to_string()))
}

fn is_larger_model(parameter_class: ControllerParameterClass) -> bool {
    matches!(
        parameter_class,
        ControllerParameterClass::ThreeB
            | ControllerParameterClass::SevenB
            | ControllerParameterClass::Frontier
    )
}

fn observed_buckets(cases: &[&ControllerEvalCaseResult]) -> Vec<String> {
    let buckets = cases
        .iter()
        .map(|case| case.parameter_class.as_str().to_string())
        .collect::<BTreeSet<_>>();
    buckets.into_iter().collect()
}

fn has_required_prompt_modes(cases: &[&ControllerEvalCaseResult]) -> bool {
    let observed = observed_prompt_modes(cases);
    ["constrained", "schema-only", "plain"]
        .iter()
        .all(|mode| observed.contains(&mode.to_string()))
}

fn observed_prompt_modes(cases: &[&ControllerEvalCaseResult]) -> Vec<String> {
    let modes = cases
        .iter()
        .map(|case| case.prompt_mode.as_str().to_string())
        .collect::<BTreeSet<_>>();
    modes.into_iter().collect()
}

fn observed_grammar_payloads(cases: &[&ControllerEvalCaseResult]) -> Vec<String> {
    let payloads = cases
        .iter()
        .map(|case| case.grammar_payload.as_str().to_string())
        .collect::<BTreeSet<_>>();
    payloads.into_iter().collect()
}

fn has_required_profile_coverage(cases: &[&ControllerEvalCaseResult]) -> bool {
    let matrix = family_profile_matrix(cases);
    !matrix.is_empty()
        && matrix.values().all(|profiles| {
            REQUIRED_PROFILES
                .iter()
                .all(|profile| profiles.contains(*profile))
        })
}

fn observed_profile_coverage(cases: &[&ControllerEvalCaseResult]) -> String {
    let matrix = family_profile_matrix(cases);
    let complete = matrix
        .values()
        .filter(|profiles| {
            REQUIRED_PROFILES
                .iter()
                .all(|profile| profiles.contains(*profile))
        })
        .count();

    format!("families={}, complete={complete}", matrix.len())
}

fn family_profile_matrix(
    cases: &[&ControllerEvalCaseResult],
) -> BTreeMap<String, BTreeSet<String>> {
    let mut matrix = BTreeMap::<String, BTreeSet<String>>::new();

    for case in cases {
        let family = tag_value(&case.tags, "family:");
        let profile = tag_value(&case.tags, "profile:");
        if let (Some(family), Some(profile)) = (family, profile) {
            matrix.entry(family).or_default().insert(profile);
        }
    }

    matrix
}

fn tag_value(tags: &[String], prefix: &str) -> Option<String> {
    tags.iter()
        .find_map(|tag| tag.strip_prefix(prefix).map(ToString::to_string))
}

fn failures_are_classified(cases: &[ControllerEvalCaseResult]) -> bool {
    cases.iter().all(|case| {
        (case.parse_ok || case.parse_error.is_some())
            && (case.validate_ok || case.validation_error.is_some() || case.parse_error.is_some())
            && (case.run_ok || case.run_error.is_some() || !case.validate_ok)
            && (case.error.is_none() || case.generated_glyph.is_empty())
            && (case.json_tool_plan_parse_ok || case.json_tool_plan_parse_error.is_some())
            && (case.json_tool_plan_run_ok
                || case.json_tool_plan_run_error.is_some()
                || !case.json_tool_plan_parse_ok)
            && (case.json_tool_plan_error.is_none() || case.generated_json_tool_plan.is_empty())
            && (case.direct_prose_parse_ok || case.direct_prose_parse_error.is_some())
            && (case.direct_prose_validate_ok
                || case.direct_prose_validation_error.is_some()
                || case.direct_prose_parse_error.is_some())
            && (case.direct_prose_run_ok
                || case.direct_prose_run_error.is_some()
                || !case.direct_prose_validate_ok)
            && (case.direct_prose_error.is_none() || case.generated_direct_prose.is_empty())
    })
}

fn classified_failure_summary(cases: &[ControllerEvalCaseResult]) -> String {
    let unclassified = cases.len()
        - cases
            .iter()
            .filter(|case| {
                (case.parse_ok || case.parse_error.is_some())
                    && (case.validate_ok
                        || case.validation_error.is_some()
                        || case.parse_error.is_some())
                    && (case.run_ok || case.run_error.is_some() || !case.validate_ok)
                    && (case.json_tool_plan_parse_ok || case.json_tool_plan_parse_error.is_some())
                    && (case.json_tool_plan_run_ok
                        || case.json_tool_plan_run_error.is_some()
                        || !case.json_tool_plan_parse_ok)
                    && (case.direct_prose_parse_ok || case.direct_prose_parse_error.is_some())
                    && (case.direct_prose_validate_ok
                        || case.direct_prose_validation_error.is_some()
                        || case.direct_prose_parse_error.is_some())
                    && (case.direct_prose_run_ok
                        || case.direct_prose_run_error.is_some()
                        || !case.direct_prose_validate_ok)
            })
            .count();

    format!("unclassified={unclassified}")
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

fn format_rate(value: f64) -> String {
    format!("{value:.3}")
}

fn format_number(value: f64) -> String {
    format!("{value:.1}")
}
