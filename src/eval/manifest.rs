use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::controller::{
    ControllerAdapterMode, ControllerEvalCaseFilter, ControllerEvalCaseResult,
    ControllerEvalModelSummary, ControllerEvalReport, ControllerGrammarPayload,
    ControllerParameterClass, ControllerPromptMode, summarize_controller_eval_by_model,
};
use super::coverage::{ControllerCoverageReport, controller_eval_coverage};
use super::fingerprint::{ControllerEvalFingerprint, controller_eval_fingerprint};

pub const CONTROLLER_EVAL_MANIFEST_VERSION: &str = "0.1";

#[derive(Debug, Clone, Serialize)]
pub struct ControllerEvalRunManifest {
    #[serde(rename = "manifestVersion")]
    pub manifest_version: String,
    #[serde(rename = "manifestKind")]
    pub manifest_kind: ControllerEvalManifestKind,
    #[serde(rename = "runStatus")]
    pub run_status: ControllerEvalRunStatus,
    #[serde(rename = "startedAtUnixSeconds")]
    pub started_at_unix_seconds: u64,
    #[serde(
        rename = "completedAtUnixSeconds",
        skip_serializing_if = "Option::is_none"
    )]
    pub completed_at_unix_seconds: Option<u64>,
    #[serde(rename = "glyphVersion")]
    pub glyph_version: String,
    #[serde(rename = "gitCommit", skip_serializing_if = "Option::is_none")]
    pub git_commit: Option<String>,
    #[serde(rename = "gitTreeDirty", skip_serializing_if = "Option::is_none")]
    pub git_tree_dirty: Option<bool>,
    pub config: ControllerEvalRunConfig,
    pub fingerprint: ControllerEvalFingerprint,
    pub security: ControllerEvalRunSecurity,
    #[serde(rename = "reportSummary", skip_serializing_if = "Option::is_none")]
    pub report_summary: Option<ControllerEvalRunReportSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage: Option<ControllerCoverageReport>,
    #[serde(rename = "sourceManifests", skip_serializing_if = "Vec::is_empty")]
    pub source_manifests: Vec<ControllerEvalSourceManifest>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ControllerEvalManifestKind {
    Run,
    Merged,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ControllerEvalRunStatus {
    Planned,
    Completed,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerEvalRunConfig {
    #[serde(rename = "adapterMode")]
    pub adapter_mode: ControllerAdapterMode,
    pub endpoint: Option<String>,
    #[serde(rename = "apiKeyEnv")]
    pub api_key_env: Option<String>,
    #[serde(rename = "apiKeyProvided")]
    pub api_key_provided: bool,
    pub models: Vec<ControllerEvalRunModel>,
    #[serde(rename = "promptModes")]
    pub prompt_modes: Vec<ControllerPromptMode>,
    #[serde(rename = "grammarPayload")]
    pub grammar_payload: ControllerGrammarPayload,
    #[serde(rename = "caseFilter")]
    pub case_filter: ControllerEvalRunCaseFilter,
    #[serde(rename = "selectedCaseIds")]
    pub selected_case_ids: Vec<String>,
    #[serde(rename = "selectedCaseCount")]
    pub selected_case_count: usize,
    pub artifacts: ControllerEvalRunArtifacts,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerEvalRunModel {
    #[serde(rename = "parameterClass")]
    pub parameter_class: ControllerParameterClass,
    #[serde(rename = "modelId")]
    pub model_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerEvalRunCaseFilter {
    #[serde(rename = "caseIds")]
    pub case_ids: Vec<String>,
    pub tags: Vec<String>,
    pub families: Vec<String>,
    pub profiles: Vec<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerEvalRunArtifacts {
    #[serde(rename = "jsonlPath")]
    pub jsonl_path: Option<String>,
    #[serde(rename = "manifestPath")]
    pub manifest_path: Option<String>,
    #[serde(rename = "emitPromptsPath")]
    pub emit_prompts_path: Option<String>,
    #[serde(
        rename = "promptBundleOverallSha256",
        skip_serializing_if = "Option::is_none"
    )]
    pub prompt_bundle_overall_sha256: Option<String>,
    #[serde(
        rename = "promptBundleManifestSha256",
        skip_serializing_if = "Option::is_none"
    )]
    pub prompt_bundle_manifest_sha256: Option<String>,
    #[serde(rename = "responseBundlePath", skip_serializing_if = "Option::is_none")]
    pub response_bundle_path: Option<String>,
    #[serde(
        rename = "responseBundleFileCount",
        skip_serializing_if = "Option::is_none"
    )]
    pub response_bundle_file_count: Option<usize>,
    #[serde(
        rename = "responseBundleBytes",
        skip_serializing_if = "Option::is_none"
    )]
    pub response_bundle_bytes: Option<u64>,
    #[serde(
        rename = "responseBundleSha256",
        skip_serializing_if = "Option::is_none"
    )]
    pub response_bundle_sha256: Option<String>,
    #[serde(rename = "streamJsonl")]
    pub stream_jsonl: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerEvalRunSecurity {
    #[serde(rename = "apiKeyValueOmitted")]
    pub api_key_value_omitted: bool,
    #[serde(rename = "realShellRunEnabled")]
    pub real_shell_run_enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerEvalRunReportSummary {
    pub mode: ControllerAdapterMode,
    #[serde(rename = "caseRows")]
    pub case_rows: usize,
    #[serde(rename = "actualModelCalls")]
    pub actual_model_calls: usize,
    #[serde(rename = "byModel")]
    pub by_model: Vec<ControllerEvalModelSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerEvalSourceManifest {
    #[serde(rename = "manifestPath")]
    pub manifest_path: String,
    #[serde(rename = "jsonlPath")]
    pub jsonl_path: String,
    #[serde(rename = "fingerprintSha256")]
    pub fingerprint_sha256: String,
    #[serde(rename = "caseRows")]
    pub case_rows: usize,
    pub verified: bool,
}

#[derive(Debug, Clone)]
pub struct ControllerEvalMergedManifestInput {
    pub started_at_unix_seconds: u64,
    pub completed_at_unix_seconds: u64,
    pub glyph_version: String,
    pub git_commit: Option<String>,
    pub git_tree_dirty: Option<bool>,
    pub jsonl_path: String,
    pub manifest_path: String,
    pub source_manifests: Vec<ControllerEvalSourceManifest>,
}

pub fn build_controller_eval_run_manifest(
    started_at_unix_seconds: u64,
    completed_at_unix_seconds: Option<u64>,
    glyph_version: impl Into<String>,
    git_commit: Option<String>,
    git_tree_dirty: Option<bool>,
    config: ControllerEvalRunConfig,
    report: Option<&ControllerEvalReport>,
) -> ControllerEvalRunManifest {
    ControllerEvalRunManifest {
        manifest_version: CONTROLLER_EVAL_MANIFEST_VERSION.to_string(),
        manifest_kind: ControllerEvalManifestKind::Run,
        run_status: if completed_at_unix_seconds.is_some() {
            ControllerEvalRunStatus::Completed
        } else {
            ControllerEvalRunStatus::Planned
        },
        started_at_unix_seconds,
        completed_at_unix_seconds,
        glyph_version: glyph_version.into(),
        git_commit,
        git_tree_dirty,
        config,
        fingerprint: controller_eval_fingerprint(),
        security: ControllerEvalRunSecurity {
            api_key_value_omitted: true,
            real_shell_run_enabled: false,
        },
        report_summary: report.map(|report| ControllerEvalRunReportSummary {
            mode: report.mode.clone(),
            case_rows: report.cases.len(),
            actual_model_calls: report.actual_model_calls,
            by_model: report.by_model.clone(),
        }),
        coverage: report.map(|report| controller_eval_coverage(&report.cases)),
        source_manifests: vec![],
    }
}

pub fn build_merged_controller_eval_manifest(
    input: ControllerEvalMergedManifestInput,
    cases: &[ControllerEvalCaseResult],
) -> ControllerEvalRunManifest {
    ControllerEvalRunManifest {
        manifest_version: CONTROLLER_EVAL_MANIFEST_VERSION.to_string(),
        manifest_kind: ControllerEvalManifestKind::Merged,
        run_status: ControllerEvalRunStatus::Completed,
        started_at_unix_seconds: input.started_at_unix_seconds,
        completed_at_unix_seconds: Some(input.completed_at_unix_seconds),
        glyph_version: input.glyph_version,
        git_commit: input.git_commit,
        git_tree_dirty: input.git_tree_dirty,
        config: merged_manifest_config(input.jsonl_path, input.manifest_path, cases),
        fingerprint: controller_eval_fingerprint(),
        security: ControllerEvalRunSecurity {
            api_key_value_omitted: true,
            real_shell_run_enabled: false,
        },
        report_summary: Some(report_summary_from_cases(cases)),
        coverage: Some(controller_eval_coverage(cases)),
        source_manifests: input.source_manifests,
    }
}

impl From<&ControllerEvalCaseFilter> for ControllerEvalRunCaseFilter {
    fn from(filter: &ControllerEvalCaseFilter) -> Self {
        Self {
            case_ids: filter.case_ids.clone(),
            tags: filter.tags.clone(),
            families: filter.families.clone(),
            profiles: filter.profiles.clone(),
            limit: filter.limit,
        }
    }
}

fn merged_manifest_config(
    jsonl_path: impl Into<String>,
    manifest_path: impl Into<String>,
    cases: &[ControllerEvalCaseResult],
) -> ControllerEvalRunConfig {
    let selected_case_ids = cases
        .iter()
        .map(|case| case.case_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    ControllerEvalRunConfig {
        adapter_mode: merged_adapter_mode(cases),
        endpoint: None,
        api_key_env: None,
        api_key_provided: false,
        models: merged_models(cases),
        prompt_modes: cases
            .iter()
            .map(|case| case.prompt_mode)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        grammar_payload: merged_grammar_payload(cases),
        case_filter: ControllerEvalRunCaseFilter {
            case_ids: selected_case_ids.clone(),
            tags: vec![],
            families: vec![],
            profiles: vec![],
            limit: None,
        },
        selected_case_count: selected_case_ids.len(),
        selected_case_ids,
        artifacts: ControllerEvalRunArtifacts {
            jsonl_path: Some(jsonl_path.into()),
            manifest_path: Some(manifest_path.into()),
            emit_prompts_path: None,
            prompt_bundle_overall_sha256: None,
            prompt_bundle_manifest_sha256: None,
            response_bundle_path: None,
            response_bundle_file_count: None,
            response_bundle_bytes: None,
            response_bundle_sha256: None,
            stream_jsonl: false,
        },
    }
}

fn merged_adapter_mode(cases: &[ControllerEvalCaseResult]) -> ControllerAdapterMode {
    let Some(first) = cases.first() else {
        return ControllerAdapterMode::Mixed;
    };

    if cases
        .iter()
        .all(|case| case.adapter_mode == first.adapter_mode)
    {
        first.adapter_mode.clone()
    } else {
        ControllerAdapterMode::Mixed
    }
}

fn merged_grammar_payload(cases: &[ControllerEvalCaseResult]) -> ControllerGrammarPayload {
    let Some(first) = cases.first() else {
        return ControllerGrammarPayload::None;
    };

    if cases
        .iter()
        .all(|case| case.grammar_payload == first.grammar_payload)
    {
        first.grammar_payload
    } else {
        ControllerGrammarPayload::None
    }
}

fn merged_models(cases: &[ControllerEvalCaseResult]) -> Vec<ControllerEvalRunModel> {
    cases
        .iter()
        .map(|case| {
            (
                (case.parameter_class, case.model_id.clone()),
                ControllerEvalRunModel {
                    parameter_class: case.parameter_class,
                    model_id: case.model_id.clone(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>()
        .into_values()
        .collect()
}

fn report_summary_from_cases(cases: &[ControllerEvalCaseResult]) -> ControllerEvalRunReportSummary {
    ControllerEvalRunReportSummary {
        mode: merged_adapter_mode(cases),
        case_rows: cases.len(),
        actual_model_calls: cases
            .iter()
            .filter(|case| case.adapter_mode == ControllerAdapterMode::OpenAiCompatible)
            .count()
            * 3,
        by_model: summarize_controller_eval_by_model(cases),
    }
}
