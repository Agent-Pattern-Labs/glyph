use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::controller::{ControllerParameterClass, ControllerPromptMode};
use super::controller_examples::controller_eval_cases;

pub const CONTROLLER_LIVE_PLAN_VERSION: &str = "glyph-controller-live-plan/0.1";
const MODEL_CALLS_PER_ROW: usize = 3;

#[derive(Debug, Clone)]
pub struct ControllerLivePlanOptions {
    pub artifact_dir: String,
    pub endpoint: String,
}

impl Default for ControllerLivePlanOptions {
    fn default() -> Self {
        Self {
            artifact_dir: "out/live-shards".to_string(),
            endpoint: "http://localhost:11434/v1".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerLivePlanReport {
    pub version: String,
    #[serde(rename = "caseCount")]
    pub case_count: usize,
    #[serde(rename = "familyCount")]
    pub family_count: usize,
    #[serde(rename = "modelBuckets")]
    pub model_buckets: Vec<String>,
    #[serde(rename = "promptModes")]
    pub prompt_modes: Vec<String>,
    #[serde(rename = "grammarPayload")]
    pub grammar_payload: String,
    #[serde(rename = "totalExpectedRows")]
    pub total_expected_rows: usize,
    #[serde(rename = "totalExpectedModelCalls")]
    pub total_expected_model_calls: usize,
    pub shards: Vec<ControllerLivePlanShard>,
    #[serde(rename = "probeEndpointCommand")]
    pub probe_endpoint_command: String,
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
pub struct ControllerLivePlanShard {
    pub id: String,
    pub family: String,
    #[serde(rename = "caseCount")]
    pub case_count: usize,
    pub profiles: Vec<String>,
    #[serde(rename = "jsonlPath")]
    pub jsonl_path: String,
    #[serde(rename = "manifestPath")]
    pub manifest_path: String,
    #[serde(rename = "expectedRows")]
    pub expected_rows: usize,
    #[serde(rename = "expectedModelCalls")]
    pub expected_model_calls: usize,
    #[serde(rename = "preflightCommand")]
    pub preflight_command: String,
    #[serde(rename = "evalCommand")]
    pub eval_command: String,
}

pub fn plan_controller_live_run(options: ControllerLivePlanOptions) -> ControllerLivePlanReport {
    let cases = controller_eval_cases();
    let model_buckets = model_buckets();
    let prompt_modes = ControllerPromptMode::all()
        .into_iter()
        .map(|mode| mode.as_str().to_string())
        .collect::<Vec<_>>();
    let families = cases_by_family();
    let artifact_dir = options.artifact_dir.trim_end_matches('/').to_string();

    let shards = families
        .into_iter()
        .map(|(family, family_cases)| {
            let profiles = family_cases
                .iter()
                .flat_map(|case| &case.tags)
                .filter_map(|tag| tag.strip_prefix("profile:").map(ToString::to_string))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let id = format!("family-{family}");
            let jsonl_path = format!("{artifact_dir}/{id}.jsonl");
            let manifest_path = format!("{artifact_dir}/{id}.manifest.json");
            let expected_rows = family_cases.len() * model_buckets.len() * prompt_modes.len();
            let expected_model_calls = expected_rows * MODEL_CALLS_PER_ROW;

            ControllerLivePlanShard {
                id,
                family: family.clone(),
                case_count: family_cases.len(),
                profiles,
                jsonl_path: jsonl_path.clone(),
                manifest_path: manifest_path.clone(),
                expected_rows,
                expected_model_calls,
                preflight_command: live_command(
                    "preflight-controller",
                    None,
                    &family,
                    &jsonl_path,
                    &manifest_path,
                ),
                eval_command: live_command(
                    "eval-controller",
                    Some(&options.endpoint),
                    &family,
                    &jsonl_path,
                    &manifest_path,
                ),
            }
        })
        .collect::<Vec<_>>();

    let total_expected_rows = shards.iter().map(|shard| shard.expected_rows).sum();
    let total_expected_model_calls = shards.iter().map(|shard| shard.expected_model_calls).sum();
    let live_plan_path = format!("{artifact_dir}/live-plan.json");
    let merged_jsonl = format!("{artifact_dir}/live-merged.jsonl");
    let merged_manifest = format!("{artifact_dir}/live-merged.manifest.json");

    ControllerLivePlanReport {
        version: CONTROLLER_LIVE_PLAN_VERSION.to_string(),
        case_count: cases.len(),
        family_count: shards.len(),
        model_buckets: model_buckets
            .into_iter()
            .map(|bucket| bucket.as_str().to_string())
            .collect(),
        prompt_modes,
        grammar_payload: "gbnf".to_string(),
        total_expected_rows,
        total_expected_model_calls,
        probe_endpoint_command: probe_endpoint_command(&options.endpoint),
        verify_shards_command: format!(
            "cargo run -- verify-controller-shards --plan {live_plan_path}"
        ),
        merge_command: merge_command(&merged_jsonl, &merged_manifest, &shards),
        coverage_command: format!("cargo run -- coverage-controller {merged_jsonl}"),
        verify_command: format!(
            "cargo run -- verify-controller-run {merged_jsonl} {merged_manifest}"
        ),
        gate_command: format!("cargo run -- gate-controller {merged_jsonl}"),
        benchmark_report_command: format!(
            "cargo run -- report-controller-benchmark {merged_jsonl} --output {artifact_dir}/live-benchmark-report.json"
        ),
        status_command: format!(
            "cargo run -- status-controller-claim --jsonl {merged_jsonl} --manifest {merged_manifest} --require-claim-ready"
        ),
        shards,
    }
}

fn cases_by_family() -> BTreeMap<String, Vec<super::controller_examples::ControllerEvalCase>> {
    let mut families = BTreeMap::<String, Vec<_>>::new();
    for case in controller_eval_cases() {
        if let Some(family) = case
            .tags
            .iter()
            .find_map(|tag| tag.strip_prefix("family:").map(ToString::to_string))
        {
            families.entry(family).or_default().push(case);
        }
    }
    families
}

fn model_buckets() -> Vec<ControllerParameterClass> {
    vec![
        ControllerParameterClass::OneB,
        ControllerParameterClass::ThreeB,
        ControllerParameterClass::SevenB,
        ControllerParameterClass::Frontier,
    ]
}

fn probe_endpoint_command(endpoint: &str) -> String {
    [
        "cargo run -- probe-controller-endpoint".to_string(),
        format!("--endpoint {endpoint}"),
        "--prompt-mode all".to_string(),
        "--grammar-payload gbnf".to_string(),
        "--model 1b=<one-billion-ish-model>".to_string(),
        "--model 3b=<three-billion-ish-model>".to_string(),
        "--model 7b=<seven-billion-ish-model>".to_string(),
        "--model frontier=<frontier-model>".to_string(),
        "--case hello_summary_normal_short".to_string(),
    ]
    .join(" ")
}

fn live_command(
    command: &str,
    endpoint: Option<&str>,
    family: &str,
    jsonl_path: &str,
    manifest_path: &str,
) -> String {
    let mut parts = vec![
        "cargo run --".to_string(),
        command.to_string(),
        "--prompt-mode all".to_string(),
        "--grammar-payload gbnf".to_string(),
    ];

    if command == "eval-controller" {
        parts.push("--adapter openai-compatible".to_string());
    }
    if let Some(endpoint) = endpoint {
        parts.push(format!("--endpoint {endpoint}"));
    }

    parts.extend([
        "--model 1b=<one-billion-ish-model>".to_string(),
        "--model-evidence 1b=\"model card or provider docs show roughly 1B parameters\""
            .to_string(),
        "--model 3b=<three-billion-ish-model>".to_string(),
        "--model-evidence 3b=\"model card or provider docs show roughly 3B parameters\""
            .to_string(),
        "--model 7b=<seven-billion-ish-model>".to_string(),
        "--model-evidence 7b=\"model card or provider docs show roughly 7B parameters\""
            .to_string(),
        "--model frontier=<frontier-model>".to_string(),
        "--model-evidence frontier=\"provider docs identify this as the frontier reference\""
            .to_string(),
        format!("--family {family}"),
        format!("--jsonl {jsonl_path}"),
        "--stream-jsonl".to_string(),
        format!("--manifest {manifest_path}"),
    ]);

    parts.join(" ")
}

fn merge_command(
    merged_jsonl: &str,
    merged_manifest: &str,
    shards: &[ControllerLivePlanShard],
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
