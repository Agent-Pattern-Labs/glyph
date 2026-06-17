use serde::Serialize;

use super::controller::{
    ControllerEvalCaseResult, ControllerEvalModelSummary, ControllerParameterClass,
    summarize_controller_eval_by_model,
};
use super::gate::{ControllerGateReport, evaluate_controller_gate};

pub const CONTROLLER_BENCHMARK_REPORT_VERSION: &str = "glyph-controller-benchmark-report/0.1";

#[derive(Debug, Clone, Serialize)]
pub struct ControllerBenchmarkReport {
    pub version: String,
    pub passed: bool,
    pub summary: String,
    #[serde(rename = "caseRows")]
    pub case_rows: usize,
    #[serde(rename = "liveCaseRows")]
    pub live_case_rows: usize,
    #[serde(rename = "targetCaseRows")]
    pub target_case_rows: usize,
    #[serde(rename = "gatePassed")]
    pub gate_passed: bool,
    pub comparisons: Vec<ControllerBenchmarkComparison>,
    #[serde(rename = "modelSummaries")]
    pub model_summaries: Vec<ControllerEvalModelSummary>,
    pub gate: ControllerGateReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerBenchmarkComparison {
    pub id: String,
    pub status: ControllerBenchmarkComparisonStatus,
    pub direction: ControllerBenchmarkComparisonDirection,
    #[serde(rename = "targetValue")]
    pub target_value: Option<f64>,
    #[serde(rename = "baselineValue")]
    pub baseline_value: Option<f64>,
    pub delta: Option<f64>,
    pub ratio: Option<f64>,
    pub observed: String,
    pub required: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ControllerBenchmarkComparisonStatus {
    Pass,
    Fail,
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ControllerBenchmarkComparisonDirection {
    HigherIsBetter,
    LowerIsBetter,
}

pub fn controller_benchmark_report(
    cases: &[ControllerEvalCaseResult],
) -> ControllerBenchmarkReport {
    let gate = evaluate_controller_gate(cases);
    let metrics = &gate.metrics;
    let target_present = gate.target_case_rows > 0;

    let mut comparisons = vec![
        higher_is_better(
            "one_b_constrained_vs_one_b_plain_trace_rate",
            target_present.then_some(metrics.target_successful_trace_rate),
            metrics.one_b_plain_successful_trace_rate,
            |target, plain| target - plain >= 0.20 || plain >= 0.90,
            "1B constrained Glyph successful trace rate lift >= 0.20 over 1B plain, or plain is already >= 0.90",
        ),
        higher_is_better(
            "one_b_constrained_vs_generic_json_trace_rate",
            target_present.then_some(metrics.target_successful_trace_rate),
            target_present.then_some(metrics.target_json_tool_plan_successful_trace_rate),
            |target, baseline| target > baseline,
            "1B constrained Glyph successful trace rate > generic JSON tool-plan successful trace rate",
        ),
        higher_is_better(
            "one_b_constrained_vs_direct_prose_trace_rate",
            target_present.then_some(metrics.target_successful_trace_rate),
            target_present.then_some(metrics.target_direct_prose_successful_trace_rate),
            |target, baseline| target > baseline,
            "1B constrained Glyph successful trace rate > direct-prose successful trace rate",
        ),
        higher_is_better(
            "one_b_constrained_vs_larger_plain_trace_rate",
            target_present.then_some(metrics.target_successful_trace_rate),
            metrics.larger_plain_successful_trace_rate,
            |target, baseline| target >= baseline,
            "1B constrained Glyph successful trace rate >= 3B/7B/frontier plain successful trace rate",
        ),
        lower_is_better(
            "one_b_glyph_vs_own_json_output_tokens",
            positive(target_present, metrics.target_average_output_tokens),
            positive(
                target_present,
                metrics.target_average_json_tool_plan_output_tokens,
            ),
            |target, baseline| target < baseline,
            "1B constrained Glyph output tokens < same-row generic JSON tool-plan output tokens",
        ),
        lower_is_better(
            "one_b_glyph_vs_larger_json_output_tokens",
            positive(target_present, metrics.target_average_output_tokens),
            metrics.larger_plain_average_json_tool_plan_output_tokens,
            |target, baseline| target < baseline,
            "1B constrained Glyph output tokens < larger models' generic JSON tool-plan output tokens",
        ),
    ];
    comparisons.extend(
        metrics
            .larger_plain_successful_trace_rates
            .iter()
            .map(|rate| {
                higher_is_better(
                    &format!(
                        "one_b_constrained_vs_{}_plain_trace_rate",
                        parameter_class_id(rate.parameter_class)
                    ),
                    target_present.then_some(metrics.target_successful_trace_rate),
                    rate.successful_trace_rate,
                    |target, baseline| target >= baseline,
                    &format!(
                        "1B constrained Glyph successful trace rate >= {} plain successful trace rate",
                        rate.parameter_class.as_str()
                    ),
                )
            }),
    );
    let comparisons_pass = comparisons
        .iter()
        .all(|comparison| comparison.status == ControllerBenchmarkComparisonStatus::Pass);
    let passed = gate.passed && comparisons_pass;

    ControllerBenchmarkReport {
        version: CONTROLLER_BENCHMARK_REPORT_VERSION.to_string(),
        passed,
        summary: if passed {
            "Benchmark report supports the tiny-controller best-in-lane claim.".to_string()
        } else {
            "Benchmark report does not support the tiny-controller best-in-lane claim.".to_string()
        },
        case_rows: cases.len(),
        live_case_rows: gate.live_case_rows,
        target_case_rows: gate.target_case_rows,
        gate_passed: gate.passed,
        comparisons,
        model_summaries: summarize_controller_eval_by_model(cases),
        gate,
    }
}

fn higher_is_better(
    id: &str,
    target: Option<f64>,
    baseline: Option<f64>,
    passes: impl Fn(f64, f64) -> bool,
    required: &str,
) -> ControllerBenchmarkComparison {
    comparison(
        id,
        ControllerBenchmarkComparisonDirection::HigherIsBetter,
        target,
        baseline,
        passes,
        required,
    )
}

fn lower_is_better(
    id: &str,
    target: Option<f64>,
    baseline: Option<f64>,
    passes: impl Fn(f64, f64) -> bool,
    required: &str,
) -> ControllerBenchmarkComparison {
    comparison(
        id,
        ControllerBenchmarkComparisonDirection::LowerIsBetter,
        target,
        baseline,
        passes,
        required,
    )
}

fn comparison(
    id: &str,
    direction: ControllerBenchmarkComparisonDirection,
    target: Option<f64>,
    baseline: Option<f64>,
    passes: impl Fn(f64, f64) -> bool,
    required: &str,
) -> ControllerBenchmarkComparison {
    let (status, delta, ratio, observed) = match (target, baseline) {
        (Some(target), Some(baseline)) => {
            let delta = target - baseline;
            let ratio = if baseline == 0.0 {
                None
            } else {
                Some(target / baseline)
            };
            (
                if passes(target, baseline) {
                    ControllerBenchmarkComparisonStatus::Pass
                } else {
                    ControllerBenchmarkComparisonStatus::Fail
                },
                Some(delta),
                ratio,
                format!(
                    "target={}, baseline={}, delta={}",
                    format_number(target),
                    format_number(baseline),
                    format_number(delta)
                ),
            )
        }
        _ => (
            ControllerBenchmarkComparisonStatus::Missing,
            None,
            None,
            "missing target or baseline evidence".to_string(),
        ),
    };

    ControllerBenchmarkComparison {
        id: id.to_string(),
        status,
        direction,
        target_value: target,
        baseline_value: baseline,
        delta,
        ratio,
        observed,
        required: required.to_string(),
    }
}

fn positive(enabled: bool, value: f64) -> Option<f64> {
    (enabled && value > 0.0).then_some(value)
}

fn format_number(value: f64) -> String {
    format!("{value:.3}")
}

fn parameter_class_id(parameter_class: ControllerParameterClass) -> &'static str {
    match parameter_class {
        ControllerParameterClass::OneB => "1b",
        ControllerParameterClass::ThreeB => "3b",
        ControllerParameterClass::SevenB => "7b",
        ControllerParameterClass::Frontier => "frontier",
    }
}
