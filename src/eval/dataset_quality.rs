use std::collections::BTreeSet;

use serde::Serialize;

use super::compression::approximate_tokens;
use super::controller::glyph_ir_to_json_tool_plan;
use super::dataset::{ControllerDatasetExport, ControllerDatasetSplit};

const MIN_RECORDS: usize = 72;
const MIN_FAMILIES: usize = 9;
const REQUIRED_PROFILES: &[&str] = &["normal", "terse", "noisy", "adversarial"];
const MIN_REPAIR_RECORDS: usize = 8;
const MAX_AVERAGE_TARGET_TOKENS: f64 = 140.0;
const MAX_MAX_TARGET_TOKENS: usize = 260;
const MIN_AVERAGE_JSON_PLAN_COMPRESSION_RATIO: f64 = 1.5;
const MIN_MIN_JSON_PLAN_COMPRESSION_RATIO: f64 = 1.4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ControllerDatasetQualityDecision {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ControllerDatasetQualityStatus {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerDatasetQualityReport {
    pub decision: ControllerDatasetQualityDecision,
    pub passed: bool,
    pub metrics: ControllerDatasetQualityMetrics,
    pub checks: Vec<ControllerDatasetQualityCheck>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerDatasetQualityMetrics {
    #[serde(rename = "recordCount")]
    pub record_count: usize,
    #[serde(rename = "trainRecords")]
    pub train_records: usize,
    #[serde(rename = "validationRecords")]
    pub validation_records: usize,
    #[serde(rename = "familyCount")]
    pub family_count: usize,
    #[serde(rename = "profileCount")]
    pub profile_count: usize,
    #[serde(rename = "repairRecords")]
    pub repair_records: usize,
    #[serde(rename = "traceCompleteRecords")]
    pub trace_complete_records: usize,
    #[serde(rename = "finalOutputRecords")]
    pub final_output_records: usize,
    #[serde(rename = "averageTargetApproxTokens")]
    pub average_target_approx_tokens: f64,
    #[serde(rename = "maxTargetApproxTokens")]
    pub max_target_approx_tokens: usize,
    #[serde(rename = "averageJsonToolPlanApproxTokens")]
    pub average_json_tool_plan_approx_tokens: f64,
    #[serde(rename = "jsonToolPlanCompressionRatio")]
    pub json_tool_plan_compression_ratio: f64,
    #[serde(rename = "minJsonToolPlanCompressionRatio")]
    pub min_json_tool_plan_compression_ratio: f64,
    #[serde(rename = "averageTraceEvents")]
    pub average_trace_events: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerDatasetQualityCheck {
    pub id: String,
    pub status: ControllerDatasetQualityStatus,
    pub observed: String,
    pub required: String,
}

pub fn assess_controller_dataset_quality(
    export: &ControllerDatasetExport,
) -> ControllerDatasetQualityReport {
    let families = tag_values(export, "family:");
    let profiles = tag_values(export, "profile:");
    let repair_records = export
        .records
        .iter()
        .filter(|record| record.metadata.expects_repair_loop)
        .count();
    let trace_complete_records = export
        .records
        .iter()
        .filter(|record| {
            !record.target_trace.is_empty()
                && record
                    .target_trace
                    .iter()
                    .all(|event| event.duration_ms == 0)
                && record.metadata.trace_event_count == record.target_trace.len()
        })
        .count();
    let final_output_records = export
        .records
        .iter()
        .filter(|record| {
            !record.final_outputs.is_empty()
                && record.metadata.final_output_count == record.final_outputs.len()
        })
        .count();
    let target_tokens = export
        .records
        .iter()
        .map(|record| approximate_tokens(record.target_glyph.trim()))
        .collect::<Vec<_>>();
    let json_tool_plan_tokens = export
        .records
        .iter()
        .map(|record| {
            let plan = glyph_ir_to_json_tool_plan(&record.target_ir);
            serde_json::to_string(&plan)
                .map(|json| approximate_tokens(&json))
                .unwrap_or(0)
        })
        .collect::<Vec<_>>();
    let json_tool_plan_compression_ratios = target_tokens
        .iter()
        .zip(json_tool_plan_tokens.iter())
        .map(|(glyph_tokens, json_tokens)| *json_tokens as f64 / (*glyph_tokens).max(1) as f64)
        .collect::<Vec<_>>();
    let trace_lengths = export
        .records
        .iter()
        .map(|record| record.target_trace.len() as f64)
        .collect::<Vec<_>>();

    let metrics = ControllerDatasetQualityMetrics {
        record_count: export.record_count,
        train_records: export.train_records,
        validation_records: export.validation_records,
        family_count: families.len(),
        profile_count: profiles.len(),
        repair_records,
        trace_complete_records,
        final_output_records,
        average_target_approx_tokens: average_usize(&target_tokens),
        max_target_approx_tokens: target_tokens.iter().copied().max().unwrap_or(0),
        average_json_tool_plan_approx_tokens: average_usize(&json_tool_plan_tokens),
        json_tool_plan_compression_ratio: average_usize(&json_tool_plan_tokens)
            / average_usize(&target_tokens).max(1.0),
        min_json_tool_plan_compression_ratio: min_f64(&json_tool_plan_compression_ratios),
        average_trace_events: average_f64(&trace_lengths),
    };

    let checks = vec![
        check(
            "record_count",
            metrics.record_count >= MIN_RECORDS && metrics.record_count == export.records.len(),
            format!("{}/{}", metrics.record_count, export.records.len()),
            format!(">= {MIN_RECORDS} records and export count matches records length"),
        ),
        check(
            "split_present",
            metrics.train_records > 0
                && metrics.validation_records > 0
                && metrics.train_records + metrics.validation_records == metrics.record_count,
            format!(
                "train={}, validation={}, total={}",
                metrics.train_records, metrics.validation_records, metrics.record_count
            ),
            "nonempty train and validation splits covering all records".to_string(),
        ),
        check(
            "family_coverage",
            metrics.family_count >= MIN_FAMILIES,
            families.into_iter().collect::<Vec<_>>().join(","),
            format!(">= {MIN_FAMILIES} workflow families"),
        ),
        check(
            "profile_coverage",
            REQUIRED_PROFILES
                .iter()
                .all(|profile| profiles.contains(*profile)),
            profiles.into_iter().collect::<Vec<_>>().join(","),
            format!("profiles include {}", REQUIRED_PROFILES.join(",")),
        ),
        check(
            "repair_examples",
            metrics.repair_records >= MIN_REPAIR_RECORDS,
            metrics.repair_records.to_string(),
            format!(">= {MIN_REPAIR_RECORDS} bounded repair records"),
        ),
        check(
            "trace_completeness",
            metrics.trace_complete_records == metrics.record_count,
            format!(
                "{}/{}",
                metrics.trace_complete_records, metrics.record_count
            ),
            "every record has a normalized target trace matching metadata".to_string(),
        ),
        check(
            "final_outputs",
            metrics.final_output_records == metrics.record_count,
            format!("{}/{}", metrics.final_output_records, metrics.record_count),
            "every record has final outputs matching metadata".to_string(),
        ),
        check(
            "training_pair_integrity",
            training_pairs_are_valid(export),
            training_pair_observed(export),
            "assistant exactly equals target Glyph, user contains request, and assistant has no markdown fence".to_string(),
        ),
        check(
            "compact_targets",
            metrics.average_target_approx_tokens <= MAX_AVERAGE_TARGET_TOKENS
                && metrics.max_target_approx_tokens <= MAX_MAX_TARGET_TOKENS,
            format!(
                "avg={:.1}, max={}",
                metrics.average_target_approx_tokens, metrics.max_target_approx_tokens
            ),
            format!(
                "average <= {MAX_AVERAGE_TARGET_TOKENS:.1} approx tokens and max <= {MAX_MAX_TARGET_TOKENS}"
            ),
        ),
        check(
            "compact_vs_json_tool_plan",
            metrics.json_tool_plan_compression_ratio >= MIN_AVERAGE_JSON_PLAN_COMPRESSION_RATIO
                && metrics.min_json_tool_plan_compression_ratio
                    >= MIN_MIN_JSON_PLAN_COMPRESSION_RATIO,
            format!(
                "avgJson={:.1}, avgGlyph={:.1}, ratio={:.2}, minRatio={:.2}",
                metrics.average_json_tool_plan_approx_tokens,
                metrics.average_target_approx_tokens,
                metrics.json_tool_plan_compression_ratio,
                metrics.min_json_tool_plan_compression_ratio
            ),
            format!(
                "average generic JSON tool-plan tokens / Glyph tokens >= {MIN_AVERAGE_JSON_PLAN_COMPRESSION_RATIO:.1}, and every record >= {MIN_MIN_JSON_PLAN_COMPRESSION_RATIO:.1}"
            ),
        ),
    ];
    let passed = checks
        .iter()
        .all(|check| check.status == ControllerDatasetQualityStatus::Pass);

    ControllerDatasetQualityReport {
        decision: if passed {
            ControllerDatasetQualityDecision::Pass
        } else {
            ControllerDatasetQualityDecision::Fail
        },
        passed,
        metrics,
        checks,
    }
}

fn tag_values(export: &ControllerDatasetExport, prefix: &str) -> BTreeSet<String> {
    export
        .records
        .iter()
        .flat_map(|record| &record.tags)
        .filter_map(|tag| tag.strip_prefix(prefix).map(ToString::to_string))
        .collect()
}

fn training_pairs_are_valid(export: &ControllerDatasetExport) -> bool {
    export.records.iter().all(|record| {
        record.training_example.user.contains(&record.request)
            && record.training_example.assistant == record.target_glyph
            && !record.training_example.assistant.contains("```")
            && !record.target_glyph.trim().is_empty()
            && matches!(
                record.split,
                ControllerDatasetSplit::Train | ControllerDatasetSplit::Validation
            )
    })
}

fn training_pair_observed(export: &ControllerDatasetExport) -> String {
    let valid = export
        .records
        .iter()
        .filter(|record| {
            record.training_example.user.contains(&record.request)
                && record.training_example.assistant == record.target_glyph
                && !record.training_example.assistant.contains("```")
                && !record.target_glyph.trim().is_empty()
        })
        .count();

    format!("{valid}/{}", export.record_count)
}

fn check(
    id: &str,
    passed: bool,
    observed: String,
    required: String,
) -> ControllerDatasetQualityCheck {
    ControllerDatasetQualityCheck {
        id: id.to_string(),
        status: if passed {
            ControllerDatasetQualityStatus::Pass
        } else {
            ControllerDatasetQualityStatus::Fail
        },
        observed,
        required,
    }
}

fn average_usize(values: &[usize]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }

    values.iter().sum::<usize>() as f64 / values.len() as f64
}

fn average_f64(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }

    values.iter().sum::<f64>() / values.len() as f64
}

fn min_f64(values: &[f64]) -> f64 {
    values.iter().copied().reduce(f64::min).unwrap_or(0.0)
}
