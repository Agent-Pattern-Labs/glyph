use serde::Serialize;

use super::controller::{
    ControllerAdapterMode, ControllerEvalCaseFilter, ControllerEvalModelSummary,
    ControllerEvalReport, ControllerGrammarPayload, ControllerParameterClass, ControllerPromptMode,
};
use super::coverage::{ControllerCoverageReport, controller_eval_coverage};

pub const CONTROLLER_EVAL_MANIFEST_VERSION: &str = "0.1";

#[derive(Debug, Clone, Serialize)]
pub struct ControllerEvalRunManifest {
    #[serde(rename = "manifestVersion")]
    pub manifest_version: String,
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
    pub security: ControllerEvalRunSecurity,
    #[serde(rename = "reportSummary", skip_serializing_if = "Option::is_none")]
    pub report_summary: Option<ControllerEvalRunReportSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage: Option<ControllerCoverageReport>,
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
