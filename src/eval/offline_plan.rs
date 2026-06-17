use serde::{Deserialize, Serialize};

use super::controller::{ControllerParameterClass, ControllerPromptMode};
use super::controller_examples::controller_eval_cases;

pub const CONTROLLER_OFFLINE_PLAN_VERSION: &str = "glyph-controller-offline-plan/0.1";
const RESPONSE_FILES_PER_ROW: usize = 3;

#[derive(Debug, Clone)]
pub struct ControllerOfflinePlanOptions {
    pub artifact_dir: String,
}

impl Default for ControllerOfflinePlanOptions {
    fn default() -> Self {
        Self {
            artifact_dir: "out/offline-shards".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControllerOfflinePlanReport {
    pub version: String,
    #[serde(rename = "caseCount")]
    pub case_count: usize,
    #[serde(rename = "modelBuckets")]
    pub model_buckets: Vec<String>,
    #[serde(rename = "promptModes")]
    pub prompt_modes: Vec<String>,
    #[serde(rename = "grammarPayload")]
    pub grammar_payload: String,
    #[serde(rename = "promptBundleDir")]
    pub prompt_bundle_dir: String,
    #[serde(rename = "responseFilePattern")]
    pub response_file_pattern: String,
    #[serde(rename = "totalExpectedRows")]
    pub total_expected_rows: usize,
    #[serde(rename = "totalExpectedResponseFiles")]
    pub total_expected_response_files: usize,
    #[serde(rename = "mergedJsonlPath")]
    pub merged_jsonl_path: String,
    #[serde(rename = "mergedManifestPath")]
    pub merged_manifest_path: String,
    #[serde(rename = "verificationReportPath")]
    pub verification_report_path: String,
    #[serde(rename = "coverageReportPath")]
    pub coverage_report_path: String,
    #[serde(rename = "gateReportPath")]
    pub gate_report_path: String,
    #[serde(rename = "benchmarkReportPath")]
    pub benchmark_report_path: String,
    #[serde(rename = "statusReportPath")]
    pub status_report_path: String,
    #[serde(rename = "finalizeReportPath")]
    pub finalize_report_path: String,
    pub shards: Vec<ControllerOfflinePlanShard>,
    #[serde(rename = "promptBundleCommand")]
    pub prompt_bundle_command: String,
    #[serde(rename = "verifyPromptBundleCommand")]
    pub verify_prompt_bundle_command: String,
    #[serde(rename = "finalizeCommand")]
    pub finalize_command: String,
    #[serde(rename = "verifyShardsCommand")]
    pub verify_shards_command: String,
    #[serde(rename = "mergeCommand")]
    pub merge_command: String,
    #[serde(rename = "coverageCommand")]
    pub coverage_command: String,
    #[serde(rename = "verifyCommand")]
    pub verify_command: String,
    #[serde(rename = "gateCommand")]
    pub gate_command: String,
    #[serde(rename = "benchmarkReportCommand")]
    pub benchmark_report_command: String,
    #[serde(rename = "statusCommand")]
    pub status_command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControllerOfflinePlanShard {
    pub id: String,
    pub bucket: String,
    #[serde(rename = "responseDir")]
    pub response_dir: String,
    #[serde(rename = "queuePath")]
    pub queue_path: String,
    #[serde(rename = "queueManifestPath")]
    pub queue_manifest_path: String,
    #[serde(rename = "jsonlPath")]
    pub jsonl_path: String,
    #[serde(rename = "manifestPath")]
    pub manifest_path: String,
    #[serde(rename = "expectedRows")]
    pub expected_rows: usize,
    #[serde(rename = "expectedResponseFiles")]
    pub expected_response_files: usize,
    #[serde(rename = "queueCommand")]
    pub queue_command: String,
    #[serde(rename = "verifyQueueCommand")]
    pub verify_queue_command: String,
    #[serde(rename = "runQueueCommand")]
    pub run_queue_command: String,
    #[serde(rename = "checkResponsesCommand")]
    pub check_responses_command: String,
    #[serde(rename = "scoreCommand")]
    pub score_command: String,
}

pub fn plan_controller_offline_run(
    options: ControllerOfflinePlanOptions,
) -> ControllerOfflinePlanReport {
    let cases = controller_eval_cases();
    let model_buckets = model_buckets();
    let prompt_modes = ControllerPromptMode::all()
        .into_iter()
        .map(|mode| mode.as_str().to_string())
        .collect::<Vec<_>>();
    let artifact_dir = options.artifact_dir.trim_end_matches('/').to_string();
    let prompt_bundle_dir = format!("{artifact_dir}/prompts");
    let rows_per_bucket = cases.len() * prompt_modes.len();
    let response_files_per_bucket = rows_per_bucket * RESPONSE_FILES_PER_ROW;

    let shards = model_buckets
        .iter()
        .map(|bucket| {
            let bucket_name = bucket.as_str().to_string();
            let id = format!("bucket-{bucket_name}");
            let response_dir = format!("{artifact_dir}/responses-{bucket_name}");
            let queue_path = format!("{artifact_dir}/{id}.queue.jsonl");
            let queue_manifest_path = format!("{artifact_dir}/{id}.queue.manifest.json");
            let jsonl_path = format!("{artifact_dir}/{id}.jsonl");
            let manifest_path = format!("{artifact_dir}/{id}.manifest.json");

            ControllerOfflinePlanShard {
                id,
                bucket: bucket_name.clone(),
                response_dir: response_dir.clone(),
                queue_path: queue_path.clone(),
                queue_manifest_path: queue_manifest_path.clone(),
                jsonl_path: jsonl_path.clone(),
                manifest_path: manifest_path.clone(),
                expected_rows: rows_per_bucket,
                expected_response_files: response_files_per_bucket,
                queue_command: queue_command(
                    &prompt_bundle_dir,
                    &response_dir,
                    &bucket_name,
                    &queue_path,
                    &queue_manifest_path,
                ),
                verify_queue_command: verify_queue_command(&queue_manifest_path),
                run_queue_command: run_queue_command(&queue_manifest_path, &bucket_name),
                check_responses_command: check_responses_command(&prompt_bundle_dir, &response_dir),
                score_command: score_command(
                    &prompt_bundle_dir,
                    &response_dir,
                    &bucket_name,
                    &jsonl_path,
                    &manifest_path,
                ),
            }
        })
        .collect::<Vec<_>>();

    let total_expected_rows = shards.iter().map(|shard| shard.expected_rows).sum();
    let total_expected_response_files = shards
        .iter()
        .map(|shard| shard.expected_response_files)
        .sum();
    let offline_plan_path = format!("{artifact_dir}/offline-plan.json");
    let merged_jsonl = format!("{artifact_dir}/offline-merged.jsonl");
    let merged_manifest = format!("{artifact_dir}/offline-merged.manifest.json");
    let verification_report = format!("{artifact_dir}/offline-verification.json");
    let coverage_report = format!("{artifact_dir}/offline-coverage.json");
    let gate_report = format!("{artifact_dir}/offline-gate.json");
    let benchmark_report = format!("{artifact_dir}/offline-benchmark-report.json");
    let status_report = format!("{artifact_dir}/offline-status.json");
    let finalize_report = format!("{artifact_dir}/offline-finalize-report.json");

    ControllerOfflinePlanReport {
        version: CONTROLLER_OFFLINE_PLAN_VERSION.to_string(),
        case_count: cases.len(),
        model_buckets: model_buckets
            .into_iter()
            .map(|bucket| bucket.as_str().to_string())
            .collect(),
        prompt_modes,
        grammar_payload: "gbnf".to_string(),
        prompt_bundle_dir: prompt_bundle_dir.clone(),
        response_file_pattern:
            "responses-<bucket>/cases/<prompt-mode>/<case-id>.<glyph|json-tool-plan|direct-prose>.txt"
                .to_string(),
        total_expected_rows,
        total_expected_response_files,
        merged_jsonl_path: merged_jsonl.clone(),
        merged_manifest_path: merged_manifest.clone(),
        verification_report_path: verification_report.clone(),
        coverage_report_path: coverage_report.clone(),
        gate_report_path: gate_report.clone(),
        benchmark_report_path: benchmark_report.clone(),
        status_report_path: status_report.clone(),
        finalize_report_path: finalize_report.clone(),
        prompt_bundle_command: format!(
            "cargo run -- eval-controller --prompt-mode all --grammar-payload gbnf --emit-prompts {prompt_bundle_dir}"
        ),
        verify_prompt_bundle_command: format!(
            "cargo run -- verify-controller-prompt-bundle {prompt_bundle_dir}"
        ),
        finalize_command: format!("cargo run -- finalize-controller-offline-run {offline_plan_path}"),
        verify_shards_command: format!(
            "cargo run -- verify-controller-shards --plan {offline_plan_path}"
        ),
        merge_command: merge_command(&merged_jsonl, &merged_manifest, &shards),
        coverage_command: format!("cargo run -- coverage-controller {merged_jsonl}"),
        verify_command: format!(
            "cargo run -- verify-controller-run {merged_jsonl} {merged_manifest}"
        ),
        gate_command: format!("cargo run -- gate-controller {merged_jsonl}"),
        benchmark_report_command: format!(
            "cargo run -- report-controller-benchmark {merged_jsonl} --output {benchmark_report}"
        ),
        status_command: format!(
            "cargo run -- status-controller-claim --jsonl {merged_jsonl} --manifest {merged_manifest} --require-claim-ready"
        ),
        shards,
    }
}

fn score_command(
    prompt_bundle_dir: &str,
    response_dir: &str,
    bucket: &str,
    jsonl_path: &str,
    manifest_path: &str,
) -> String {
    [
        "cargo run -- score-controller-responses".to_string(),
        format!("--prompt-bundle {prompt_bundle_dir}"),
        format!("--responses {response_dir}"),
        format!("--model-id <{bucket}-local-model-id>"),
        format!("--bucket {bucket}"),
        format!("--jsonl {jsonl_path}"),
        format!("--manifest {manifest_path}"),
    ]
    .join(" ")
}

fn queue_command(
    prompt_bundle_dir: &str,
    response_dir: &str,
    bucket: &str,
    queue_path: &str,
    queue_manifest_path: &str,
) -> String {
    [
        "cargo run -- export-controller-offline-queue".to_string(),
        format!("--prompt-bundle {prompt_bundle_dir}"),
        format!("--responses {response_dir}"),
        format!("--model-id <{bucket}-local-model-id>"),
        format!("--output {queue_path}"),
        format!("--manifest {queue_manifest_path}"),
    ]
    .join(" ")
}

fn verify_queue_command(queue_manifest_path: &str) -> String {
    format!("cargo run -- verify-controller-offline-queue {queue_manifest_path}")
}

fn run_queue_command(queue_manifest_path: &str, bucket: &str) -> String {
    format!(
        "cargo run -- run-controller-offline-queue {queue_manifest_path} --endpoint <openai-compatible-endpoint-for-{bucket}>"
    )
}

fn check_responses_command(prompt_bundle_dir: &str, response_dir: &str) -> String {
    [
        "cargo run -- check-controller-offline-responses".to_string(),
        format!("--prompt-bundle {prompt_bundle_dir}"),
        format!("--responses {response_dir}"),
    ]
    .join(" ")
}

fn merge_command(
    merged_jsonl: &str,
    merged_manifest: &str,
    shards: &[ControllerOfflinePlanShard],
) -> String {
    let mut parts = vec![
        "cargo run -- merge-controller".to_string(),
        format!("--output {merged_jsonl}"),
        format!("--manifest {merged_manifest}"),
    ];
    parts.extend(
        shards
            .iter()
            .map(|shard| format!("--source-manifest {}", shard.manifest_path)),
    );
    parts.extend(shards.iter().map(|shard| shard.jsonl_path.clone()));
    parts.join(" ")
}

fn model_buckets() -> Vec<ControllerParameterClass> {
    vec![
        ControllerParameterClass::OneB,
        ControllerParameterClass::ThreeB,
        ControllerParameterClass::SevenB,
        ControllerParameterClass::Frontier,
    ]
}
