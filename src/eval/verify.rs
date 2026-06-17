use std::collections::{BTreeMap, BTreeSet};

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
            "model_ids_unique",
            model_ids_unique(cases, manifest),
            observed_model_identity_conflicts(cases, manifest),
            "each model id maps to exactly one parameter bucket in the manifest and result rows"
                .to_string(),
        ),
        check(
            "model_bucket_evidence",
            model_bucket_evidence_recorded(manifest),
            observed_model_bucket_evidence(manifest),
            "fixture runs are exempt; non-fixture run manifests record bucketEvidence for each model; merged manifests rely on verified source manifests".to_string(),
        ),
        check(
            "adapter_modes_match_manifest",
            adapter_modes_match_manifest(cases, manifest),
            observed_adapter_modes(cases)
                .into_iter()
                .collect::<Vec<_>>()
                .join(","),
            configured_adapter_mode_requirement(manifest),
        ),
        check(
            "grammar_payload_matches_manifest",
            observed_grammar_payloads(cases) == configured_grammar_payloads(manifest),
            observed_grammar_payloads(cases)
                .into_iter()
                .collect::<Vec<_>>()
                .join(","),
            configured_grammar_payloads(manifest)
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
    configured_model_entries(manifest)
        .into_iter()
        .map(|(parameter_class, model_id)| format!("{parameter_class}:{model_id}"))
        .collect()
}

fn configured_model_entries(manifest: &Value) -> Vec<(String, String)> {
    manifest_value(manifest, &["config", "models"])
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|model| {
            let parameter_class = model.get("parameterClass")?.as_str()?;
            let model_id = model.get("modelId")?.as_str()?;
            Some((parameter_class.to_string(), model_id.to_string()))
        })
        .collect()
}

fn model_ids_unique(cases: &[ControllerEvalCaseResult], manifest: &Value) -> bool {
    duplicate_configured_model_entries(manifest).is_empty()
        && duplicate_model_id_buckets(configured_model_entries(manifest)).is_empty()
        && duplicate_model_id_buckets(observed_model_entries(cases)).is_empty()
}

fn observed_model_identity_conflicts(
    cases: &[ControllerEvalCaseResult],
    manifest: &Value,
) -> String {
    format!(
        "manifestDuplicateEntries={}, manifestSharedIds={}, rowSharedIds={}",
        format_conflicts(duplicate_configured_model_entries(manifest)),
        format_conflicts(duplicate_model_id_buckets(configured_model_entries(
            manifest
        ))),
        format_conflicts(duplicate_model_id_buckets(observed_model_entries(cases)))
    )
}

fn format_conflicts(conflicts: Vec<String>) -> String {
    if conflicts.is_empty() {
        "none".to_string()
    } else {
        conflicts.join(",")
    }
}

fn duplicate_configured_model_entries(manifest: &Value) -> Vec<String> {
    let mut entries = BTreeMap::<(String, String), usize>::new();
    for entry in configured_model_entries(manifest) {
        *entries.entry(entry).or_default() += 1;
    }

    entries
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|((parameter_class, model_id), count)| {
            format!("{parameter_class}:{model_id} x{count}")
        })
        .collect()
}

fn observed_model_entries(cases: &[ControllerEvalCaseResult]) -> Vec<(String, String)> {
    cases
        .iter()
        .map(|case| {
            (
                case.parameter_class.as_str().to_string(),
                case.model_id.clone(),
            )
        })
        .collect()
}

fn duplicate_model_id_buckets(entries: Vec<(String, String)>) -> Vec<String> {
    let mut assignments = BTreeMap::<String, BTreeSet<String>>::new();
    for (parameter_class, model_id) in entries {
        assignments
            .entry(model_id)
            .or_default()
            .insert(parameter_class);
    }

    assignments
        .into_iter()
        .filter(|(_, buckets)| buckets.len() > 1)
        .map(|(model_id, buckets)| {
            format!(
                "{model_id}=>{}",
                buckets.into_iter().collect::<Vec<_>>().join("|")
            )
        })
        .collect()
}

