use std::collections::BTreeMap;

use serde::Serialize;

use super::controller::{ControllerAdapterMode, ControllerEvalCaseResult};

#[derive(Debug, Clone, Serialize)]
pub struct ControllerEvalMergeReport {
    #[serde(rename = "inputRows")]
    pub input_rows: usize,
    #[serde(rename = "outputRows")]
    pub output_rows: usize,
    #[serde(rename = "replacedRows")]
    pub replaced_rows: usize,
}

#[derive(Debug, Clone)]
pub struct ControllerEvalMergeResult {
    pub cases: Vec<ControllerEvalCaseResult>,
    pub report: ControllerEvalMergeReport,
}

pub fn merge_controller_eval_cases(
    case_sets: Vec<Vec<ControllerEvalCaseResult>>,
) -> ControllerEvalMergeResult {
    let input_rows = case_sets.iter().map(Vec::len).sum::<usize>();
    let mut rows = BTreeMap::<String, ControllerEvalCaseResult>::new();
    let mut replaced_rows = 0usize;

    for case in case_sets.into_iter().flatten() {
        if rows.insert(controller_eval_case_key(&case), case).is_some() {
            replaced_rows += 1;
        }
    }

    let cases = rows.into_values().collect::<Vec<_>>();
    let output_rows = cases.len();

    ControllerEvalMergeResult {
        cases,
        report: ControllerEvalMergeReport {
            input_rows,
            output_rows,
            replaced_rows,
        },
    }
}

fn controller_eval_case_key(case: &ControllerEvalCaseResult) -> String {
    [
        adapter_mode_key(&case.adapter_mode),
        case.parameter_class.as_str(),
        &case.model_id,
        case.prompt_mode.as_str(),
        case.grammar_payload.as_str(),
        &case.case_id,
    ]
    .join("\u{1f}")
}

fn adapter_mode_key(adapter_mode: &ControllerAdapterMode) -> &'static str {
    match adapter_mode {
        ControllerAdapterMode::Fixture => "fixture",
        ControllerAdapterMode::OpenAiCompatible => "openai-compatible",
        ControllerAdapterMode::OfflineResponses => "offline-responses",
        ControllerAdapterMode::Mixed => "mixed",
    }
}
