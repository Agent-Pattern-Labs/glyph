use serde::Serialize;
use serde_json::Value;

use super::conformance::{GlyphConformanceReport, glyph_conformance_report};
use super::controller::ControllerEvalCaseResult;
use super::coverage::{ControllerCoverageReport, controller_eval_coverage};
use super::curriculum::{
    ControllerCurriculumQualityReport, assess_controller_curriculum_quality,
    export_controller_curriculum,
};
use super::dataset::export_controller_dataset;
use super::dataset_quality::{ControllerDatasetQualityReport, assess_controller_dataset_quality};
use super::fingerprint::{ControllerEvalFingerprint, controller_eval_fingerprint};
use super::gate::{ControllerGateReport, evaluate_controller_gate};
use super::robustness::{ControllerRobustnessReport, evaluate_controller_robustness};
use super::verify::{ControllerRunVerificationReport, verify_controller_run};

const BENCHMARK_GATE_DOC: &str = include_str!("../../docs/benchmark-gate.md");
const ADJACENT_SYSTEMS_DOC: &str = include_str!("../../docs/adjacent-systems.md");
const CONTROLLER_FINGERPRINT_LOCK: &str =
    include_str!("../../spec/controller-fingerprint.lock.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ControllerClaimAuditDecision {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ControllerClaimAuditStatus {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerClaimAuditReport {
    pub decision: ControllerClaimAuditDecision,
    pub passed: bool,
    #[serde(rename = "claimReady")]
    pub claim_ready: bool,
    pub summary: String,
    pub checks: Vec<ControllerClaimAuditCheck>,
    pub fingerprint: ControllerEvalFingerprint,
    pub dataset: ControllerClaimDatasetSummary,
    #[serde(rename = "datasetQuality", skip_serializing_if = "Option::is_none")]
    pub dataset_quality: Option<ControllerDatasetQualityReport>,
    #[serde(rename = "curriculumQuality", skip_serializing_if = "Option::is_none")]
    pub curriculum_quality: Option<ControllerCurriculumQualityReport>,
    pub robustness: ControllerRobustnessReport,
    pub conformance: GlyphConformanceReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification: Option<ControllerRunVerificationReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage: Option<ControllerCoverageReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gate: Option<ControllerGateReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerClaimAuditCheck {
    pub id: String,
    pub status: ControllerClaimAuditStatus,
    pub observed: String,
    pub required: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerClaimDatasetSummary {
    pub version: String,
    #[serde(rename = "recordCount")]
    pub record_count: usize,
    #[serde(rename = "trainRecords")]
    pub train_records: usize,
    #[serde(rename = "validationRecords")]
    pub validation_records: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct ControllerClaimAuditInput<'a> {
    pub cases: Option<&'a [ControllerEvalCaseResult]>,
    pub manifest: Option<&'a Value>,
    pub jsonl_path: Option<&'a str>,
}

pub fn audit_controller_claim(input: ControllerClaimAuditInput<'_>) -> ControllerClaimAuditReport {
    let fingerprint = controller_eval_fingerprint();
    let fingerprint_value = serde_json::to_value(&fingerprint).ok();
    let fingerprint_lock = serde_json::from_str::<Value>(CONTROLLER_FINGERPRINT_LOCK);
    let fingerprint_lock_matches = fingerprint_lock
        .as_ref()
        .ok()
        .zip(fingerprint_value.as_ref())
        .is_some_and(|(locked, current)| locked == current);
    let fingerprint_lock_observed = match fingerprint_lock.as_ref() {
        Ok(lock) => format!(
            "locked={}, current={}",
            lock.get("overallSha256")
                .and_then(Value::as_str)
                .unwrap_or("missing"),
            fingerprint.overall_sha256
        ),
        Err(error) => format!("invalid lock: {error}"),
    };
    let dataset_export = export_controller_dataset(Default::default());
    let dataset_quality = dataset_export
        .as_ref()
        .ok()
        .map(assess_controller_dataset_quality);
    let curriculum_export = export_controller_curriculum(Default::default());
    let curriculum_quality = curriculum_export
        .as_ref()
        .ok()
        .map(assess_controller_curriculum_quality);
    let robustness = evaluate_controller_robustness();
    let conformance = glyph_conformance_report();
    let dataset = match &dataset_export {
        Ok(export) => ControllerClaimDatasetSummary {
            version: export.version.clone(),
            record_count: export.record_count,
            train_records: export.train_records,
            validation_records: export.validation_records,
        },
        Err(_) => ControllerClaimDatasetSummary {
            version: "unavailable".to_string(),
            record_count: 0,
            train_records: 0,
            validation_records: 0,
        },
    };

    let verification = match (input.cases, input.manifest, input.jsonl_path) {
        (Some(cases), Some(manifest), Some(jsonl_path)) => {
            Some(verify_controller_run(cases, manifest, jsonl_path))
        }
        _ => None,
    };
    let coverage = input.cases.map(controller_eval_coverage);
    let gate = input.cases.map(evaluate_controller_gate);

    let checks = vec![
        check(
            "spec_fingerprint",
            fingerprint.eval_corpus.case_count == 72
                && has_artifact(&fingerprint, "glyph.gbnf")
                && has_artifact(&fingerprint, "controller-output.schema.json")
                && has_artifact(&fingerprint, "generic-tool-plan.schema.json")
                && has_artifact(&fingerprint, "glyph-ir.schema.json")
                && fingerprint.request_contract.request_count == 72 * 3 * 2 * 3
                && fingerprint.request_contract.sha256.len() == 64,
            format!(
                "cases={}, artifacts={}, requestBodies={}",
                fingerprint.eval_corpus.case_count,
                fingerprint.spec_artifacts.len(),
                fingerprint.request_contract.request_count
            ),
            "72-case corpus, canonical grammar/schema artifacts, and OpenAI-compatible request bodies are fingerprinted".to_string(),
        ),
        check(
            "fingerprint_lock",
            fingerprint_lock_matches,
            fingerprint_lock_observed,
            "committed controller fingerprint lock exactly matches the current grammar, schemas, eval corpus, and request contract".to_string(),
        ),
        check(
            "controller_dataset",
            dataset_export.is_ok()
                && dataset_quality
                    .as_ref()
                    .is_some_and(|quality| quality.passed)
                && dataset.record_count == fingerprint.eval_corpus.case_count,
            format!(
                "records={}, train={}, validation={}, quality={}",
                dataset.record_count,
                dataset.train_records,
                dataset.validation_records,
                dataset_quality
                    .as_ref()
                    .map(|quality| quality.passed.to_string())
                    .unwrap_or_else(|| "missing".to_string())
            ),
            "deterministic dataset export covers the fingerprinted eval corpus and passes the dataset quality gate".to_string(),
        ),
        check(
            "controller_curriculum",
            curriculum_export.is_ok()
                && curriculum_quality
                    .as_ref()
                    .is_some_and(|quality| quality.passed),
            curriculum_quality
                .as_ref()
                .map(|quality| {
                    format!(
                        "records={}, cases={}, quality={}",
                        quality.metrics.record_count, quality.metrics.case_count, quality.passed
                    )
                })
                .unwrap_or_else(|| "missing".to_string()),
            "controller curriculum includes positive, repair, and rejected-negative records and passes the curriculum quality gate".to_string(),
        ),
        check(
            "controller_robustness",
            robustness.passed,
            format!(
                "mutations={}, rejected={}, accepted={}",
                robustness.metrics.mutation_count,
                robustness.metrics.rejected_mutations,
                robustness.metrics.accepted_mutation_count
            ),
            "parser and semantic validation reject deterministic invalid controller-output mutations".to_string(),
        ),
        check(
            "glyph_conformance",
            conformance.passed,
            format!(
                "examples={}, parse={}, validate={}, run={}",
                conformance.example_count,
                conformance.parse_passed,
                conformance.validation_passed,
                conformance.run_passed
            ),
            "all public Glyph examples parse, validate, and run with the mock harness".to_string(),
        ),
        check(
            "benchmark_gate_documented",
            BENCHMARK_GATE_DOC.contains("Best-In-Lane Gate")
                && BENCHMARK_GATE_DOC.contains("Do not claim best-in-lane"),
            "docs/benchmark-gate.md".to_string(),
            "benchmark gate documents the claim threshold and no-claim rule".to_string(),
        ),
        check(
            "adjacent_systems_documented",
            ADJACENT_SYSTEMS_DOC.contains("Evidence Standard")
                && ADJACENT_SYSTEMS_DOC.contains("LMQL")
                && ADJACENT_SYSTEMS_DOC.contains("LangGraph"),
            "docs/adjacent-systems.md".to_string(),
            "adjacent systems and direct-competitor evidence standard are documented".to_string(),
        ),
        check(
            "live_jsonl_supplied",
            input.cases.is_some_and(|cases| !cases.is_empty()),
            input
                .cases
                .map(|cases| cases.len().to_string())
                .unwrap_or_else(|| "missing".to_string()),
            "live OpenAI-compatible or offline-response JSONL rows are supplied for claim audit"
                .to_string(),
        ),
        check(
            "manifest_supplied",
            input.manifest.is_some() && input.jsonl_path.is_some(),
            match (input.manifest.is_some(), input.jsonl_path) {
                (true, Some(path)) => format!("manifest=true, jsonlPath={path}"),
                (true, None) => "manifest=true, jsonlPath=missing".to_string(),
                (false, Some(path)) => format!("manifest=false, jsonlPath={path}"),
                (false, None) => "missing".to_string(),
            },
            "completed run or merged manifest is supplied with the JSONL path".to_string(),
        ),
        check(
            "evidence_git_provenance",
            input.manifest.is_some_and(manifest_has_git_provenance),
            input
                .manifest
                .map(observed_git_provenance)
                .unwrap_or_else(|| "missing".to_string()),
            "claim evidence manifest records gitCommit and gitTreeDirty".to_string(),
        ),
        check(
            "run_verification",
            verification.as_ref().is_some_and(|report| report.passed),
            verification
                .as_ref()
                .map(|report| format!("passed={}, rows={}", report.passed, report.case_rows))
                .unwrap_or_else(|| "missing".to_string()),
            "verify-controller-run passes for the supplied JSONL and manifest".to_string(),
        ),
        check(
            "coverage_complete",
            coverage
                .as_ref()
                .is_some_and(|report| report.coverage_complete),
            coverage
                .as_ref()
                .map(|report| {
                    format!(
                        "complete={}, targetRows={}, missingTargetRows={}, missingComparisonRows={}",
                        report.coverage_complete,
                        report.target_rows,
                        report.missing_target_rows,
                        report.missing_comparison_rows
                    )
                })
                .unwrap_or_else(|| "missing".to_string()),
            "coverage includes all target cases, buckets, prompt modes, families, profiles, and bucket-by-prompt comparison rows".to_string(),
        ),
        check(
            "benchmark_gate",
            gate.as_ref().is_some_and(|report| report.passed),
            gate.as_ref()
                .map(|report| {
                    format!(
                        "passed={}, targetRows={}, liveRows={}",
                        report.passed, report.target_case_rows, report.live_case_rows
                    )
                })
                .unwrap_or_else(|| "missing".to_string()),
            "gate-controller passes on the supplied live results".to_string(),
        ),
    ];

    let passed = checks
        .iter()
        .all(|check| check.status == ControllerClaimAuditStatus::Pass);

    ControllerClaimAuditReport {
        decision: if passed {
            ControllerClaimAuditDecision::Pass
        } else {
            ControllerClaimAuditDecision::Fail
        },
        passed,
        claim_ready: passed,
        summary: if passed {
            "Claim-ready: supplied evidence passes verification, coverage, and the benchmark gate."
                .to_string()
        } else {
            "Not claim-ready: missing or failing live evidence prevents a best-in-lane claim."
                .to_string()
        },
        checks,
        fingerprint,
        dataset,
        dataset_quality,
        curriculum_quality,
        robustness,
        conformance,
        verification,
        coverage,
        gate,
    }
}

fn check(id: &str, passed: bool, observed: String, required: String) -> ControllerClaimAuditCheck {
    ControllerClaimAuditCheck {
        id: id.to_string(),
        status: if passed {
            ControllerClaimAuditStatus::Pass
        } else {
            ControllerClaimAuditStatus::Fail
        },
        observed,
        required,
    }
}

fn has_artifact(fingerprint: &ControllerEvalFingerprint, name: &str) -> bool {
    fingerprint
        .spec_artifacts
        .iter()
        .any(|artifact| artifact.name == name && artifact.bytes > 0 && artifact.sha256.len() == 64)
}

fn manifest_has_git_provenance(manifest: &Value) -> bool {
    manifest
        .get("gitCommit")
        .and_then(Value::as_str)
        .is_some_and(|commit| !commit.trim().is_empty())
        && manifest
            .get("gitTreeDirty")
            .and_then(Value::as_bool)
            .is_some()
}

fn observed_git_provenance(manifest: &Value) -> String {
    format!(
        "gitCommit={}, gitTreeDirty={}",
        manifest
            .get("gitCommit")
            .and_then(Value::as_str)
            .unwrap_or("missing"),
        manifest
            .get("gitTreeDirty")
            .and_then(Value::as_bool)
            .map(|dirty| dirty.to_string())
            .unwrap_or_else(|| "missing".to_string())
    )
}