fn model_bucket_evidence_recorded(manifest: &Value) -> bool {
    match manifest_string(manifest, &["manifestKind"]).as_deref() {
        Some("merged") => return source_manifests_verified(manifest),
        Some("run") => {}
        _ => return false,
    }

    match manifest_string(manifest, &["config", "adapterMode"]).as_deref() {
        Some("fixture") => true,
        Some("openai-compatible") | Some("offline-responses") | Some("mixed") => {
            let models = manifest_models(manifest);
            !models.is_empty()
                && models.iter().all(|model| {
                    model
                        .get("bucketEvidence")
                        .and_then(Value::as_str)
                        .is_some_and(|evidence| !evidence.trim().is_empty())
                })
        }
        _ => false,
    }
}

fn observed_model_bucket_evidence(manifest: &Value) -> String {
    let kind = manifest_string(manifest, &["manifestKind"]).unwrap_or_else(|| "missing".into());
    let adapter =
        manifest_string(manifest, &["config", "adapterMode"]).unwrap_or_else(|| "missing".into());
    if kind == "merged" {
        return format!(
            "kind=merged, adapter={adapter}, sourceManifests={}",
            source_manifest_observed(manifest)
        );
    }

    let models = manifest_models(manifest);
    let missing = models
        .iter()
        .filter_map(|model| {
            let evidence_present = model
                .get("bucketEvidence")
                .and_then(Value::as_str)
                .is_some_and(|evidence| !evidence.trim().is_empty());
            (!evidence_present).then(|| {
                let parameter_class = model
                    .get("parameterClass")
                    .and_then(Value::as_str)
                    .unwrap_or("missing");
                let model_id = model
                    .get("modelId")
                    .and_then(Value::as_str)
                    .unwrap_or("missing");
                format!("{parameter_class}:{model_id}")
            })
        })
        .collect::<Vec<_>>();
    let present = models.len().saturating_sub(missing.len());
    format!(
        "kind={kind}, adapter={adapter}, present={present}/{}, missing={}",
        models.len(),
        if missing.is_empty() {
            "none".to_string()
        } else {
            missing.join(",")
        }
    )
}

fn manifest_models(manifest: &Value) -> Vec<Value> {
    manifest_value(manifest, &["config", "models"])
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn observed_adapter_modes(cases: &[ControllerEvalCaseResult]) -> BTreeSet<String> {
    cases
        .iter()
        .map(|case| adapter_mode_as_str(&case.adapter_mode).to_string())
        .collect()
}

fn adapter_modes_match_manifest(cases: &[ControllerEvalCaseResult], manifest: &Value) -> bool {
    let observed = observed_adapter_modes(cases);
    match manifest_string(manifest, &["config", "adapterMode"]).as_deref() {
        Some("mixed") => observed.len() > 1,
        Some(adapter) => observed == BTreeSet::from([adapter.to_string()]),
        None => false,
    }
}

fn configured_adapter_mode_requirement(manifest: &Value) -> String {
    match manifest_string(manifest, &["config", "adapterMode"]) {
        Some(adapter) if adapter == "mixed" => "more than one adapter mode".to_string(),
        Some(adapter) => adapter,
        None => "config.adapterMode present".to_string(),
    }
}

fn adapter_mode_as_str(adapter_mode: &ControllerAdapterMode) -> &'static str {
    match adapter_mode {
        ControllerAdapterMode::Fixture => "fixture",
        ControllerAdapterMode::OpenAiCompatible => "openai-compatible",
        ControllerAdapterMode::OfflineResponses => "offline-responses",
        ControllerAdapterMode::Mixed => "mixed",
    }
}

fn observed_grammar_payloads(cases: &[ControllerEvalCaseResult]) -> BTreeSet<String> {
    cases
        .iter()
        .map(|case| case.grammar_payload.as_str().to_string())
        .collect()
}

fn configured_grammar_payloads(manifest: &Value) -> BTreeSet<String> {
    manifest_string(manifest, &["config", "grammarPayload"])
        .into_iter()
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
