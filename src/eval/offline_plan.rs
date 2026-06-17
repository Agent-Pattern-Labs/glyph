use serde::Serialize;

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

#[derive(Debug, Clone, Serialize)]
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
    pub shards: Vec<ControllerOfflinePlanShard>,
    #[serde(rename = "promptBundleCommand")]
    pub prompt_bundle_command: String,
    #[serde(rename = "verifyPromptBundleCommand")]
    pub verify_prompt_bundle_command: String,
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

#[derive(Debug, Clone, Serialize)]
pub struct ControllerOfflinePlanShard {
    pub id: String,
    pub bucket: String,
    #[serde(rename = "responseDir")]
    pub response_dir: String,
    #[serde(rename = "jsonlPath")]
    pub jsonl_path: String,
    #[serde(rename = "manifestPath")]
    pub manifest_path: String,
    #[serde(rename = "expectedRows")]
    pub expected_rows: usize,
    #[serde(rename = "expectedResponseFiles")]
    pub expected_response_files: usize,
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
            let jsonl_path = format!("{artifact_dir}/{id}.jsonl");
            let manifest_path = format!("{artifact_dir}/{id}.manifest.json");

            ControllerOfflinePlanShard {
                id,
                bucket: bucket_name.clone(),
                response_dir: response_dir.clone(),
                jsonl_path: jsonl_path.clone(),
                manifest_path: manifest_path.clone(),
                expected_rows: rows_per_bucket,
                expected_response_files: response_files_per_bucket,
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
        prompt_bundle_command: format!(
            "cargo run -- eval-controller --prompt-mode all --grammar-payload gbnf --emit-prompts {prompt_bundle_dir}"
        ),
        verify_prompt_bundle_command: format!(
            "cargo run -- verify-controller-prompt-bundle {prompt_bundle_dir}"
        ),
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
            "cargo run -- report-controller-benchmark {merged_jsonl} --output {artifact_dir}/offline-benchmark-report.json"
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
