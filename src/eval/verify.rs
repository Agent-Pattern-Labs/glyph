use std::collections::BTreeSet;

use serde::Serialize;
use serde_json::Value;

use super::controller::{ControllerAdapterMode, ControllerEvalCaseResult};
use super::fingerprint::controller_eval_fingerprint;
use super::replay::{ControllerReplayReport, replay_controller_run};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ControllerRunVerificationDecision {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ControllerRunVerificationStatus {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerRunVerificationReport {
    pub decision: ControllerRunVerificationDecision,
    pub passed: bool,
    #[serde(rename = "caseRows")]
    pub case_rows: usize,
    pub checks: Vec<ControllerRunVerificationCheck>,
    pub replay: ControllerReplayReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerRunVerificationCheck {
    pub id: String,
    pub status: ControllerRunVerificationStatus,
    pub observed: String,
    pub required: String,
}

pub fn verify_controller_run(
    cases: &[ControllerEvalCaseResult],
    manifest: &Value,
    jsonl_path: &str,
) -> ControllerRunVerificationReport {
    let replay = replay_controller_run(cases);
    let checks = vec![
        check(
            "manifest_version",
            manifest_string(manifest, &["manifestVersion"]).as_deref() == Some("0.1"),
            manifest_string(manifest, &["manifestVersion"]).unwrap_or_else(|| "missing".into()),
            "manifestVersion=0.1".to_string(),
        ),
        check(
            "manifest_kind",
            matches!(
                manifest_string(manifest, &["manifestKind"]).as_deref(),
                Some("run") | Some("merged")
            ),
            manifest_string(manifest, &["manifestKind"]).unwrap_or_else(|| "missing".into()),
            "manifestKind=run or manifestKind=merged".to_string(),
        ),
        check(
            "manifest_completed",
            manifest_string(manifest, &["runStatus"]).as_deref() == Some("completed"),
            manifest_string(manifest, &["runStatus"]).unwrap_or_else(|| "missing".into()),
            "runStatus=completed".to_string(),
        ),
        check(
            "fingerprint_current",
            manifest_string(manifest, &["fingerprint", "overallSha256"]).as_deref()
                == Some(controller_eval_fingerprint().overall_sha256.as_str()),
            manifest_string(manifest, &["fingerprint", "overallSha256"])
                .unwrap_or_else(|| "missing".into()),
            format!(
                "current fingerprint {}",
                controller_eval_fingerprint().overall_sha256
            ),
        ),
        check(
            "jsonl_artifact_path",
            manifest_string(manifest, &["config", "artifacts", "jsonlPath"]).as_deref()
                == Some(jsonl_path),
            manifest_string(manifest, &["config", "artifacts", "jsonlPath"])
                .unwrap_or_else(|| "missing".into()),
            jsonl_path.to_string(),
        ),
        check(
            "offline_prompt_bundle_provenance",
            offline_prompt_bundle_provenance_recorded(cases, manifest),
            offline_prompt_bundle_provenance_observed(cases, manifest),
            "offline-response rows require emitPromptsPath, promptBundleOverallSha256, and promptBundleManifestSha256 in the manifest".to_string(),
        ),
        check(
            "offline_response_bundle_provenance",
            offline_response_bundle_provenance_recorded(cases, manifest),
            offline_response_bundle_provenance_observed(cases, manifest),
            "offline-response rows require responseBundlePath, responseBundleFileCount, responseBundleBytes, and responseBundleSha256 in the manifest".to_string(),
        ),
        check(
            "report_case_rows",
            manifest_usize(manifest, &["reportSummary", "caseRows"]) == Some(cases.len()),
            manifest_usize(manifest, &["reportSummary", "caseRows"])
                .map(|value| value.to_string())
                .unwrap_or_else(|| "missing".into()),
            cases.len().to_string(),
        ),
        check(
            "coverage_case_rows",
            manifest_usize(manifest, &["coverage", "caseRows"]) == Some(cases.len()),
            manifest_usize(manifest, &["coverage", "caseRows"])
                .map(|value| value.to_string())
                .unwrap_or_else(|| "missing".into()),
            cases.len().to_string(),
        ),
        check(
            "selected_case_count",
            selected_case_ids(manifest).len()
                == manifest_usize(manifest, &["config", "selectedCaseCount"]).unwrap_or(usize::MAX),
            selected_case_ids(manifest).len().to_string(),
            manifest_usize(manifest, &["config", "selectedCaseCount"])
                .map(|value| value.to_string())
                .unwrap_or_else(|| "selectedCaseCount present".into()),
        ),
        check(
            "selected_case_ids",
            selected_case_ids_match_rows(manifest, cases),
            observed_case_ids(cases)
                .into_iter()
                .collect::<Vec<_>>()
                .join(","),
            selected_case_ids(manifest)
                .into_iter()
                .collect::<Vec<_>>()
                .join(","),
        ),
        check(
            "expected_row_count",
            expected_row_count(manifest) == Some(cases.len()),
            cases.len().to_string(),
            expected_row_count(manifest)
                .map(|value| value.to_string())
                .unwrap_or_else(|| "selected cases * models * prompt modes".into()),
        ),
        check(
            "models_covered",
            observed_models(cases) == configured_models(manifest),
            observed_models(cases)
                .into_iter()
                .collect::<Vec<_>>()
                .join(","),
            configured_models(manifest)
                .into_iter()
                .collect::<Vec<_>>()
                .join(","),
        ),
        check(
            "prompt_modes_covered",
            observed_prompt_modes(cases) == configured_prompt_modes(manifest),
            observed_prompt_modes(cases)
                .into_iter()
                .collect::<Vec<_>>()
                .join(","),
            configured_prompt_modes(manifest)
                .into_iter()
                .collect::<Vec<_>>()
                .join(","),
        ),
        check(
            "secret_omitted",
            manifest_bool(manifest, &["security", "apiKeyValueOmitted"]) == Some(true),
            manifest_bool(manifest, &["security", "apiKeyValueOmitted"])
                .map(|value| value.to_string())
                .unwrap_or_else(|| "missing".into()),
            "true".to_string(),
        ),
        check(
            "real_shell_disabled",
            manifest_bool(manifest, &["security", "realShellRunEnabled"]) == Some(false),
            manifest_bool(manifest, &["security", "realShellRunEnabled"])
                .map(|value| value.to_string())
                .unwrap_or_else(|| "missing".into()),
            "false".to_string(),
        ),
        check(
            "source_manifests_verified",
            source_manifests_verified(manifest),
            source_manifest_observed(manifest),
            "normal run or merged run with all sourceManifests verified=true".to_string(),
        ),
        check(
            "replay_consistency",
            replay.passed,
            format!(
                "caseRows={}, failureCount={}",
                replay.case_rows, replay.failure_count
            ),
            "stored generated outputs replay to the recorded parse, validation, run, trace, and baseline metrics".to_string(),
        ),
    ];
    let passed = checks
        .iter()
        .all(|check| check.status == ControllerRunVerificationStatus::Pass);

    ControllerRunVerificationReport {
        decision: if passed {
            ControllerRunVerificationDecision::Pass
        } else {
            ControllerRunVerificationDecision::Fail
        },
        passed,
        case_rows: cases.len(),
        checks,
        replay,
    }
}

fn check(
    id: &str,
    passed: bool,
    observed: String,
    required: String,
) -> ControllerRunVerificationCheck {
    ControllerRunVerificationCheck {
        id: id.to_string(),
        status: if passed {
            ControllerRunVerificationStatus::Pass
        } else {
            ControllerRunVerificationStatus::Fail
        },
        observed,
        required,
    }
}

fn manifest_string(manifest: &Value, path: &[&str]) -> Option<String> {
    manifest_value(manifest, path)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn manifest_usize(manifest: &Value, path: &[&str]) -> Option<usize> {
    manifest_value(manifest, path)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
}

fn manifest_bool(manifest: &Value, path: &[&str]) -> Option<bool> {
    manifest_value(manifest, path).and_then(Value::as_bool)
}

fn manifest_value<'a>(manifest: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut value = manifest;
    for segment in path {
        value = value.get(*segment)?;
    }
    Some(value)
}

fn selected_case_ids(manifest: &Value) -> BTreeSet<String> {
    manifest_value(manifest, &["config", "selectedCaseIds"])
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToString::to_string)
        .collect()
}

fn selected_case_ids_match_rows(manifest: &Value, cases: &[ControllerEvalCaseResult]) -> bool {
    let selected = selected_case_ids(manifest);
    let observed = observed_case_ids(cases);
    !selected.is_empty()
        && !observed.is_empty()
        && selected.iter().all(|case_id| observed.contains(case_id))
        && observed.iter().all(|case_id| selected.contains(case_id))
}

fn observed_case_ids(cases: &[ControllerEvalCaseResult]) -> BTreeSet<String> {
    cases.iter().map(|case| case.case_id.clone()).collect()
}

fn expected_row_count(manifest: &Value) -> Option<usize> {
    let selected = manifest_usize(manifest, &["config", "selectedCaseCount"])?;
    let models = manifest_value(manifest, &["config", "models"])?
        .as_array()?
        .len();
    let prompt_modes = manifest_value(manifest, &["config", "promptModes"])?
        .as_array()?
        .len();
    Some(selected * models * prompt_modes)
}

fn observed_models(cases: &[ControllerEvalCaseResult]) -> BTreeSet<String> {
    cases
        .iter()
        .map(|case| format!("{}:{}", case.parameter_class.as_str(), case.model_id))
        .collect()
}

fn configured_models(manifest: &Value) -> BTreeSet<String> {
    manifest_value(manifest, &["config", "models"])
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|model| {
            let parameter_class = model.get("parameterClass")?.as_str()?;
            let model_id = model.get("modelId")?.as_str()?;
            Some(format!("{parameter_class}:{model_id}"))
        })
        .collect()
}

fn observed_prompt_modes(cases: &[ControllerEvalCaseResult]) -> BTreeSet<String> {
    cases
        .iter()
        .map(|case| case.prompt_mode.as_str().to_string())
        .collect()
}

fn has_offline_response_rows(cases: &[ControllerEvalCaseResult]) -> bool {
    cases
        .iter()
        .any(|case| case.adapter_mode == ControllerAdapterMode::OfflineResponses)
}

fn offline_prompt_bundle_provenance_recorded(
    cases: &[ControllerEvalCaseResult],
    manifest: &Value,
) -> bool {
    if !has_offline_response_rows(cases) {
        return true;
    }
    if manifest_string(manifest, &["manifestKind"]).as_deref() == Some("merged") {
        return source_manifests_verified(manifest);
    }

    manifest_string(manifest, &["config", "artifacts", "emitPromptsPath"]).is_some()
        && manifest_string(
            manifest,
            &["config", "artifacts", "promptBundleOverallSha256"],
        )
        .is_some_and(|value| value.len() == 64)
        && manifest_string(
            manifest,
            &["config", "artifacts", "promptBundleManifestSha256"],
        )
        .is_some_and(|value| value.len() == 64)
}

fn offline_prompt_bundle_provenance_observed(
    cases: &[ControllerEvalCaseResult],
    manifest: &Value,
) -> String {
    if !has_offline_response_rows(cases) {
        return "no offline-response rows".to_string();
    }
    if manifest_string(manifest, &["manifestKind"]).as_deref() == Some("merged") {
        return source_manifest_observed(manifest);
    }

    format!(
        "emitPromptsPath={}, promptBundleOverallSha256={}, promptBundleManifestSha256={}",
        manifest_string(manifest, &["config", "artifacts", "emitPromptsPath"])
            .unwrap_or_else(|| "missing".to_string()),
        manifest_string(
            manifest,
            &["config", "artifacts", "promptBundleOverallSha256"]
        )
        .unwrap_or_else(|| "missing".to_string()),
        manifest_string(
            manifest,
            &["config", "artifacts", "promptBundleManifestSha256"]
        )
        .unwrap_or_else(|| "missing".to_string())
    )
}

fn offline_response_bundle_provenance_recorded(
    cases: &[ControllerEvalCaseResult],
    manifest: &Value,
) -> bool {
    if !has_offline_response_rows(cases) {
        return true;
    }
    if manifest_string(manifest, &["manifestKind"]).as_deref() == Some("merged") {
        return source_manifests_verified(manifest);
    }

    manifest_string(manifest, &["config", "artifacts", "responseBundlePath"]).is_some()
        && manifest_usize(
            manifest,
            &["config", "artifacts", "responseBundleFileCount"],
        )
        .is_some_and(|value| value > 0)
        && manifest_usize(manifest, &["config", "artifacts", "responseBundleBytes"])
            .is_some_and(|value| value > 0)
        && manifest_string(manifest, &["config", "artifacts", "responseBundleSha256"])
            .is_some_and(|value| value.len() == 64)
}

fn offline_response_bundle_provenance_observed(
    cases: &[ControllerEvalCaseResult],
    manifest: &Value,
) -> String {
    if !has_offline_response_rows(cases) {
        return "no offline-response rows".to_string();
    }
    if manifest_string(manifest, &["manifestKind"]).as_deref() == Some("merged") {
        return source_manifest_observed(manifest);
    }

    format!(
        "responseBundlePath={}, responseBundleFileCount={}, responseBundleBytes={}, responseBundleSha256={}",
        manifest_string(manifest, &["config", "artifacts", "responseBundlePath"])
            .unwrap_or_else(|| "missing".to_string()),
        manifest_usize(
            manifest,
            &["config", "artifacts", "responseBundleFileCount"]
        )
        .map(|value| value.to_string())
        .unwrap_or_else(|| "missing".to_string()),
        manifest_usize(manifest, &["config", "artifacts", "responseBundleBytes"])
            .map(|value| value.to_string())
            .unwrap_or_else(|| "missing".to_string()),
        manifest_string(manifest, &["config", "artifacts", "responseBundleSha256"])
            .unwrap_or_else(|| "missing".to_string())
    )
}

fn configured_prompt_modes(manifest: &Value) -> BTreeSet<String> {
    manifest_value(manifest, &["config", "promptModes"])
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToString::to_string)
        .collect()
}

fn source_manifests_verified(manifest: &Value) -> bool {
    let Some(kind) = manifest_string(manifest, &["manifestKind"]) else {
        return false;
    };
    let sources = manifest_value(manifest, &["sourceManifests"])
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    if kind == "run" {
        return sources.iter().all(|source| {
            source
                .get("verified")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        });
    }

    kind == "merged"
        && !sources.is_empty()
        && sources.iter().all(|source| {
            source
                .get("verified")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
}

fn source_manifest_observed(manifest: &Value) -> String {
    let kind = manifest_string(manifest, &["manifestKind"]).unwrap_or_else(|| "missing".into());
    let sources = manifest_value(manifest, &["sourceManifests"])
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let verified = sources
        .iter()
        .filter(|source| {
            source
                .get("verified")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .count();

    format!(
        "kind={kind}, sources={}, verified={verified}",
        sources.len()
    )
}
