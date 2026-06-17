use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

use crate::harness::mock_tools::create_mock_tool_registry;
use crate::ir::glyph_ir::{GlyphIr, parse_glyph_to_ir};
use crate::ir::validate_ir::validate_ir;
use crate::runtime::glyph_vm::GlyphVm;
use crate::runtime::trace::TraceEvent;

use super::controller::{
    ControllerEvalCaseFilter, ControllerGrammarPayload, ControllerPromptMode,
    build_controller_prompt_with_payload, select_controller_eval_cases,
};

pub const CONTROLLER_DATASET_VERSION: &str = "glyph-controller-dataset/0.1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ControllerDatasetSplit {
    Train,
    Validation,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerDatasetExport {
    pub version: String,
    #[serde(rename = "recordCount")]
    pub record_count: usize,
    #[serde(rename = "trainRecords")]
    pub train_records: usize,
    #[serde(rename = "validationRecords")]
    pub validation_records: usize,
    pub records: Vec<ControllerDatasetRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerDatasetRecord {
    pub version: String,
    #[serde(rename = "caseId")]
    pub case_id: String,
    pub split: ControllerDatasetSplit,
    pub request: String,
    pub tags: Vec<String>,
    #[serde(rename = "targetGlyph")]
    pub target_glyph: String,
    #[serde(rename = "targetIr")]
    pub target_ir: GlyphIr,
    #[serde(rename = "targetTrace")]
    pub target_trace: Vec<TraceEvent>,
    #[serde(rename = "finalOutputs")]
    pub final_outputs: Vec<Value>,
    pub variables: BTreeMap<String, Value>,
    #[serde(rename = "mockFs")]
    pub mock_fs: BTreeMap<String, Value>,
    #[serde(rename = "trainingExample")]
    pub training_example: ControllerTrainingExample,
    pub metadata: ControllerDatasetMetadata,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerTrainingExample {
    pub system: String,
    pub user: String,
    pub assistant: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerDatasetMetadata {
    #[serde(rename = "expectsRepairLoop")]
    pub expects_repair_loop: bool,
    #[serde(rename = "directFailureReason")]
    pub direct_failure_reason: String,
    #[serde(rename = "traceEventCount")]
    pub trace_event_count: usize,
    #[serde(rename = "finalOutputCount")]
    pub final_output_count: usize,
}

#[derive(Debug, Clone)]
pub struct ControllerDatasetOptions {
    pub case_filter: ControllerEvalCaseFilter,
    pub validation_stride: Option<usize>,
}

impl Default for ControllerDatasetOptions {
    fn default() -> Self {
        Self {
            case_filter: ControllerEvalCaseFilter::default(),
            validation_stride: Some(8),
        }
    }
}

pub fn export_controller_dataset(
    options: ControllerDatasetOptions,
) -> Result<ControllerDatasetExport, String> {
    let vm = GlyphVm::new(create_mock_tool_registry());
    let records = select_controller_eval_cases(&options.case_filter)
        .into_iter()
        .enumerate()
        .map(|(index, eval_case)| {
            let split = dataset_split(index, options.validation_stride);
            let ir = validate_ir(
                parse_glyph_to_ir(&eval_case.expected_glyph).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            let run = vm
                .execute(ir.clone(), Default::default())
                .map_err(|error| error.to_string())?;
            let target_trace = normalize_trace(run.trace);
            let prompt = build_controller_prompt_with_payload(
                &eval_case,
                ControllerPromptMode::Constrained,
                ControllerGrammarPayload::Gbnf,
            );

            Ok(ControllerDatasetRecord {
                version: CONTROLLER_DATASET_VERSION.to_string(),
                case_id: eval_case.id,
                split,
                request: eval_case.request,
                tags: eval_case.tags,
                target_glyph: eval_case.expected_glyph.clone(),
                target_ir: ir,
                metadata: ControllerDatasetMetadata {
                    expects_repair_loop: eval_case.expects_repair_loop,
                    direct_failure_reason: eval_case.direct_failure_reason,
                    trace_event_count: target_trace.len(),
                    final_output_count: run.outputs.len(),
                },
                target_trace,
                final_outputs: run.outputs,
                variables: run.variables,
                mock_fs: run.mock_fs,
                training_example: ControllerTrainingExample {
                    system: "You are a Glyph controller. Return only one executable Glyph program."
                        .to_string(),
                    user: prompt,
                    assistant: eval_case.expected_glyph,
                },
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let train_records = records
        .iter()
        .filter(|record| record.split == ControllerDatasetSplit::Train)
        .count();
    let validation_records = records.len() - train_records;

    Ok(ControllerDatasetExport {
        version: CONTROLLER_DATASET_VERSION.to_string(),
        record_count: records.len(),
        train_records,
        validation_records,
        records,
    })
}

fn dataset_split(index: usize, validation_stride: Option<usize>) -> ControllerDatasetSplit {
    match validation_stride {
        Some(stride) if stride > 0 && (index + 1) % stride == 0 => {
            ControllerDatasetSplit::Validation
        }
        _ => ControllerDatasetSplit::Train,
    }
}

fn normalize_trace(trace: Vec<TraceEvent>) -> Vec<TraceEvent> {
    trace
        .into_iter()
        .map(|mut event| {
            event.duration_ms = 0;
            event
        })
        .collect()
}
