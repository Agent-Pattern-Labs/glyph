use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::ir::glyph_ir::parse_glyph_to_ir;
use crate::ir::validate_ir::validate_ir;

use super::dataset::{
    CONTROLLER_DATASET_VERSION, ControllerDatasetOptions, ControllerDatasetRecord,
    ControllerDatasetSplit, ControllerTrainingExample, export_controller_dataset,
};

pub const CONTROLLER_CURRICULUM_VERSION: &str = "glyph-controller-curriculum/0.1";
const NEGATIVE_CANDIDATES_PER_CASE: usize = 3;
const MIN_CASES: usize = 72;

#[derive(Debug, Clone, Default)]
pub struct ControllerCurriculumOptions {
    pub dataset_options: ControllerDatasetOptions,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerCurriculumExport {
    pub version: String,
    #[serde(rename = "sourceDatasetVersion")]
    pub source_dataset_version: String,
    #[serde(rename = "recordCount")]
    pub record_count: usize,
    #[serde(rename = "caseCount")]
    pub case_count: usize,
    #[serde(rename = "positiveRecords")]
    pub positive_records: usize,
    #[serde(rename = "repairRecords")]
    pub repair_records: usize,
    #[serde(rename = "rejectedNegativeRecords")]
    pub rejected_negative_records: usize,
    #[serde(rename = "trainRecords")]
    pub train_records: usize,
    #[serde(rename = "validationRecords")]
    pub validation_records: usize,
    pub records: Vec<ControllerCurriculumRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerCurriculumRecord {
    pub version: String,
    pub id: String,
    #[serde(rename = "caseId")]
    pub case_id: String,
    pub split: ControllerDatasetSplit,
    pub kind: ControllerCurriculumRecordKind,
    pub request: String,
    pub tags: Vec<String>,
    #[serde(rename = "targetGlyph")]
    pub target_glyph: String,
    #[serde(rename = "rejectedOutput", skip_serializing_if = "Option::is_none")]
    pub rejected_output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejection: Option<ControllerCurriculumRejection>,
    #[serde(rename = "trainingExample")]
    pub training_example: ControllerTrainingExample,
    pub metadata: ControllerCurriculumMetadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ControllerCurriculumRecordKind {
    Positive,
    Repair,
    RejectedNegative,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerCurriculumRejection {
    pub stage: ControllerCurriculumRejectionStage,
    pub message: String,
    pub feedback: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ControllerCurriculumRejectionStage {
    Parse,
    SemanticValidation,
    UnexpectedPass,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerCurriculumMetadata {
    #[serde(rename = "sourceDatasetVersion")]
    pub source_dataset_version: String,
    #[serde(rename = "candidateId", skip_serializing_if = "Option::is_none")]
    pub candidate_id: Option<String>,
    #[serde(rename = "expectsRepairLoop")]
    pub expects_repair_loop: bool,
    #[serde(rename = "useCase")]
    pub use_case: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ControllerCurriculumQualityDecision {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ControllerCurriculumQualityStatus {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerCurriculumQualityReport {
    pub decision: ControllerCurriculumQualityDecision,
    pub passed: bool,
    pub metrics: ControllerCurriculumQualityMetrics,
    pub checks: Vec<ControllerCurriculumQualityCheck>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerCurriculumQualityMetrics {
    #[serde(rename = "recordCount")]
    pub record_count: usize,
    #[serde(rename = "caseCount")]
    pub case_count: usize,
    #[serde(rename = "positiveRecords")]
    pub positive_records: usize,
    #[serde(rename = "repairRecords")]
    pub repair_records: usize,
    #[serde(rename = "rejectedNegativeRecords")]
    pub rejected_negative_records: usize,
    #[serde(rename = "trainRecords")]
    pub train_records: usize,
    #[serde(rename = "validationRecords")]
    pub validation_records: usize,
    #[serde(rename = "parseRejectionRecords")]
    pub parse_rejection_records: usize,
    #[serde(rename = "semanticRejectionRecords")]
    pub semantic_rejection_records: usize,
    #[serde(rename = "casesWithFullCurriculum")]
    pub cases_with_full_curriculum: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerCurriculumQualityCheck {
    pub id: String,
    pub status: ControllerCurriculumQualityStatus,
    pub observed: String,
    pub required: String,
}

struct NegativeCandidate {
    id: &'static str,
    output: String,
}

pub fn export_controller_curriculum(
    options: ControllerCurriculumOptions,
) -> Result<ControllerCurriculumExport, String> {
    let dataset = export_controller_dataset(options.dataset_options)?;
    let mut records = Vec::new();

    for source in &dataset.records {
        records.push(positive_record(source));

        for candidate in negative_candidates(source) {
            let rejection = reject_candidate(&candidate.output);
            records.push(rejected_negative_record(source, &candidate, &rejection));
            records.push(repair_record(source, &candidate, &rejection));
        }
    }

    let positive_records = records
        .iter()
        .filter(|record| record.kind == ControllerCurriculumRecordKind::Positive)
        .count();
    let repair_records = records
        .iter()
        .filter(|record| record.kind == ControllerCurriculumRecordKind::Repair)
        .count();
    let rejected_negative_records = records
        .iter()
        .filter(|record| record.kind == ControllerCurriculumRecordKind::RejectedNegative)
        .count();
    let train_records = records
        .iter()
        .filter(|record| record.split == ControllerDatasetSplit::Train)
        .count();
    let validation_records = records.len() - train_records;

    Ok(ControllerCurriculumExport {
        version: CONTROLLER_CURRICULUM_VERSION.to_string(),
        source_dataset_version: CONTROLLER_DATASET_VERSION.to_string(),
        record_count: records.len(),
        case_count: dataset.record_count,
        positive_records,
        repair_records,
        rejected_negative_records,
        train_records,
        validation_records,
        records,
    })
}

pub fn assess_controller_curriculum_quality(
    export: &ControllerCurriculumExport,
) -> ControllerCurriculumQualityReport {
    let cases = export
        .records
        .iter()
        .map(|record| record.case_id.clone())
        .collect::<BTreeSet<_>>();
    let positive_records = count_kind(export, ControllerCurriculumRecordKind::Positive);
    let repair_records = count_kind(export, ControllerCurriculumRecordKind::Repair);
    let rejected_negative_records =
        count_kind(export, ControllerCurriculumRecordKind::RejectedNegative);
    let parse_rejection_records =
        count_rejection_stage(export, ControllerCurriculumRejectionStage::Parse);
    let semantic_rejection_records = count_rejection_stage(
        export,
        ControllerCurriculumRejectionStage::SemanticValidation,
    );
    let cases_with_full_curriculum = cases_with_full_curriculum(export);

    let metrics = ControllerCurriculumQualityMetrics {
        record_count: export.record_count,
        case_count: cases.len(),
        positive_records,
        repair_records,
        rejected_negative_records,
        train_records: export.train_records,
        validation_records: export.validation_records,
        parse_rejection_records,
        semantic_rejection_records,
        cases_with_full_curriculum,
    };

    let checks = vec![
        curriculum_check(
            "record_count",
            metrics.record_count == export.records.len()
                && metrics.case_count >= MIN_CASES
                && metrics.record_count >= MIN_CASES * 7,
            format!(
                "records={}, cases={}",
                metrics.record_count, metrics.case_count
            ),
            format!(">= {MIN_CASES} cases and >= {} records", MIN_CASES * 7),
        ),
        curriculum_check(
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
        curriculum_check(
            "positive_examples",
            metrics.positive_records == metrics.case_count && metrics.positive_records >= MIN_CASES,
            format!("{}/{}", metrics.positive_records, metrics.case_count),
            "one positive emit record per case".to_string(),
        ),
        curriculum_check(
            "repair_examples",
            metrics.repair_records == metrics.case_count * NEGATIVE_CANDIDATES_PER_CASE
                && repair_targets_are_valid(export),
            format!(
                "repair={}, expected={}",
                metrics.repair_records,
                metrics.case_count * NEGATIVE_CANDIDATES_PER_CASE
            ),
            "one correction record per rejected candidate with assistant equal to target Glyph"
                .to_string(),
        ),
        curriculum_check(
            "rejected_negative_examples",
            metrics.rejected_negative_records == metrics.case_count * NEGATIVE_CANDIDATES_PER_CASE
                && rejected_negative_labels_are_valid(export),
            format!(
                "rejected={}, expected={}",
                metrics.rejected_negative_records,
                metrics.case_count * NEGATIVE_CANDIDATES_PER_CASE
            ),
            "one rejection record per invalid candidate with an explicit REJECT label".to_string(),
        ),
        curriculum_check(
            "rejection_stage_coverage",
            metrics.parse_rejection_records >= metrics.case_count
                && metrics.semantic_rejection_records >= metrics.case_count,
            format!(
                "parse={}, semantic={}",
                metrics.parse_rejection_records, metrics.semantic_rejection_records
            ),
            "parse and semantic validation failures are both represented".to_string(),
        ),
        curriculum_check(
            "full_case_curriculum",
            metrics.cases_with_full_curriculum == metrics.case_count,
            format!(
                "{}/{}",
                metrics.cases_with_full_curriculum, metrics.case_count
            ),
            "every case has positive, repair, and rejected-negative records".to_string(),
        ),
    ];
    let passed = checks
        .iter()
        .all(|check| check.status == ControllerCurriculumQualityStatus::Pass);

    ControllerCurriculumQualityReport {
        decision: if passed {
            ControllerCurriculumQualityDecision::Pass
        } else {
            ControllerCurriculumQualityDecision::Fail
        },
        passed,
        metrics,
        checks,
    }
}

fn positive_record(source: &ControllerDatasetRecord) -> ControllerCurriculumRecord {
    ControllerCurriculumRecord {
        version: CONTROLLER_CURRICULUM_VERSION.to_string(),
        id: format!("{}::positive", source.case_id),
        case_id: source.case_id.clone(),
        split: source.split,
        kind: ControllerCurriculumRecordKind::Positive,
        request: source.request.clone(),
        tags: source.tags.clone(),
        target_glyph: source.target_glyph.clone(),
        rejected_output: None,
        rejection: None,
        training_example: source.training_example.clone(),
        metadata: ControllerCurriculumMetadata {
            source_dataset_version: CONTROLLER_DATASET_VERSION.to_string(),
            candidate_id: None,
            expects_repair_loop: source.metadata.expects_repair_loop,
            use_case: "sft-positive".to_string(),
        },
    }
}

fn rejected_negative_record(
    source: &ControllerDatasetRecord,
    candidate: &NegativeCandidate,
    rejection: &ControllerCurriculumRejection,
) -> ControllerCurriculumRecord {
    ControllerCurriculumRecord {
        version: CONTROLLER_CURRICULUM_VERSION.to_string(),
        id: format!("{}::rejected-negative::{}", source.case_id, candidate.id),
        case_id: source.case_id.clone(),
        split: source.split,
        kind: ControllerCurriculumRecordKind::RejectedNegative,
        request: source.request.clone(),
        tags: source.tags.clone(),
        target_glyph: source.target_glyph.clone(),
        rejected_output: Some(candidate.output.clone()),
        rejection: Some(rejection.clone()),
        training_example: ControllerTrainingExample {
            system: "You are a Glyph output judge. Return only REJECT with a concise reason."
                .to_string(),
            user: format!(
                "Request:\n{}\n\nCandidate output:\n{}\n\nGlyphVM rejection stage: {:?}\nGlyphVM error:\n{}\n\nShould this candidate be accepted?",
                source.request, candidate.output, rejection.stage, rejection.message
            ),
            assistant: format!("REJECT: {}", rejection.feedback),
        },
        metadata: ControllerCurriculumMetadata {
            source_dataset_version: CONTROLLER_DATASET_VERSION.to_string(),
            candidate_id: Some(candidate.id.to_string()),
            expects_repair_loop: source.metadata.expects_repair_loop,
            use_case: "preference-negative".to_string(),
        },
    }
}

fn repair_record(
    source: &ControllerDatasetRecord,
    candidate: &NegativeCandidate,
    rejection: &ControllerCurriculumRejection,
) -> ControllerCurriculumRecord {
    ControllerCurriculumRecord {
        version: CONTROLLER_CURRICULUM_VERSION.to_string(),
        id: format!("{}::repair::{}", source.case_id, candidate.id),
        case_id: source.case_id.clone(),
        split: source.split,
        kind: ControllerCurriculumRecordKind::Repair,
        request: source.request.clone(),
        tags: source.tags.clone(),
        target_glyph: source.target_glyph.clone(),
        rejected_output: Some(candidate.output.clone()),
        rejection: Some(rejection.clone()),
        training_example: ControllerTrainingExample {
            system: "You are a Glyph repair controller. Return only the corrected executable Glyph program."
                .to_string(),
            user: format!(
                "{}\n\nA previous controller output was rejected.\nRejected candidate:\n{}\n\nGlyphVM rejection stage: {:?}\nGlyphVM error:\n{}\n\nReturn the corrected Glyph program.",
                source.training_example.user, candidate.output, rejection.stage, rejection.message
            ),
            assistant: source.target_glyph.clone(),
        },
        metadata: ControllerCurriculumMetadata {
            source_dataset_version: CONTROLLER_DATASET_VERSION.to_string(),
            candidate_id: Some(candidate.id.to_string()),
            expects_repair_loop: source.metadata.expects_repair_loop,
            use_case: "sft-repair".to_string(),
        },
    }
}

fn negative_candidates(source: &ControllerDatasetRecord) -> Vec<NegativeCandidate> {
    let request_literal = glyph_string(&source.request);
    vec![
        NegativeCandidate {
            id: "natural-language-plan",
            output: format!(
                "I will satisfy this request by making a spec, planning, generating artifacts, checking the result, repairing issues, and exporting the final output: {}",
                source.request
            ),
        },
        NegativeCandidate {
            id: "unknown-tool",
            output: format!(
                "goal {request_literal}\n\nflow main {{\n  DO(request={request_literal}) -> result\n  EXPORT(result)\n}}"
            ),
        },
        NegativeCandidate {
            id: "unknown-variable",
            output: format!(
                "goal {request_literal}\n\nflow main {{\n  PLAN(missing) -> plan\n  EXPORT(plan)\n}}"
            ),
        },
    ]
}

fn reject_candidate(candidate: &str) -> ControllerCurriculumRejection {
    match parse_glyph_to_ir(candidate) {
        Ok(ir) => match validate_ir(ir) {
            Ok(_) => ControllerCurriculumRejection {
                stage: ControllerCurriculumRejectionStage::UnexpectedPass,
                message: "candidate unexpectedly passed GlyphIR validation".to_string(),
                feedback: "candidate should not be used as a negative example".to_string(),
            },
            Err(error) => ControllerCurriculumRejection {
                stage: ControllerCurriculumRejectionStage::SemanticValidation,
                message: error.to_string(),
                feedback:
                    "emit only registered primitives, defined variables, valid context references, and bounded repair loops"
                        .to_string(),
            },
        },
        Err(error) => ControllerCurriculumRejection {
            stage: ControllerCurriculumRejectionStage::Parse,
            message: error.to_string(),
            feedback: "emit a complete Glyph program, not prose, markdown, or partial syntax"
                .to_string(),
        },
    }
}

fn glyph_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn count_kind(export: &ControllerCurriculumExport, kind: ControllerCurriculumRecordKind) -> usize {
    export
        .records
        .iter()
        .filter(|record| record.kind == kind)
        .count()
}

fn count_rejection_stage(
    export: &ControllerCurriculumExport,
    stage: ControllerCurriculumRejectionStage,
) -> usize {
    export
        .records
        .iter()
        .filter(|record| {
            record
                .rejection
                .as_ref()
                .is_some_and(|rejection| rejection.stage == stage)
        })
        .count()
}

fn repair_targets_are_valid(export: &ControllerCurriculumExport) -> bool {
    export
        .records
        .iter()
        .filter(|record| record.kind == ControllerCurriculumRecordKind::Repair)
        .all(|record| {
            record.training_example.assistant == record.target_glyph
                && !record.training_example.assistant.contains("```")
                && record.rejected_output.is_some()
                && record.rejection.is_some()
        })
}

fn rejected_negative_labels_are_valid(export: &ControllerCurriculumExport) -> bool {
    export
        .records
        .iter()
        .filter(|record| record.kind == ControllerCurriculumRecordKind::RejectedNegative)
        .all(|record| {
            record.training_example.assistant.starts_with("REJECT:")
                && record.rejected_output.is_some()
                && record.rejection.is_some()
        })
}

fn cases_with_full_curriculum(export: &ControllerCurriculumExport) -> usize {
    let mut cases: BTreeMap<String, BTreeSet<ControllerCurriculumRecordKind>> = BTreeMap::new();
    for record in &export.records {
        cases
            .entry(record.case_id.clone())
            .or_default()
            .insert(record.kind);
    }

    cases
        .into_values()
        .filter(|kinds| {
            kinds.contains(&ControllerCurriculumRecordKind::Positive)
                && kinds.contains(&ControllerCurriculumRecordKind::Repair)
                && kinds.contains(&ControllerCurriculumRecordKind::RejectedNegative)
        })
        .count()
}

fn curriculum_check(
    id: &str,
    passed: bool,
    observed: String,
    required: String,
) -> ControllerCurriculumQualityCheck {
    ControllerCurriculumQualityCheck {
        id: id.to_string(),
        status: if passed {
            ControllerCurriculumQualityStatus::Pass
        } else {
            ControllerCurriculumQualityStatus::Fail
        },
        observed,
        required,
    }
}
