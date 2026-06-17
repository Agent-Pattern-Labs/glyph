use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::controller::{
    ControllerAdapterMode, ControllerEvalCaseFilter, ControllerGrammarPayload,
    ControllerParameterClass, ControllerPromptMode, select_controller_eval_cases,
};

#[derive(Debug, Clone, Serialize)]
pub struct ControllerPreflightReport {
    pub passed: bool,
    #[serde(rename = "adapterMode")]
    pub adapter_mode: ControllerAdapterMode,
    #[serde(rename = "promptModes")]
    pub prompt_modes: Vec<ControllerPromptMode>,
    #[serde(rename = "grammarPayload")]
    pub grammar_payload: ControllerGrammarPayload,
    pub models: Vec<ControllerPreflightModel>,
    #[serde(rename = "selectedCaseCount")]
    pub selected_case_count: usize,
    #[serde(rename = "selectedCaseIds")]
    pub selected_case_ids: Vec<String>,
    #[serde(rename = "expectedRows")]
    pub expected_rows: usize,
    #[serde(rename = "expectedModelCalls")]
    pub expected_model_calls: usize,
    pub artifacts: ControllerPreflightArtifacts,
    pub checks: Vec<ControllerPreflightCheck>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerPreflightModel {
    #[serde(rename = "parameterClass")]
    pub parameter_class: ControllerParameterClass,
    #[serde(rename = "modelId")]
    pub model_id: Option<String>,
    #[serde(rename = "bucketEvidence", skip_serializing_if = "Option::is_none")]
    pub bucket_evidence: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerPreflightArtifacts {
    #[serde(rename = "jsonlPath")]
    pub jsonl_path: Option<String>,
    #[serde(rename = "manifestPath")]
    pub manifest_path: Option<String>,
    #[serde(rename = "streamJsonl")]
    pub stream_jsonl: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerPreflightCheck {
    pub id: String,
    pub status: ControllerPreflightCheckStatus,
    pub observed: String,
    pub required: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ControllerPreflightCheckStatus {
    Pass,
    Fail,
}

#[derive(Debug, Clone)]
pub struct ControllerPreflightOptions {
    pub adapter_mode: ControllerAdapterMode,
    pub prompt_modes: Vec<ControllerPromptMode>,
    pub grammar_payload: ControllerGrammarPayload,
    pub case_filter: ControllerEvalCaseFilter,
    pub models: Vec<ControllerPreflightModel>,
    pub jsonl_path: Option<String>,
    pub manifest_path: Option<String>,
    pub stream_jsonl: bool,
}

pub fn preflight_controller_eval(options: ControllerPreflightOptions) -> ControllerPreflightReport {
    let prompt_modes = if options.prompt_modes.is_empty() {
        vec![ControllerPromptMode::Constrained]
    } else {
        options.prompt_modes
    };
    let selected_case_ids = select_controller_eval_cases(&options.case_filter)
        .into_iter()
        .map(|case| case.id)
        .collect::<Vec<_>>();
    let selected_case_count = selected_case_ids.len();
    let model_count = options.models.len();
    let expected_rows = selected_case_count * prompt_modes.len() * model_count;
    let expected_model_calls = if options.adapter_mode == ControllerAdapterMode::OpenAiCompatible {
        expected_rows * 3
    } else {
        0
    };
    let artifacts = ControllerPreflightArtifacts {
        jsonl_path: options.jsonl_path,
        manifest_path: options.manifest_path,
        stream_jsonl: options.stream_jsonl,
    };
    let checks = vec![
        check(
            "selected_cases",
            selected_case_count > 0,
            selected_case_count.to_string(),
            "at least one selected eval case".to_string(),
        ),
        check(
            "required_model_buckets",
            has_required_buckets(&options.models),
            observed_buckets(&options.models).join(","),
            "1b,3b,7b,frontier model buckets".to_string(),
        ),
        check(
            "model_ids_present",
            options.adapter_mode != ControllerAdapterMode::OpenAiCompatible
                || options.models.iter().all(|model| model.model_id.is_some()),
            missing_model_buckets(&options.models).join(","),
            "all OpenAI-compatible model buckets have model ids".to_string(),
        ),
        check(
            "model_ids_unique",
            options.adapter_mode != ControllerAdapterMode::OpenAiCompatible
                || duplicate_model_ids(&options.models).is_empty(),
            observed_duplicate_model_ids(&options.models),
            "each OpenAI-compatible model bucket uses a distinct model id".to_string(),
        ),
        check(
            "model_bucket_evidence",
            options.adapter_mode != ControllerAdapterMode::OpenAiCompatible
                || options.models.iter().all(|model| {
                    model
                        .bucket_evidence
                        .as_deref()
                        .is_some_and(|evidence| !evidence.trim().is_empty())
                }),
            missing_bucket_evidence(&options.models).join(","),
            "all OpenAI-compatible model buckets include reproducible bucket evidence".to_string(),
        ),
        check(
            "constrained_uses_gbnf",
            options.adapter_mode != ControllerAdapterMode::OpenAiCompatible
                || !prompt_modes.contains(&ControllerPromptMode::Constrained)
                || options.grammar_payload == ControllerGrammarPayload::Gbnf,
            options.grammar_payload.as_str().to_string(),
            "live constrained runs use grammarPayload=gbnf".to_string(),
        ),
        check(
            "live_jsonl_artifact",
            options.adapter_mode != ControllerAdapterMode::OpenAiCompatible
                || artifacts.jsonl_path.is_some(),
            artifacts
                .jsonl_path
                .clone()
                .unwrap_or_else(|| "missing".to_string()),
            "live runs write --jsonl".to_string(),
        ),
        check(
            "live_stream_jsonl",
            options.adapter_mode != ControllerAdapterMode::OpenAiCompatible
                || artifacts.stream_jsonl,
            artifacts.stream_jsonl.to_string(),
            "live runs use --stream-jsonl".to_string(),
        ),
        check(
            "live_manifest_artifact",
            options.adapter_mode != ControllerAdapterMode::OpenAiCompatible
                || artifacts.manifest_path.is_some(),
            artifacts
                .manifest_path
                .clone()
                .unwrap_or_else(|| "missing".to_string()),
            "live runs write --manifest".to_string(),
        ),
    ];
    let passed = checks
        .iter()
        .all(|check| check.status == ControllerPreflightCheckStatus::Pass);

    ControllerPreflightReport {
        passed,
        adapter_mode: options.adapter_mode,
        prompt_modes,
        grammar_payload: options.grammar_payload,
        models: options.models,
        selected_case_count,
        selected_case_ids,
        expected_rows,
        expected_model_calls,
        artifacts,
        checks,
    }
}

fn check(id: &str, passed: bool, observed: String, required: String) -> ControllerPreflightCheck {
    ControllerPreflightCheck {
        id: id.to_string(),
        status: if passed {
            ControllerPreflightCheckStatus::Pass
        } else {
            ControllerPreflightCheckStatus::Fail
        },
        observed,
        required,
    }
}

fn has_required_buckets(models: &[ControllerPreflightModel]) -> bool {
    let observed = observed_buckets(models);
    ["1b", "3b", "7b", "frontier"]
        .iter()
        .all(|bucket| observed.contains(&bucket.to_string()))
}

fn observed_buckets(models: &[ControllerPreflightModel]) -> Vec<String> {
    models
        .iter()
        .map(|model| model.parameter_class.as_str().to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn missing_model_buckets(models: &[ControllerPreflightModel]) -> Vec<String> {
    models
        .iter()
        .filter(|model| model.model_id.is_none())
        .map(|model| model.parameter_class.as_str().to_string())
        .collect()
}

fn missing_bucket_evidence(models: &[ControllerPreflightModel]) -> Vec<String> {
    models
        .iter()
        .filter(|model| {
            model
                .bucket_evidence
                .as_deref()
                .is_none_or(|evidence| evidence.trim().is_empty())
        })
        .map(|model| model.parameter_class.as_str().to_string())
        .collect()
}

fn observed_duplicate_model_ids(models: &[ControllerPreflightModel]) -> String {
    let duplicates = duplicate_model_ids(models);
    if duplicates.is_empty() {
        "none".to_string()
    } else {
        duplicates.join(",")
    }
}

fn duplicate_model_ids(models: &[ControllerPreflightModel]) -> Vec<String> {
    let mut assignments = BTreeMap::<String, Vec<String>>::new();
    for model in models {
        if let Some(model_id) = &model.model_id {
            assignments
                .entry(model_id.clone())
                .or_default()
                .push(model.parameter_class.as_str().to_string());
        }
    }

    assignments
        .into_iter()
        .filter(|(_, buckets)| buckets.len() > 1)
        .map(|(model_id, buckets)| format!("{model_id}=>{}", buckets.join("|")))
        .collect()
}
