use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, BufRead, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use glyph::eval::benchmark_report::controller_benchmark_report;
use glyph::eval::compression::compare_compression;
use glyph::eval::conformance::glyph_conformance_report;
use glyph::eval::controller::{
    ControllerEvalCaseFilter, ControllerEvalCaseResult, ControllerEvalOptions,
    ControllerEvalReport, ControllerGrammarPayload, ControllerOfflineResponseKey,
    ControllerOfflineResponseSet, ControllerParameterClass, ControllerPromptMode,
    ControllerRequestKind, GENERIC_TOOL_PLAN_JSON_SCHEMA, build_controller_prompt_with_payload,
    build_direct_prose_prompt, build_json_tool_plan_prompt, build_openai_compatible_request_body,
    create_offline_response_controller_model, create_openai_compatible_controller_models,
    run_controller_eval_with_observer, run_controller_eval_with_options,
    select_controller_eval_cases,
};
use glyph::eval::coverage::controller_eval_coverage;
use glyph::eval::curriculum::{
    CONTROLLER_CURRICULUM_VERSION, ControllerCurriculumOptions, ControllerCurriculumRecord,
    assess_controller_curriculum_quality, export_controller_curriculum,
};
use glyph::eval::dataset::{
    CONTROLLER_DATASET_VERSION, ControllerDatasetOptions, ControllerDatasetRecord,
    export_controller_dataset,
};
use glyph::eval::dataset_quality::assess_controller_dataset_quality;
use glyph::eval::evidence::{ControllerClaimAuditInput, audit_controller_claim};
use glyph::eval::examples::find_compression_example;
use glyph::eval::fingerprint::controller_eval_fingerprint;
use glyph::eval::gate::evaluate_controller_gate;
use glyph::eval::live_plan::{
    CONTROLLER_LIVE_PLAN_VERSION, ControllerLivePlanOptions, plan_controller_live_run,
};
use glyph::eval::manifest::{
    ControllerEvalMergedManifestInput, ControllerEvalRunArtifacts, ControllerEvalRunCaseFilter,
    ControllerEvalRunConfig, ControllerEvalRunModel, ControllerEvalSourceManifest,
    build_controller_eval_run_manifest, build_merged_controller_eval_manifest,
};
use glyph::eval::offline_plan::{
    CONTROLLER_OFFLINE_PLAN_VERSION, ControllerOfflinePlanOptions, plan_controller_offline_run,
};
use glyph::eval::preflight::{
    ControllerPreflightModel, ControllerPreflightOptions, preflight_controller_eval,
};
use glyph::eval::results::merge_controller_eval_cases;
use glyph::eval::robustness::evaluate_controller_robustness;
use glyph::eval::status::{
    ControllerClaimStatusInput, controller_claim_status, controller_claim_status_from_audit,
};
use glyph::eval::verify::{ControllerRunVerificationReport, verify_controller_run};
use glyph::harness::mock_tools::create_mock_tool_registry;
use glyph::ir::glyph_ir::parse_glyph_to_ir;
use glyph::ir::validate_ir::validate_ir;
use glyph::language::formatter::format_glyph;
use glyph::language::grammar::{
    GLYPH_CONTROLLER_OUTPUT_JSON_SCHEMA, GLYPH_GBNF, get_grammar_artifact,
};
use glyph::language::parser::parse_glyph;
use glyph::runtime::glyph_vm::GlyphVm;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

#[derive(Debug, Parser)]
#[command(name = "glyph", version, about = "GlyphVM CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
#[allow(clippy::large_enum_variant)]
enum Commands {
    /// Parse a .glyph file and print AST and/or IR.
    Parse {
        file: String,
        #[arg(long)]
        ast: bool,
        #[arg(long)]
        ir: bool,
    },
    /// Execute a .glyph program with mock harness tools.
    Run { file: String },
    /// Format Glyph source.
    Format {
        file: String,
        #[arg(short, long)]
        write: bool,
    },
    /// Parse and validate a .glyph file without running it.
    Check { file: String },
    /// Compare Glyph source length against a verbose natural-language equivalent.
    Compress { file: String },
    /// Print official Glyph grammar artifacts for constrained decoding.
    Grammar {
        #[arg(short, long, value_enum, default_value_t = GrammarFormat::Ebnf)]
        format: GrammarFormat,
    },
    /// Print a canonical spec artifact.
    Spec { artifact: String },
    /// Check example programs against the parser, IR validator, and mock runtime.
    CheckConformance {
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long)]
        no_fail: bool,
    },
    /// Run the controller eval harness with fixture or OpenAI-compatible adapters.
    EvalController {
        #[arg(long, value_enum, default_value_t = EvalAdapter::Fixture)]
        adapter: EvalAdapter,
        #[arg(long, default_value = "http://localhost:11434/v1")]
        endpoint: String,
        #[arg(long, default_value = "GLYPH_EVAL_API_KEY")]
        api_key_env: String,
        #[arg(long, value_enum, default_value_t = EvalPromptMode::Constrained)]
        prompt_mode: EvalPromptMode,
        #[arg(long, value_enum, default_value_t = EvalGrammarPayload::None)]
        grammar_payload: EvalGrammarPayload,
        #[arg(short, long)]
        model: Vec<String>,
        #[arg(long)]
        case: Vec<String>,
        #[arg(long)]
        tag: Vec<String>,
        #[arg(long)]
        family: Vec<String>,
        #[arg(long)]
        profile: Vec<String>,
        #[arg(long)]
        case_limit: Option<usize>,
        #[arg(long)]
        emit_prompts: Option<PathBuf>,
        #[arg(long)]
        jsonl: Option<PathBuf>,
        #[arg(long, requires = "jsonl")]
        stream_jsonl: bool,
        #[arg(long)]
        manifest: Option<PathBuf>,
    },
    /// Preview OpenAI-compatible controller request bodies without making model calls.
    PreviewControllerRequests {
        #[arg(long, default_value = "model-under-test")]
        model_id: String,
        #[arg(long, value_enum, default_value_t = EvalPromptMode::Constrained)]
        prompt_mode: EvalPromptMode,
        #[arg(long, value_enum, default_value_t = EvalGrammarPayload::None)]
        grammar_payload: EvalGrammarPayload,
        #[arg(long)]
        case: Vec<String>,
        #[arg(long)]
        tag: Vec<String>,
        #[arg(long)]
        family: Vec<String>,
        #[arg(long)]
        profile: Vec<String>,
        #[arg(long)]
        case_limit: Option<usize>,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Recompute and verify a constrained-decoding prompt bundle manifest.
    VerifyControllerPromptBundle {
        directory: PathBuf,
        #[arg(long)]
        no_fail: bool,
    },
    /// Check saved local-decoder response files before scoring an offline run.
    CheckControllerOfflineResponses {
        #[arg(long)]
        prompt_bundle: PathBuf,
        #[arg(long)]
        responses: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long)]
        no_fail: bool,
    },
    /// Export a JSONL job queue for local decoders to fill offline response files.
    ExportControllerOfflineQueue {
        #[arg(long)]
        prompt_bundle: PathBuf,
        #[arg(long)]
        responses: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        manifest: Option<PathBuf>,
    },
    /// Score saved local-decoder responses against a sealed controller prompt bundle.
    ScoreControllerResponses {
        #[arg(long)]
        prompt_bundle: PathBuf,
        #[arg(long)]
        responses: PathBuf,
        #[arg(long)]
        model_id: String,
        #[arg(long, value_enum)]
        bucket: EvalParameterClass,
        #[arg(long, requires = "manifest")]
        jsonl: Option<PathBuf>,
        #[arg(long, requires = "jsonl")]
        manifest: Option<PathBuf>,
    },
    /// Print stable hashes for controller eval specs and corpus.
    FingerprintController,
    /// Check the current controller fingerprint against the committed stability lock.
    CheckControllerFingerprintLock {
        #[arg(long, default_value = "spec/controller-fingerprint.lock.json")]
        lock: PathBuf,
        #[arg(long)]
        no_fail: bool,
    },
    /// Export deterministic controller training records from the eval corpus.
    ExportControllerDataset {
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long, requires = "output")]
        manifest: Option<PathBuf>,
        #[arg(long)]
        case: Vec<String>,
        #[arg(long)]
        tag: Vec<String>,
        #[arg(long)]
        family: Vec<String>,
        #[arg(long)]
        profile: Vec<String>,
        #[arg(long)]
        case_limit: Option<usize>,
        #[arg(long, default_value_t = 8)]
        validation_stride: usize,
        #[arg(long)]
        no_validation_split: bool,
    },
    /// Check deterministic controller dataset quality before training.
    CheckControllerDataset {
        #[arg(long)]
        case: Vec<String>,
        #[arg(long)]
        tag: Vec<String>,
        #[arg(long)]
        family: Vec<String>,
        #[arg(long)]
        profile: Vec<String>,
        #[arg(long)]
        case_limit: Option<usize>,
        #[arg(long, default_value_t = 8)]
        validation_stride: usize,
        #[arg(long)]
        no_validation_split: bool,
        #[arg(long)]
        no_fail: bool,
    },
    /// Export controller curriculum records with positive, repair, and rejection examples.
    ExportControllerCurriculum {
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long, requires = "output")]
        manifest: Option<PathBuf>,
        #[arg(long)]
        case: Vec<String>,
        #[arg(long)]
        tag: Vec<String>,
        #[arg(long)]
        family: Vec<String>,
        #[arg(long)]
        profile: Vec<String>,
        #[arg(long)]
        case_limit: Option<usize>,
        #[arg(long, default_value_t = 8)]
        validation_stride: usize,
        #[arg(long)]
        no_validation_split: bool,
    },
    /// Check controller curriculum readiness before tiny-model training.
    CheckControllerCurriculum {
        #[arg(long)]
        case: Vec<String>,
        #[arg(long)]
        tag: Vec<String>,
        #[arg(long)]
        family: Vec<String>,
        #[arg(long)]
        profile: Vec<String>,
        #[arg(long)]
        case_limit: Option<usize>,
        #[arg(long, default_value_t = 8)]
        validation_stride: usize,
        #[arg(long)]
        no_validation_split: bool,
        #[arg(long)]
        no_fail: bool,
    },
    /// Recompute and verify a controller dataset or curriculum export manifest.
    VerifyControllerTrainingExport {
        manifest: PathBuf,
        #[arg(long)]
        no_fail: bool,
    },
    /// Check that invalid controller outputs are rejected by parser and semantic validation.
    CheckControllerRobustness {
        #[arg(long)]
        no_fail: bool,
    },
    /// Audit whether supplied evidence supports the best-in-lane controller claim.
    AuditControllerClaim {
        #[arg(long)]
        jsonl: Option<PathBuf>,
        #[arg(long)]
        manifest: Option<PathBuf>,
        #[arg(long)]
        no_fail: bool,
    },
    /// Print machine-readable claim status and blocking reasons.
    StatusControllerClaim {
        #[arg(long)]
        jsonl: Option<PathBuf>,
        #[arg(long)]
        manifest: Option<PathBuf>,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long)]
        require_claim_ready: bool,
    },
    /// Export a reviewable controller evidence pack.
    ExportControllerEvidencePack {
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        jsonl: Option<PathBuf>,
        #[arg(long)]
        manifest: Option<PathBuf>,
        #[arg(long, default_value = "model-under-test")]
        model_id: String,
        #[arg(long, default_value_t = 1)]
        request_preview_limit: usize,
        #[arg(long)]
        require_claim_ready: bool,
    },
    /// Recompute and verify a controller evidence pack manifest.
    VerifyControllerEvidencePack {
        directory: PathBuf,
        #[arg(long)]
        no_fail: bool,
    },
    /// Validate a planned controller eval before making model calls.
    PreflightController {
        #[arg(long, value_enum, default_value_t = EvalAdapter::OpenaiCompatible)]
        adapter: EvalAdapter,
        #[arg(long, value_enum, default_value_t = EvalPromptMode::Constrained)]
        prompt_mode: EvalPromptMode,
        #[arg(long, value_enum, default_value_t = EvalGrammarPayload::None)]
        grammar_payload: EvalGrammarPayload,
        #[arg(short, long)]
        model: Vec<String>,
        #[arg(long)]
        case: Vec<String>,
        #[arg(long)]
        tag: Vec<String>,
        #[arg(long)]
        family: Vec<String>,
        #[arg(long)]
        profile: Vec<String>,
        #[arg(long)]
        case_limit: Option<usize>,
        #[arg(long)]
        jsonl: Option<PathBuf>,
        #[arg(long)]
        stream_jsonl: bool,
        #[arg(long)]
        manifest: Option<PathBuf>,
        #[arg(long)]
        no_fail: bool,
    },
    /// Generate a staged live-eval shard plan for collecting benchmark evidence.
    PlanControllerLiveRun {
        #[arg(long, default_value = "out/live-shards")]
        artifact_dir: String,
        #[arg(long, default_value = "http://localhost:11434/v1")]
        endpoint: String,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Generate a staged offline-response eval plan for local decoder benchmark evidence.
    PlanControllerOfflineRun {
        #[arg(long, default_value = "out/offline-shards")]
        artifact_dir: String,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Verify a controller JSONL trace matches its manifest and current benchmark fingerprint.
    VerifyControllerRun {
        jsonl: PathBuf,
        manifest: PathBuf,
        #[arg(long)]
        no_fail: bool,
    },
    /// Verify every shard listed in a staged live-run plan before merging.
    VerifyControllerShards {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        no_fail: bool,
    },
    /// Evaluate controller JSONL results against the best-in-lane benchmark gate.
    GateController {
        jsonl: PathBuf,
        #[arg(long)]
        no_fail: bool,
    },
    /// Produce a benchmark comparison report from controller JSONL results.
    ReportControllerBenchmark {
        jsonl: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long)]
        no_fail: bool,
    },
    /// Report missing live controller eval rows needed for the benchmark gate.
    CoverageController { jsonl: PathBuf },
    /// Merge and dedupe staged controller JSONL result files.
    MergeController {
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        manifest: Option<PathBuf>,
        #[arg(long = "source-manifest")]
        source_manifest: Vec<PathBuf>,
        #[arg(required = true)]
        jsonl: Vec<PathBuf>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum GrammarFormat {
    Ebnf,
    Gbnf,
    JsonSchema,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
enum EvalAdapter {
    Fixture,
    OpenaiCompatible,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum EvalParameterClass {
    #[value(name = "1b")]
    OneB,
    #[value(name = "3b")]
    ThreeB,
    #[value(name = "7b")]
    SevenB,
    Frontier,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum EvalPromptMode {
    Constrained,
    SchemaOnly,
    Plain,
    All,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum EvalGrammarPayload {
    None,
    Gbnf,
}

impl GrammarFormat {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ebnf => "ebnf",
            Self::Gbnf => "gbnf",
            Self::JsonSchema => "json-schema",
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Parse { file, ast, ir } => {
            let source = read_glyph_file(&file)?.source;
            let ast_value = parse_glyph(&source)?;
            let ir_value = validate_ir(parse_glyph_to_ir(&source)?)?;

            if ast && ir {
                print_json(&json!({
                    "ast": ast_value,
                    "ir": ir_value
                }))?;
            } else if ast {
                print_json(&ast_value)?;
            } else {
                print_json(&ir_value)?;
            }
        }
        Commands::Run { file } => {
            let source = read_glyph_file(&file)?.source;
            let vm = GlyphVm::new(create_mock_tool_registry());
            let result = vm.run_source(&source)?;
            print_json(&json!({
                "trace": result.trace,
                "outputs": result.outputs,
                "variables": result.variables
            }))?;
        }
        Commands::Format { file, write } => {
            let glyph_file = read_glyph_file(&file)?;
            let formatted = format_glyph(&glyph_file.source)?;
            if write {
                fs::write(&glyph_file.path, formatted)
                    .with_context(|| format!("Failed to write {}", glyph_file.path.display()))?;
                println!("Formatted {}", display_path(&glyph_file.path));
            } else {
                write_stdout(&formatted)?;
            }
        }
        Commands::Check { file } => {
            let glyph_file = read_glyph_file(&file)?;
            validate_ir(parse_glyph_to_ir(&glyph_file.source)?)?;
            println!("OK {}", display_path(&glyph_file.path));
        }
        Commands::Compress { file } => {
            let glyph_file = read_glyph_file(&file)?;
            let example = find_compression_example(&file)
                .with_context(|| format!("No compression eval example registered for {file}"))?;
            print_json(&json!({
                "example": example.name,
                "stats": compare_compression(&glyph_file.source, example)
            }))?;
        }
        Commands::Grammar { format } => {
            let artifact = get_grammar_artifact(format.as_str())
                .with_context(|| format!("Unsupported grammar format {}", format.as_str()))?;
            write_stdout(artifact)?;
        }
        Commands::Spec { artifact } => {
            let allowed = [
                "glyph.ebnf",
                "glyph.gbnf",
                "controller-output.schema.json",
                "generic-tool-plan.schema.json",
                "glyph-ir.schema.json",
            ];
            if !allowed.contains(&artifact.as_str()) {
                bail!("Unknown spec artifact: {artifact}");
            }
            write_stdout(
                &fs::read_to_string(Path::new("spec").join(&artifact))
                    .with_context(|| format!("Failed to read spec artifact {artifact}"))?,
            )?;
        }
        Commands::CheckConformance { output, no_fail } => {
            let report = glyph_conformance_report();

            if let Some(output) = output {
                write_json_file(&output, &report)?;
                print_json(&json!({
                    "passed": report.passed,
                    "exampleCount": report.example_count,
                    "parsePassed": report.parse_passed,
                    "validationPassed": report.validation_passed,
                    "runPassed": report.run_passed,
                    "output": output
                }))?;
            } else {
                print_json(&report)?;
            }

            if !no_fail && !report.passed {
                bail!("Glyph conformance check did not pass");
            }
        }
        Commands::EvalController {
            adapter,
            endpoint,
            api_key_env,
            prompt_mode,
            grammar_payload,
            model,
            case,
            tag,
            family,
            profile,
            case_limit,
            emit_prompts,
            jsonl,
            stream_jsonl,
            manifest,
        } => {
            let prompt_modes = resolve_prompt_modes(prompt_mode);
            let grammar_payload = resolve_grammar_payload(grammar_payload);
            let case_filter = ControllerEvalCaseFilter {
                case_ids: case,
                tags: tag,
                families: family,
                profiles: profile,
                limit: case_limit,
            };
            let emit_prompts_path = emit_prompts.clone();
            let mut prompt_bundle_overall_sha256 = None;
            let mut prompt_bundle_manifest_hash = None;
            if let Some(output_dir) = emit_prompts {
                emit_prompt_bundle(&output_dir, &prompt_modes, grammar_payload, &case_filter)?;
                let prompt_bundle = read_prompt_bundle_manifest(&output_dir)?;
                prompt_bundle_overall_sha256 = Some(prompt_bundle.overall_sha256);
                prompt_bundle_manifest_hash = Some(prompt_bundle_manifest_sha256(&output_dir)?);
            }

            let selected_case_ids = select_controller_eval_cases(&case_filter)
                .into_iter()
                .map(|eval_case| eval_case.id)
                .collect::<Vec<_>>();
            let started_at_unix_seconds = current_unix_seconds()?;
            let git_commit = current_git_commit();
            let git_tree_dirty = current_git_tree_dirty();

            let adapter_mode = match adapter {
                EvalAdapter::Fixture => glyph::eval::controller::ControllerAdapterMode::Fixture,
                EvalAdapter::OpenaiCompatible => {
                    glyph::eval::controller::ControllerAdapterMode::OpenAiCompatible
                }
            };
            let mut api_key_env_for_manifest = None;
            let mut api_key_provided = false;

            let (options, manifest_models) = match adapter {
                EvalAdapter::Fixture => {
                    let models = glyph::eval::controller::create_fixture_controller_models();
                    (
                        ControllerEvalOptions {
                            models: None,
                            prompt_modes: prompt_modes.clone(),
                            case_filter: case_filter.clone(),
                        },
                        models
                            .into_iter()
                            .map(|model| ControllerEvalRunModel {
                                parameter_class: model.parameter_class,
                                model_id: model.id,
                            })
                            .collect::<Vec<_>>(),
                    )
                }
                EvalAdapter::OpenaiCompatible => {
                    api_key_env_for_manifest = Some(api_key_env.clone());
                    let api_key = std::env::var(&api_key_env).ok();
                    api_key_provided = api_key.is_some();
                    let models = create_openai_compatible_controller_models(
                        endpoint.clone(),
                        api_key,
                        grammar_payload,
                        resolve_model_mappings(&model)?,
                    );
                    let manifest_models = models
                        .iter()
                        .map(|model| ControllerEvalRunModel {
                            parameter_class: model.parameter_class,
                            model_id: model.id.clone(),
                        })
                        .collect::<Vec<_>>();
                    (
                        ControllerEvalOptions {
                            models: Some(models),
                            prompt_modes: prompt_modes.clone(),
                            case_filter: case_filter.clone(),
                        },
                        manifest_models,
                    )
                }
            };
            let manifest_config = ControllerEvalRunConfig {
                adapter_mode,
                endpoint: matches!(adapter, EvalAdapter::OpenaiCompatible)
                    .then_some(endpoint.clone()),
                api_key_env: api_key_env_for_manifest,
                api_key_provided,
                models: manifest_models,
                prompt_modes: prompt_modes.clone(),
                grammar_payload,
                case_filter: ControllerEvalRunCaseFilter::from(&case_filter),
                selected_case_count: selected_case_ids.len(),
                selected_case_ids,
                artifacts: ControllerEvalRunArtifacts {
                    jsonl_path: jsonl.as_ref().map(|path| path.display().to_string()),
                    manifest_path: manifest.as_ref().map(|path| path.display().to_string()),
                    emit_prompts_path: emit_prompts_path
                        .as_ref()
                        .map(|path| path.display().to_string()),
                    prompt_bundle_overall_sha256,
                    prompt_bundle_manifest_sha256: prompt_bundle_manifest_hash,
                    response_bundle_path: None,
                    response_bundle_file_count: None,
                    response_bundle_bytes: None,
                    response_bundle_sha256: None,
                    stream_jsonl,
                },
            };

            if let Some(path) = &manifest {
                let planned_manifest = build_controller_eval_run_manifest(
                    started_at_unix_seconds,
                    None,
                    env!("CARGO_PKG_VERSION"),
                    git_commit.clone(),
                    git_tree_dirty,
                    manifest_config.clone(),
                    None,
                );
                write_json_file(path, &planned_manifest)?;
            }

            let report = if stream_jsonl {
                let path = jsonl
                    .as_ref()
                    .expect("clap requires --jsonl when --stream-jsonl is set");
                let mut writer = create_eval_jsonl_writer(path)?;
                run_controller_eval_with_observer(options, |case| {
                    write_eval_jsonl_case(&mut writer, case)?;
                    writer.flush()?;
                    Ok::<(), anyhow::Error>(())
                })?
            } else {
                run_controller_eval_with_options(options)
            };

            if let Some(path) = jsonl
                && !stream_jsonl
            {
                write_eval_jsonl(&path, &report.cases)?;
            }

            if let Some(path) = &manifest {
                let completed_manifest = build_controller_eval_run_manifest(
                    started_at_unix_seconds,
                    Some(current_unix_seconds()?),
                    env!("CARGO_PKG_VERSION"),
                    git_commit,
                    git_tree_dirty,
                    manifest_config,
                    Some(&report),
                );
                write_json_file(path, &completed_manifest)?;
            }

            print_json(&report)?;
        }
        Commands::PreviewControllerRequests {
            model_id,
            prompt_mode,
            grammar_payload,
            case,
            tag,
            family,
            profile,
            case_limit,
            output,
        } => {
            let prompt_modes = resolve_prompt_modes(prompt_mode);
            let grammar_payload = resolve_grammar_payload(grammar_payload);
            let case_filter = ControllerEvalCaseFilter {
                case_ids: case,
                tags: tag,
                families: family,
                profiles: profile,
                limit: case_limit,
            };
            let preview = preview_controller_requests(
                &model_id,
                &prompt_modes,
                grammar_payload,
                &case_filter,
            );

            if let Some(output) = output {
                write_json_file(&output, &preview)?;
                print_json(&json!({
                    "modelId": model_id,
                    "promptModes": prompt_modes.iter().map(|mode| mode.as_str()).collect::<Vec<_>>(),
                    "grammarPayload": grammar_payload.as_str(),
                    "caseCount": preview["caseCount"],
                    "requestCount": preview["requestCount"],
                    "output": output
                }))?;
            } else {
                print_json(&preview)?;
            }
        }
        Commands::VerifyControllerPromptBundle { directory, no_fail } => {
            let report = verify_prompt_bundle(&directory)?;
            print_json(&report)?;

            if !report.passed && !no_fail {
                bail!("Controller prompt bundle verification failed");
            }
        }
        Commands::CheckControllerOfflineResponses {
            prompt_bundle,
            responses,
            output,
            no_fail,
        } => {
            let report = check_controller_offline_responses(&prompt_bundle, &responses)?;
            if let Some(output) = output {
                write_json_file(&output, &report)?;
                print_json(&json!({
                    "passed": report.passed,
                    "promptFileCount": report.prompt_file_count,
                    "expectedResponseFileCount": report.expected_response_file_count,
                    "presentResponseFileCount": report.present_response_file_count,
                    "missingResponseFileCount": report.missing_response_file_count,
                    "extraResponseFileCount": report.extra_response_file_count,
                    "output": output
                }))?;
            } else {
                print_json(&report)?;
            }

            if !report.passed && !no_fail {
                bail!("Controller offline response check failed");
            }
        }
        Commands::ExportControllerOfflineQueue {
            prompt_bundle,
            responses,
            output,
            manifest,
        } => {
            let report = export_controller_offline_queue(
                &prompt_bundle,
                &responses,
                &output,
                manifest.as_deref(),
            )?;
            print_json(&report)?;
        }
        Commands::ScoreControllerResponses {
            prompt_bundle,
            responses,
            model_id,
            bucket,
            jsonl,
            manifest,
        } => {
            let report = score_controller_response_bundle(
                &prompt_bundle,
                &responses,
                &model_id,
                resolve_parameter_class(bucket),
                jsonl.as_deref(),
                manifest.as_deref(),
            )?;
            print_json(&report)?;
        }
        Commands::FingerprintController => {
            print_json(&controller_eval_fingerprint())?;
        }
        Commands::CheckControllerFingerprintLock { lock, no_fail } => {
            let report = check_controller_fingerprint_lock(&lock)?;
            print_json(&report)?;

            if !report.passed && !no_fail {
                bail!("Controller fingerprint lock check did not pass");
            }
        }
        Commands::ExportControllerDataset {
            output,
            manifest,
            case,
            tag,
            family,
            profile,
            case_limit,
            validation_stride,
            no_validation_split,
        } => {
            let options = dataset_options(
                case,
                tag,
                family,
                profile,
                case_limit,
                validation_stride,
                no_validation_split,
            );
            let export = export_controller_dataset(options.clone()).map_err(anyhow::Error::msg)?;

            if let Some(output) = output {
                write_dataset_jsonl(&output, &export.records)?;
                let manifest_path = manifest
                    .as_ref()
                    .map(|path| {
                        write_training_export_manifest(
                            path,
                            "dataset",
                            &output,
                            &export.version,
                            json!({
                                "recordCount": export.record_count,
                                "trainRecords": export.train_records,
                                "validationRecords": export.validation_records
                            }),
                            &options,
                        )
                    })
                    .transpose()?;
                print_json(&json!({
                    "version": export.version,
                    "recordCount": export.record_count,
                    "trainRecords": export.train_records,
                    "validationRecords": export.validation_records,
                    "output": output,
                    "manifest": manifest_path.map(|path| path.display().to_string())
                }))?;
            } else {
                print_json(&export)?;
            }
        }
        Commands::CheckControllerDataset {
            case,
            tag,
            family,
            profile,
            case_limit,
            validation_stride,
            no_validation_split,
            no_fail,
        } => {
            let export = export_controller_dataset(dataset_options(
                case,
                tag,
                family,
                profile,
                case_limit,
                validation_stride,
                no_validation_split,
            ))
            .map_err(anyhow::Error::msg)?;
            let report = assess_controller_dataset_quality(&export);

            print_json(&report)?;

            if !report.passed && !no_fail {
                bail!("Controller dataset quality check did not pass");
            }
        }
        Commands::ExportControllerCurriculum {
            output,
            manifest,
            case,
            tag,
            family,
            profile,
            case_limit,
            validation_stride,
            no_validation_split,
        } => {
            let options = dataset_options(
                case,
                tag,
                family,
                profile,
                case_limit,
                validation_stride,
                no_validation_split,
            );
            let export = export_controller_curriculum(ControllerCurriculumOptions {
                dataset_options: options.clone(),
            })
            .map_err(anyhow::Error::msg)?;

            if let Some(output) = output {
                write_curriculum_jsonl(&output, &export.records)?;
                let manifest_path = manifest
                    .as_ref()
                    .map(|path| {
                        write_training_export_manifest(
                            path,
                            "curriculum",
                            &output,
                            &export.version,
                            json!({
                                "recordCount": export.record_count,
                                "caseCount": export.case_count,
                                "positiveRecords": export.positive_records,
                                "repairRecords": export.repair_records,
                                "rejectedNegativeRecords": export.rejected_negative_records,
                                "trainRecords": export.train_records,
                                "validationRecords": export.validation_records
                            }),
                            &options,
                        )
                    })
                    .transpose()?;
                print_json(&json!({
                    "version": export.version,
                    "recordCount": export.record_count,
                    "caseCount": export.case_count,
                    "positiveRecords": export.positive_records,
                    "repairRecords": export.repair_records,
                    "rejectedNegativeRecords": export.rejected_negative_records,
                    "trainRecords": export.train_records,
                    "validationRecords": export.validation_records,
                    "output": output,
                    "manifest": manifest_path.map(|path| path.display().to_string())
                }))?;
            } else {
                print_json(&export)?;
            }
        }
        Commands::CheckControllerCurriculum {
            case,
            tag,
            family,
            profile,
            case_limit,
            validation_stride,
            no_validation_split,
            no_fail,
        } => {
            let export = export_controller_curriculum(ControllerCurriculumOptions {
                dataset_options: dataset_options(
                    case,
                    tag,
                    family,
                    profile,
                    case_limit,
                    validation_stride,
                    no_validation_split,
                ),
            })
            .map_err(anyhow::Error::msg)?;
            let report = assess_controller_curriculum_quality(&export);

            print_json(&report)?;

            if !report.passed && !no_fail {
                bail!("Controller curriculum quality check did not pass");
            }
        }
        Commands::VerifyControllerTrainingExport { manifest, no_fail } => {
            let report = verify_training_export_manifest(&manifest)?;
            print_json(&report)?;

            if !report.passed && !no_fail {
                bail!("Controller training export verification failed");
            }
        }
        Commands::CheckControllerRobustness { no_fail } => {
            let report = evaluate_controller_robustness();
            print_json(&report)?;

            if !report.passed && !no_fail {
                bail!("Controller robustness check did not pass");
            }
        }
        Commands::AuditControllerClaim {
            jsonl,
            manifest,
            no_fail,
        } => {
            let cases = jsonl
                .as_ref()
                .map(|path| read_eval_jsonl(path))
                .transpose()?;
            let manifest_value = manifest
                .as_ref()
                .map(|path| read_json_file(path))
                .transpose()?;
            let jsonl_path = jsonl.as_ref().map(|path| path.display().to_string());
            let report = audit_controller_claim(ControllerClaimAuditInput {
                cases: cases.as_deref(),
                manifest: manifest_value.as_ref(),
                jsonl_path: jsonl_path.as_deref(),
            });

            print_json(&report)?;

            if !report.passed && !no_fail {
                bail!("Controller claim audit did not pass");
            }
        }
        Commands::StatusControllerClaim {
            jsonl,
            manifest,
            output,
            require_claim_ready,
        } => {
            let cases = jsonl
                .as_ref()
                .map(|path| read_eval_jsonl(path))
                .transpose()?;
            let manifest_value = manifest
                .as_ref()
                .map(|path| read_json_file(path))
                .transpose()?;
            let jsonl_path = jsonl.as_ref().map(|path| path.display().to_string());
            let status = controller_claim_status(ControllerClaimStatusInput {
                cases: cases.as_deref(),
                manifest: manifest_value.as_ref(),
                jsonl_path: jsonl_path.as_deref(),
            });

            if let Some(output) = output {
                write_json_file(&output, &status)?;
                print_json(&json!({
                    "claimAllowed": status.claim_allowed,
                    "phase": status.phase,
                    "staticReadinessPassed": status.static_readiness_passed,
                    "liveEvidenceSupplied": status.live_evidence_supplied,
                    "failedChecks": status.failed_checks.iter().map(|check| check.id.clone()).collect::<Vec<_>>(),
                    "output": output
                }))?;
            } else {
                print_json(&status)?;
            }

            if require_claim_ready && !status.claim_allowed {
                bail!("Controller claim status is not claim-ready");
            }
        }
        Commands::ExportControllerEvidencePack {
            output,
            jsonl,
            manifest,
            model_id,
            request_preview_limit,
            require_claim_ready,
        } => {
            let summary = export_controller_evidence_pack(
                &output,
                jsonl.as_ref(),
                manifest.as_ref(),
                &model_id,
                request_preview_limit,
            )?;

            print_json(&summary)?;

            if require_claim_ready && summary["claimReady"].as_bool() != Some(true) {
                bail!("Controller evidence pack is not claim-ready");
            }
        }
        Commands::VerifyControllerEvidencePack { directory, no_fail } => {
            let report = verify_evidence_pack(&directory)?;
            print_json(&report)?;

            if !report.passed && !no_fail {
                bail!("Evidence pack verification failed");
            }
        }
        Commands::PreflightController {
            adapter,
            prompt_mode,
            grammar_payload,
            model,
            case,
            tag,
            family,
            profile,
            case_limit,
            jsonl,
            stream_jsonl,
            manifest,
            no_fail,
        } => {
            let adapter_mode = match adapter {
                EvalAdapter::Fixture => glyph::eval::controller::ControllerAdapterMode::Fixture,
                EvalAdapter::OpenaiCompatible => {
                    glyph::eval::controller::ControllerAdapterMode::OpenAiCompatible
                }
            };
            let report = preflight_controller_eval(ControllerPreflightOptions {
                adapter_mode,
                prompt_modes: resolve_prompt_modes(prompt_mode),
                grammar_payload: resolve_grammar_payload(grammar_payload),
                case_filter: ControllerEvalCaseFilter {
                    case_ids: case,
                    tags: tag,
                    families: family,
                    profiles: profile,
                    limit: case_limit,
                },
                models: resolve_preflight_model_mappings(adapter, &model)?,
                jsonl_path: jsonl.as_ref().map(|path| path.display().to_string()),
                manifest_path: manifest.as_ref().map(|path| path.display().to_string()),
                stream_jsonl,
            });
            print_json(&report)?;

            if !no_fail && !report.passed {
                bail!("Controller preflight did not pass");
            }
        }
        Commands::PlanControllerLiveRun {
            artifact_dir,
            endpoint,
            output,
        } => {
            let report = plan_controller_live_run(ControllerLivePlanOptions {
                artifact_dir,
                endpoint,
            });

            if let Some(output) = output {
                write_json_file(&output, &report)?;
                print_json(&json!({
                    "caseCount": report.case_count,
                    "familyCount": report.family_count,
                    "totalExpectedRows": report.total_expected_rows,
                    "totalExpectedModelCalls": report.total_expected_model_calls,
                    "output": output
                }))?;
            } else {
                print_json(&report)?;
            }
        }
        Commands::PlanControllerOfflineRun {
            artifact_dir,
            output,
        } => {
            let report = plan_controller_offline_run(ControllerOfflinePlanOptions { artifact_dir });

            if let Some(output) = output {
                write_json_file(&output, &report)?;
                print_json(&json!({
                    "version": report.version,
                    "caseCount": report.case_count,
                    "modelBuckets": report.model_buckets,
                    "totalExpectedRows": report.total_expected_rows,
                    "totalExpectedResponseFiles": report.total_expected_response_files,
                    "output": output
                }))?;
            } else {
                print_json(&report)?;
            }
        }
        Commands::VerifyControllerRun {
            jsonl,
            manifest,
            no_fail,
        } => {
            let cases = read_eval_jsonl(&jsonl)?;
            let manifest_value = read_json_file(&manifest)?;
            let report =
                verify_controller_run(&cases, &manifest_value, &jsonl.display().to_string());
            print_json(&report)?;

            if !no_fail && !report.passed {
                bail!("Controller run verification did not pass");
            }
        }
        Commands::VerifyControllerShards { plan, no_fail } => {
            let report = verify_controller_shards(&plan)?;
            print_json(&report)?;

            if !no_fail && !report.passed {
                bail!("Controller shard verification did not pass");
            }
        }
        Commands::GateController { jsonl, no_fail } => {
            let cases = read_eval_jsonl(&jsonl)?;
            let report = evaluate_controller_gate(&cases);
            print_json(&report)?;

            if !no_fail && !report.passed {
                bail!("Controller benchmark gate did not pass");
            }
        }
        Commands::ReportControllerBenchmark {
            jsonl,
            output,
            no_fail,
        } => {
            let cases = read_eval_jsonl(&jsonl)?;
            let report = controller_benchmark_report(&cases);

            if let Some(output) = output {
                write_json_file(&output, &report)?;
                print_json(&json!({
                    "passed": report.passed,
                    "gatePassed": report.gate_passed,
                    "caseRows": report.case_rows,
                    "liveCaseRows": report.live_case_rows,
                    "targetCaseRows": report.target_case_rows,
                    "comparisonStatuses": report.comparisons.iter().map(|comparison| {
                        json!({
                            "id": comparison.id,
                            "status": comparison.status,
                            "observed": comparison.observed,
                        })
                    }).collect::<Vec<_>>(),
                    "output": output
                }))?;
            } else {
                print_json(&report)?;
            }

            if !no_fail && !report.passed {
                bail!("Controller benchmark report did not pass");
            }
        }
        Commands::CoverageController { jsonl } => {
            let cases = read_eval_jsonl(&jsonl)?;
            print_json(&controller_eval_coverage(&cases))?;
        }
        Commands::MergeController {
            output,
            manifest,
            source_manifest,
            jsonl,
        } => {
            let case_sets = jsonl
                .iter()
                .map(|path| read_eval_jsonl(path))
                .collect::<Result<Vec<_>>>()?;
            let source_manifests = if manifest.is_some() {
                verified_source_manifests(&jsonl, &source_manifest, &case_sets)?
            } else {
                if !source_manifest.is_empty() {
                    bail!("--source-manifest requires --manifest");
                }
                vec![]
            };
            let started_at_unix_seconds = current_unix_seconds()?;
            let git_commit = current_git_commit();
            let git_tree_dirty = current_git_tree_dirty();
            let merged = merge_controller_eval_cases(case_sets);
            write_eval_jsonl(&output, &merged.cases)?;
            if let Some(manifest_path) = &manifest {
                let completed_manifest = build_merged_controller_eval_manifest(
                    ControllerEvalMergedManifestInput {
                        started_at_unix_seconds,
                        completed_at_unix_seconds: current_unix_seconds()?,
                        glyph_version: env!("CARGO_PKG_VERSION").to_string(),
                        git_commit,
                        git_tree_dirty,
                        jsonl_path: output.display().to_string(),
                        manifest_path: manifest_path.display().to_string(),
                        source_manifests,
                    },
                    &merged.cases,
                );
                write_json_file(manifest_path, &completed_manifest)?;
            }
            print_json(&json!({
                "output": output,
                "manifest": manifest,
                "merge": merged.report
            }))?;
        }
    }

    Ok(())
}

struct GlyphFile {
    source: String,
    path: PathBuf,
}

fn read_glyph_file(input: &str) -> Result<GlyphFile> {
    let candidates = [
        PathBuf::from(input),
        Path::new("src").join(input),
        Path::new("src/examples").join(
            Path::new(input)
                .file_name()
                .with_context(|| format!("Invalid file path {input}"))?,
        ),
    ];

    for candidate in candidates {
        if candidate.exists() {
            return Ok(GlyphFile {
                source: fs::read_to_string(&candidate)
                    .with_context(|| format!("Failed to read {}", candidate.display()))?,
                path: candidate,
            });
        }
    }

    bail!("Glyph file not found: {input}")
}

fn print_json(value: &impl serde::Serialize) -> Result<()> {
    write_stdout(&format!("{}\n", serde_json::to_string_pretty(value)?))
}

fn write_stdout(value: &str) -> Result<()> {
    match io::stdout().write_all(value.as_bytes()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn display_path(path: &Path) -> String {
    path.strip_prefix(std::env::current_dir().unwrap_or_default())
        .unwrap_or(path)
        .display()
        .to_string()
}

fn dataset_options(
    case: Vec<String>,
    tag: Vec<String>,
    family: Vec<String>,
    profile: Vec<String>,
    case_limit: Option<usize>,
    validation_stride: usize,
    no_validation_split: bool,
) -> ControllerDatasetOptions {
    ControllerDatasetOptions {
        case_filter: ControllerEvalCaseFilter {
            case_ids: case,
            tags: tag,
            families: family,
            profiles: profile,
            limit: case_limit,
        },
        validation_stride: if no_validation_split {
            None
        } else {
            Some(validation_stride)
        },
    }
}

fn resolve_model_mappings(mappings: &[String]) -> Result<Vec<(ControllerParameterClass, String)>> {
    let mut resolved = vec![
        (
            ControllerParameterClass::OneB,
            std::env::var("GLYPH_EVAL_MODEL_1B").ok(),
        ),
        (
            ControllerParameterClass::ThreeB,
            std::env::var("GLYPH_EVAL_MODEL_3B").ok(),
        ),
        (
            ControllerParameterClass::SevenB,
            std::env::var("GLYPH_EVAL_MODEL_7B").ok(),
        ),
        (
            ControllerParameterClass::Frontier,
            std::env::var("GLYPH_EVAL_MODEL_FRONTIER").ok(),
        ),
    ];

    for mapping in mappings {
        let Some((key, value)) = mapping.split_once('=') else {
            bail!("Invalid model mapping {mapping:?}. Expected key=value.");
        };
        let class = parse_parameter_class(key)?;
        let slot = resolved
            .iter_mut()
            .find(|(candidate, _)| *candidate == class)
            .expect("all parameter classes are present");
        slot.1 = Some(value.to_string());
    }

    resolved
        .into_iter()
        .map(|(class, maybe_model)| {
            maybe_model
                .map(|model| (class, model))
                .with_context(|| format!("Missing {} model. Pass --model {}=<model-id> or set the matching GLYPH_EVAL_MODEL_* env var.", class.as_str(), class.as_str()))
        })
        .collect()
}

fn resolve_preflight_model_mappings(
    adapter: EvalAdapter,
    mappings: &[String],
) -> Result<Vec<ControllerPreflightModel>> {
    if adapter == EvalAdapter::Fixture {
        return Ok(glyph::eval::controller::create_fixture_controller_models()
            .into_iter()
            .map(|model| ControllerPreflightModel {
                parameter_class: model.parameter_class,
                model_id: Some(model.id),
            })
            .collect());
    }

    let mut resolved = vec![
        (
            ControllerParameterClass::OneB,
            std::env::var("GLYPH_EVAL_MODEL_1B").ok(),
        ),
        (
            ControllerParameterClass::ThreeB,
            std::env::var("GLYPH_EVAL_MODEL_3B").ok(),
        ),
        (
            ControllerParameterClass::SevenB,
            std::env::var("GLYPH_EVAL_MODEL_7B").ok(),
        ),
        (
            ControllerParameterClass::Frontier,
            std::env::var("GLYPH_EVAL_MODEL_FRONTIER").ok(),
        ),
    ];

    for mapping in mappings {
        let Some((key, value)) = mapping.split_once('=') else {
            bail!("Invalid model mapping {mapping:?}. Expected key=value.");
        };
        let class = parse_parameter_class(key)?;
        let slot = resolved
            .iter_mut()
            .find(|(candidate, _)| *candidate == class)
            .expect("all parameter classes are present");
        slot.1 = Some(value.to_string());
    }

    Ok(resolved
        .into_iter()
        .map(|(parameter_class, model_id)| ControllerPreflightModel {
            parameter_class,
            model_id,
        })
        .collect())
}

fn parse_parameter_class(value: &str) -> Result<ControllerParameterClass> {
    match value {
        "1b" => Ok(ControllerParameterClass::OneB),
        "3b" => Ok(ControllerParameterClass::ThreeB),
        "7b" => Ok(ControllerParameterClass::SevenB),
        "frontier" => Ok(ControllerParameterClass::Frontier),
        _ => bail!("Invalid model bucket {value:?}. Expected 1b, 3b, 7b, or frontier."),
    }
}

fn resolve_prompt_modes(prompt_mode: EvalPromptMode) -> Vec<ControllerPromptMode> {
    match prompt_mode {
        EvalPromptMode::Constrained => vec![ControllerPromptMode::Constrained],
        EvalPromptMode::SchemaOnly => vec![ControllerPromptMode::SchemaOnly],
        EvalPromptMode::Plain => vec![ControllerPromptMode::Plain],
        EvalPromptMode::All => ControllerPromptMode::all(),
    }
}

fn resolve_grammar_payload(grammar_payload: EvalGrammarPayload) -> ControllerGrammarPayload {
    match grammar_payload {
        EvalGrammarPayload::None => ControllerGrammarPayload::None,
        EvalGrammarPayload::Gbnf => ControllerGrammarPayload::Gbnf,
    }
}

fn resolve_parameter_class(parameter_class: EvalParameterClass) -> ControllerParameterClass {
    match parameter_class {
        EvalParameterClass::OneB => ControllerParameterClass::OneB,
        EvalParameterClass::ThreeB => ControllerParameterClass::ThreeB,
        EvalParameterClass::SevenB => ControllerParameterClass::SevenB,
        EvalParameterClass::Frontier => ControllerParameterClass::Frontier,
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct PromptBundleManifest {
    version: String,
    #[serde(rename = "promptModes")]
    prompt_modes: Vec<String>,
    #[serde(rename = "grammarPayload")]
    grammar_payload: String,
    #[serde(rename = "caseCount")]
    case_count: usize,
    #[serde(rename = "promptFileCount")]
    prompt_file_count: usize,
    #[serde(rename = "artifactCount")]
    artifact_count: usize,
    #[serde(rename = "totalBytes")]
    total_bytes: u64,
    #[serde(rename = "overallSha256")]
    overall_sha256: String,
    #[serde(rename = "controllerFingerprintSha256")]
    controller_fingerprint_sha256: String,
    artifacts: Vec<PromptBundleArtifactDigest>,
    #[serde(rename = "excludedArtifacts")]
    excluded_artifacts: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct PromptBundleArtifactDigest {
    path: String,
    bytes: u64,
    sha256: String,
}

#[derive(Debug, Deserialize)]
struct PromptBundlePromptFile {
    id: String,
    #[serde(rename = "promptMode")]
    prompt_mode: String,
    #[serde(rename = "grammarPayload")]
    grammar_payload: String,
}

#[derive(Debug, Deserialize)]
struct PromptBundleQueuePromptFile {
    id: String,
    #[serde(rename = "promptMode")]
    prompt_mode: String,
    #[serde(rename = "grammarPayload")]
    grammar_payload: String,
    prompt: String,
    #[serde(rename = "jsonToolPlanPrompt")]
    json_tool_plan_prompt: String,
    #[serde(rename = "directProsePrompt")]
    direct_prose_prompt: String,
}

#[derive(Debug, Serialize)]
struct OfflineQueueRecord {
    version: &'static str,
    #[serde(rename = "caseId")]
    case_id: String,
    #[serde(rename = "promptMode")]
    prompt_mode: String,
    #[serde(rename = "grammarPayload")]
    grammar_payload: String,
    #[serde(rename = "requestKind")]
    request_kind: String,
    #[serde(rename = "promptFile")]
    prompt_file: String,
    #[serde(rename = "promptField")]
    prompt_field: String,
    prompt: String,
    #[serde(rename = "responsePath")]
    response_path: String,
}

#[derive(Debug, Serialize)]
struct OfflineQueueExportReport {
    version: &'static str,
    #[serde(rename = "promptBundlePath")]
    prompt_bundle_path: String,
    #[serde(rename = "responsesPath")]
    responses_path: String,
    #[serde(rename = "outputPath")]
    output_path: String,
    #[serde(rename = "manifestPath", skip_serializing_if = "Option::is_none")]
    manifest_path: Option<String>,
    #[serde(rename = "promptFileCount")]
    prompt_file_count: usize,
    #[serde(rename = "recordCount")]
    record_count: usize,
    #[serde(rename = "promptBundleOverallSha256")]
    prompt_bundle_overall_sha256: String,
    #[serde(rename = "promptBundleManifestSha256")]
    prompt_bundle_manifest_sha256: String,
    #[serde(rename = "outputBytes")]
    output_bytes: u64,
    #[serde(rename = "outputSha256")]
    output_sha256: String,
}

#[derive(Debug, Serialize)]
struct PromptBundleVerificationReport {
    version: &'static str,
    passed: bool,
    #[serde(rename = "manifestPath")]
    manifest_path: String,
    #[serde(rename = "promptModes")]
    prompt_modes: Vec<String>,
    #[serde(rename = "grammarPayload")]
    grammar_payload: String,
    #[serde(rename = "caseCount")]
    case_count: usize,
    #[serde(rename = "promptFileCount")]
    prompt_file_count: usize,
    #[serde(rename = "artifactCount")]
    artifact_count: usize,
    #[serde(rename = "checkedArtifacts")]
    checked_artifacts: usize,
    #[serde(rename = "missingArtifacts")]
    missing_artifacts: Vec<String>,
    #[serde(rename = "mismatchedArtifacts")]
    mismatched_artifacts: Vec<PromptBundleArtifactMismatch>,
    #[serde(rename = "expectedTotalBytes")]
    expected_total_bytes: u64,
    #[serde(rename = "actualTotalBytes")]
    actual_total_bytes: u64,
    #[serde(rename = "manifestOverallSha256")]
    manifest_overall_sha256: String,
    #[serde(rename = "computedManifestOverallSha256")]
    computed_manifest_overall_sha256: String,
    #[serde(rename = "actualOverallSha256")]
    actual_overall_sha256: String,
    #[serde(rename = "manifestFingerprintSha256")]
    manifest_fingerprint_sha256: String,
    #[serde(rename = "currentFingerprintSha256")]
    current_fingerprint_sha256: String,
    #[serde(rename = "excludedArtifacts")]
    excluded_artifacts: Vec<String>,
    errors: Vec<String>,
}

#[derive(Debug, Serialize)]
struct PromptBundleArtifactMismatch {
    path: String,
    #[serde(rename = "expectedBytes")]
    expected_bytes: u64,
    #[serde(rename = "actualBytes")]
    actual_bytes: Option<u64>,
    #[serde(rename = "expectedSha256")]
    expected_sha256: String,
    #[serde(rename = "actualSha256")]
    actual_sha256: Option<String>,
}

#[derive(Debug, Serialize)]
struct OfflineResponseCheckReport {
    version: &'static str,
    passed: bool,
    #[serde(rename = "promptBundlePath")]
    prompt_bundle_path: String,
    #[serde(rename = "responsesPath")]
    responses_path: String,
    #[serde(rename = "promptBundlePassed")]
    prompt_bundle_passed: bool,
    #[serde(rename = "promptModes")]
    prompt_modes: Vec<String>,
    #[serde(rename = "grammarPayload")]
    grammar_payload: String,
    #[serde(rename = "caseCount")]
    case_count: usize,
    #[serde(rename = "promptFileCount")]
    prompt_file_count: usize,
    #[serde(rename = "expectedResponseFileCount")]
    expected_response_file_count: usize,
    #[serde(rename = "presentResponseFileCount")]
    present_response_file_count: usize,
    #[serde(rename = "completeResponseSetCount")]
    complete_response_set_count: usize,
    #[serde(rename = "missingResponseFileCount")]
    missing_response_file_count: usize,
    #[serde(rename = "extraResponseFileCount")]
    extra_response_file_count: usize,
    #[serde(rename = "missingResponseFiles")]
    missing_response_files: Vec<String>,
    #[serde(rename = "extraResponseFiles")]
    extra_response_files: Vec<String>,
    #[serde(rename = "responseSets")]
    response_sets: Vec<OfflineResponseSetCheck>,
    errors: Vec<String>,
}

#[derive(Debug, Serialize)]
struct OfflineResponseSetCheck {
    #[serde(rename = "caseId")]
    case_id: String,
    #[serde(rename = "promptMode")]
    prompt_mode: String,
    #[serde(rename = "promptFile")]
    prompt_file: String,
    complete: bool,
    files: Vec<OfflineResponseFileCheck>,
}

#[derive(Debug, Serialize)]
struct OfflineResponseFileCheck {
    kind: String,
    path: String,
    present: bool,
    #[serde(rename = "validUtf8", skip_serializing_if = "Option::is_none")]
    valid_utf8: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn emit_prompt_bundle(
    output_dir: &Path,
    prompt_modes: &[ControllerPromptMode],
    grammar_payload: ControllerGrammarPayload,
    case_filter: &ControllerEvalCaseFilter,
) -> Result<()> {
    fs::create_dir_all(output_dir)
        .with_context(|| format!("Failed to create {}", output_dir.display()))?;
    let cases_dir = output_dir.join("cases");
    fs::create_dir_all(&cases_dir)
        .with_context(|| format!("Failed to create {}", cases_dir.display()))?;
    let selected_cases = select_controller_eval_cases(case_filter);
    let mut artifacts = Vec::new();

    write_prompt_bundle_artifact(output_dir, "glyph.gbnf", GLYPH_GBNF, &mut artifacts)?;
    write_prompt_bundle_artifact(
        output_dir,
        "controller-output.schema.json",
        GLYPH_CONTROLLER_OUTPUT_JSON_SCHEMA,
        &mut artifacts,
    )?;
    write_prompt_bundle_artifact(
        output_dir,
        "generic-tool-plan.schema.json",
        GENERIC_TOOL_PLAN_JSON_SCHEMA,
        &mut artifacts,
    )?;

    for prompt_mode in prompt_modes {
        let mode_dir = cases_dir.join(prompt_mode.as_str());
        fs::create_dir_all(&mode_dir)
            .with_context(|| format!("Failed to create {}", mode_dir.display()))?;

        for eval_case in &selected_cases {
            write_prompt_bundle_artifact(
                output_dir,
                &format!("cases/{}/{}.json", prompt_mode.as_str(), eval_case.id),
                &serde_json::to_string_pretty(&json!({
                    "id": &eval_case.id,
                    "request": &eval_case.request,
                    "tags": &eval_case.tags,
                    "promptMode": prompt_mode.as_str(),
                    "grammarPayload": grammar_payload.as_str(),
                    "grammar": {
                        "gbnf": "glyph.gbnf",
                        "jsonSchema": "controller-output.schema.json",
                        "genericToolPlanJsonSchema": "generic-tool-plan.schema.json"
                    },
                    "prompt": build_controller_prompt_with_payload(eval_case, *prompt_mode, grammar_payload),
                    "jsonToolPlanPrompt": build_json_tool_plan_prompt(eval_case, *prompt_mode),
                    "directProsePrompt": build_direct_prose_prompt(eval_case)
                }))?,
                &mut artifacts,
            )?;
        }
    }

    let total_bytes = artifacts.iter().map(|artifact| artifact.bytes).sum();
    let manifest = PromptBundleManifest {
        version: "glyph-controller-prompt-bundle/0.1".to_string(),
        prompt_modes: prompt_modes
            .iter()
            .map(|mode| mode.as_str().to_string())
            .collect(),
        grammar_payload: grammar_payload.as_str().to_string(),
        case_count: selected_cases.len(),
        prompt_file_count: selected_cases.len() * prompt_modes.len(),
        artifact_count: artifacts.len(),
        total_bytes,
        overall_sha256: prompt_bundle_overall_sha256(&artifacts),
        controller_fingerprint_sha256: controller_eval_fingerprint().overall_sha256,
        artifacts,
        excluded_artifacts: vec!["prompt-bundle-manifest.json".to_string()],
    };
    write_json_file(&output_dir.join("prompt-bundle-manifest.json"), &manifest)?;

    Ok(())
}

fn verify_prompt_bundle(output_dir: &Path) -> Result<PromptBundleVerificationReport> {
    let manifest_path = output_dir.join("prompt-bundle-manifest.json");
    let manifest_text = fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read {}", manifest_path.display()))?;
    let manifest: PromptBundleManifest = serde_json::from_str(&manifest_text)
        .with_context(|| format!("Failed to parse {}", manifest_path.display()))?;

    let mut errors = Vec::new();
    if manifest.version != "glyph-controller-prompt-bundle/0.1" {
        errors.push(format!(
            "unsupported manifest version `{}`",
            manifest.version
        ));
    }
    if manifest.artifact_count != manifest.artifacts.len() {
        errors.push(format!(
            "artifactCount {} does not match artifact list length {}",
            manifest.artifact_count,
            manifest.artifacts.len()
        ));
    }
    if manifest.prompt_file_count != manifest.case_count * manifest.prompt_modes.len() {
        errors.push(format!(
            "promptFileCount {} does not match caseCount {} * promptModes {}",
            manifest.prompt_file_count,
            manifest.case_count,
            manifest.prompt_modes.len()
        ));
    }
    if !manifest
        .excluded_artifacts
        .iter()
        .any(|artifact| artifact == "prompt-bundle-manifest.json")
    {
        errors.push("excludedArtifacts must include prompt-bundle-manifest.json".to_string());
    }
    if manifest
        .artifacts
        .iter()
        .any(|artifact| artifact.path == "prompt-bundle-manifest.json")
    {
        errors.push("prompt-bundle-manifest.json must not hash itself".to_string());
    }

    let expected_total_bytes: u64 = manifest
        .artifacts
        .iter()
        .map(|artifact| artifact.bytes)
        .sum();
    if manifest.total_bytes != expected_total_bytes {
        errors.push(format!(
            "totalBytes {} does not match artifact byte sum {}",
            manifest.total_bytes, expected_total_bytes
        ));
    }

    let computed_manifest_overall_sha256 = prompt_bundle_overall_sha256(&manifest.artifacts);
    if manifest.overall_sha256 != computed_manifest_overall_sha256 {
        errors.push("overallSha256 does not match manifest artifact entries".to_string());
    }

    let current_fingerprint_sha256 = controller_eval_fingerprint().overall_sha256;
    if manifest.controller_fingerprint_sha256 != current_fingerprint_sha256 {
        errors.push(
            "controllerFingerprintSha256 does not match current controller fingerprint".to_string(),
        );
    }

    let mut actual_artifacts = Vec::new();
    let mut missing_artifacts = Vec::new();
    let mut mismatched_artifacts = Vec::new();
    for expected in &manifest.artifacts {
        let artifact_path = output_dir.join(&expected.path);
        match fs::read(&artifact_path) {
            Ok(bytes) => {
                let actual = PromptBundleArtifactDigest {
                    path: expected.path.clone(),
                    bytes: bytes.len() as u64,
                    sha256: sha256_hex(&bytes),
                };
                if actual.bytes != expected.bytes || actual.sha256 != expected.sha256 {
                    mismatched_artifacts.push(PromptBundleArtifactMismatch {
                        path: expected.path.clone(),
                        expected_bytes: expected.bytes,
                        actual_bytes: Some(actual.bytes),
                        expected_sha256: expected.sha256.clone(),
                        actual_sha256: Some(actual.sha256.clone()),
                    });
                }
                actual_artifacts.push(actual);
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                missing_artifacts.push(expected.path.clone());
                mismatched_artifacts.push(PromptBundleArtifactMismatch {
                    path: expected.path.clone(),
                    expected_bytes: expected.bytes,
                    actual_bytes: None,
                    expected_sha256: expected.sha256.clone(),
                    actual_sha256: None,
                });
            }
            Err(error) => {
                bail!("Failed to read {}: {error}", artifact_path.display());
            }
        }
    }

    let actual_total_bytes = actual_artifacts.iter().map(|artifact| artifact.bytes).sum();
    let actual_overall_sha256 = prompt_bundle_overall_sha256(&actual_artifacts);
    let passed = errors.is_empty()
        && missing_artifacts.is_empty()
        && mismatched_artifacts.is_empty()
        && manifest.total_bytes == actual_total_bytes
        && manifest.overall_sha256 == actual_overall_sha256;

    Ok(PromptBundleVerificationReport {
        version: "glyph-controller-prompt-bundle-verification/0.1",
        passed,
        manifest_path: manifest_path.display().to_string(),
        prompt_modes: manifest.prompt_modes,
        grammar_payload: manifest.grammar_payload,
        case_count: manifest.case_count,
        prompt_file_count: manifest.prompt_file_count,
        artifact_count: manifest.artifact_count,
        checked_artifacts: actual_artifacts.len(),
        missing_artifacts,
        mismatched_artifacts,
        expected_total_bytes: manifest.total_bytes,
        actual_total_bytes,
        manifest_overall_sha256: manifest.overall_sha256,
        computed_manifest_overall_sha256,
        actual_overall_sha256,
        manifest_fingerprint_sha256: manifest.controller_fingerprint_sha256,
        current_fingerprint_sha256,
        excluded_artifacts: manifest.excluded_artifacts,
        errors,
    })
}

fn export_controller_offline_queue(
    prompt_bundle: &Path,
    responses: &Path,
    output: &Path,
    manifest: Option<&Path>,
) -> Result<OfflineQueueExportReport> {
    let bundle_verification = verify_prompt_bundle(prompt_bundle)?;
    if !bundle_verification.passed {
        bail!("Controller prompt bundle verification failed");
    }

    let bundle_manifest = read_prompt_bundle_manifest(prompt_bundle)?;
    let records = build_offline_queue_records(prompt_bundle, responses, &bundle_manifest)?;
    write_offline_queue_jsonl(output, &records)?;
    let output_bytes = fs::read(output)
        .with_context(|| format!("Failed to read exported queue {}", output.display()))?;
    let report = OfflineQueueExportReport {
        version: "glyph-controller-offline-queue-export/0.1",
        prompt_bundle_path: prompt_bundle.display().to_string(),
        responses_path: responses.display().to_string(),
        output_path: output.display().to_string(),
        manifest_path: manifest.map(|path| path.display().to_string()),
        prompt_file_count: bundle_manifest.prompt_file_count,
        record_count: records.len(),
        prompt_bundle_overall_sha256: bundle_manifest.overall_sha256,
        prompt_bundle_manifest_sha256: prompt_bundle_manifest_sha256(prompt_bundle)?,
        output_bytes: output_bytes.len() as u64,
        output_sha256: sha256_hex(&output_bytes),
    };

    if let Some(manifest) = manifest {
        write_json_file(manifest, &report)?;
    }

    Ok(report)
}

fn build_offline_queue_records(
    prompt_bundle: &Path,
    responses: &Path,
    bundle_manifest: &PromptBundleManifest,
) -> Result<Vec<OfflineQueueRecord>> {
    let prompt_artifacts = bundle_manifest
        .artifacts
        .iter()
        .filter(|artifact| is_prompt_file_artifact(&artifact.path))
        .collect::<Vec<_>>();

    if prompt_artifacts.len() != bundle_manifest.prompt_file_count {
        bail!(
            "Prompt bundle manifest expected {} prompt files but listed {} case artifacts",
            bundle_manifest.prompt_file_count,
            prompt_artifacts.len()
        );
    }

    let mut records = Vec::with_capacity(prompt_artifacts.len() * OFFLINE_RESPONSE_KINDS.len());
    for artifact in prompt_artifacts {
        let prompt_path = prompt_bundle.join(&artifact.path);
        let prompt_file: PromptBundleQueuePromptFile =
            serde_json::from_str(&fs::read_to_string(&prompt_path).with_context(|| {
                format!("Failed to read prompt file {}", prompt_path.display())
            })?)
            .with_context(|| format!("Failed to parse prompt file {}", prompt_path.display()))?;
        if prompt_file.grammar_payload != bundle_manifest.grammar_payload {
            bail!(
                "Prompt file {} grammarPayload `{}` does not match manifest `{}`",
                artifact.path,
                prompt_file.grammar_payload,
                bundle_manifest.grammar_payload
            );
        }

        let prompt_mode = parse_prompt_mode_name(&prompt_file.prompt_mode)?;
        let prompts = [
            (
                ControllerRequestKind::Glyph,
                "prompt",
                prompt_file.prompt.as_str(),
            ),
            (
                ControllerRequestKind::JsonToolPlan,
                "jsonToolPlanPrompt",
                prompt_file.json_tool_plan_prompt.as_str(),
            ),
            (
                ControllerRequestKind::DirectProse,
                "directProsePrompt",
                prompt_file.direct_prose_prompt.as_str(),
            ),
        ];

        for (request_kind, prompt_field, prompt) in prompts {
            records.push(OfflineQueueRecord {
                version: "glyph-controller-offline-queue-record/0.1",
                case_id: prompt_file.id.clone(),
                prompt_mode: prompt_mode.as_str().to_string(),
                grammar_payload: prompt_file.grammar_payload.clone(),
                request_kind: request_kind.as_str().to_string(),
                prompt_file: artifact.path.clone(),
                prompt_field: prompt_field.to_string(),
                prompt: prompt.to_string(),
                response_path: responses
                    .join(offline_response_relative_path(
                        prompt_mode,
                        &prompt_file.id,
                        request_kind.as_str(),
                    ))
                    .display()
                    .to_string(),
            });
        }
    }

    Ok(records)
}

const OFFLINE_RESPONSE_KINDS: [&str; 3] = ["glyph", "json-tool-plan", "direct-prose"];

fn check_controller_offline_responses(
    prompt_bundle: &Path,
    responses: &Path,
) -> Result<OfflineResponseCheckReport> {
    let bundle_verification = verify_prompt_bundle(prompt_bundle)?;
    let bundle_manifest = read_prompt_bundle_manifest(prompt_bundle)?;
    let mut errors = Vec::new();
    if !bundle_verification.passed {
        errors.push("prompt bundle verification failed".to_string());
    }

    let prompt_artifacts = bundle_manifest
        .artifacts
        .iter()
        .filter(|artifact| is_prompt_file_artifact(&artifact.path))
        .collect::<Vec<_>>();

    if prompt_artifacts.len() != bundle_manifest.prompt_file_count {
        errors.push(format!(
            "prompt bundle manifest expected {} prompt files but listed {} case artifacts",
            bundle_manifest.prompt_file_count,
            prompt_artifacts.len()
        ));
    }

    let mut expected_response_files = BTreeSet::new();
    let mut response_sets = Vec::new();
    for artifact in prompt_artifacts {
        let prompt_path = prompt_bundle.join(&artifact.path);
        let prompt_text = match fs::read_to_string(&prompt_path) {
            Ok(prompt_text) => prompt_text,
            Err(error) => {
                errors.push(format!(
                    "failed to read prompt file {}: {error}",
                    prompt_path.display()
                ));
                continue;
            }
        };
        let prompt_file: PromptBundlePromptFile = match serde_json::from_str(&prompt_text) {
            Ok(prompt_file) => prompt_file,
            Err(error) => {
                errors.push(format!(
                    "failed to parse prompt file {}: {error}",
                    prompt_path.display()
                ));
                continue;
            }
        };
        if prompt_file.grammar_payload != bundle_manifest.grammar_payload {
            errors.push(format!(
                "prompt file {} grammarPayload `{}` does not match manifest `{}`",
                artifact.path, prompt_file.grammar_payload, bundle_manifest.grammar_payload
            ));
        }

        let prompt_mode = match parse_prompt_mode_name(&prompt_file.prompt_mode) {
            Ok(prompt_mode) => prompt_mode,
            Err(error) => {
                errors.push(format!(
                    "prompt file {} has invalid promptMode `{}`: {error}",
                    artifact.path, prompt_file.prompt_mode
                ));
                continue;
            }
        };

        let mut files = Vec::new();
        for kind in OFFLINE_RESPONSE_KINDS {
            let relative_path = offline_response_relative_path(prompt_mode, &prompt_file.id, kind);
            expected_response_files.insert(relative_path.clone());
            files.push(check_offline_response_file(responses, kind, &relative_path));
        }
        let complete = files
            .iter()
            .all(|file| file.present && file.valid_utf8 == Some(true));
        response_sets.push(OfflineResponseSetCheck {
            case_id: prompt_file.id,
            prompt_mode: prompt_mode.as_str().to_string(),
            prompt_file: artifact.path.clone(),
            complete,
            files,
        });
    }

    let actual_response_files = collect_response_text_files(responses)?;
    let actual_response_file_set = actual_response_files.into_iter().collect::<BTreeSet<_>>();
    let missing_response_files = expected_response_files
        .difference(&actual_response_file_set)
        .cloned()
        .collect::<Vec<_>>();
    let extra_response_files = actual_response_file_set
        .difference(&expected_response_files)
        .cloned()
        .collect::<Vec<_>>();
    let present_response_file_count = response_sets
        .iter()
        .flat_map(|set| set.files.iter())
        .filter(|file| file.present)
        .count();
    let complete_response_set_count = response_sets.iter().filter(|set| set.complete).count();
    let expected_response_file_count =
        bundle_manifest.prompt_file_count * OFFLINE_RESPONSE_KINDS.len();
    let invalid_utf8_count = response_sets
        .iter()
        .flat_map(|set| set.files.iter())
        .filter(|file| file.valid_utf8 == Some(false))
        .count();
    if invalid_utf8_count > 0 {
        errors.push(format!(
            "{invalid_utf8_count} offline response files are not valid UTF-8"
        ));
    }

    let passed = errors.is_empty()
        && bundle_verification.passed
        && missing_response_files.is_empty()
        && extra_response_files.is_empty()
        && complete_response_set_count == bundle_manifest.prompt_file_count
        && expected_response_files.len() == expected_response_file_count;

    Ok(OfflineResponseCheckReport {
        version: "glyph-controller-offline-response-check/0.1",
        passed,
        prompt_bundle_path: prompt_bundle.display().to_string(),
        responses_path: responses.display().to_string(),
        prompt_bundle_passed: bundle_verification.passed,
        prompt_modes: bundle_manifest.prompt_modes,
        grammar_payload: bundle_manifest.grammar_payload,
        case_count: bundle_manifest.case_count,
        prompt_file_count: bundle_manifest.prompt_file_count,
        expected_response_file_count,
        present_response_file_count,
        complete_response_set_count,
        missing_response_file_count: missing_response_files.len(),
        extra_response_file_count: extra_response_files.len(),
        missing_response_files,
        extra_response_files,
        response_sets,
        errors,
    })
}

fn check_offline_response_file(
    responses: &Path,
    kind: &str,
    relative_path: &str,
) -> OfflineResponseFileCheck {
    let path = responses.join(relative_path);
    match fs::read(&path) {
        Ok(bytes) => {
            let valid_utf8 = String::from_utf8(bytes.clone()).is_ok();
            OfflineResponseFileCheck {
                kind: kind.to_string(),
                path: relative_path.to_string(),
                present: true,
                valid_utf8: Some(valid_utf8),
                bytes: Some(bytes.len() as u64),
                sha256: Some(sha256_hex(&bytes)),
                error: (!valid_utf8).then(|| "not valid UTF-8".to_string()),
            }
        }
        Err(error) if error.kind() == ErrorKind::NotFound => OfflineResponseFileCheck {
            kind: kind.to_string(),
            path: relative_path.to_string(),
            present: false,
            valid_utf8: None,
            bytes: None,
            sha256: None,
            error: Some("missing".to_string()),
        },
        Err(error) => OfflineResponseFileCheck {
            kind: kind.to_string(),
            path: relative_path.to_string(),
            present: false,
            valid_utf8: None,
            bytes: None,
            sha256: None,
            error: Some(error.to_string()),
        },
    }
}

fn score_controller_response_bundle(
    prompt_bundle: &Path,
    responses: &Path,
    model_id: &str,
    parameter_class: ControllerParameterClass,
    jsonl: Option<&Path>,
    manifest: Option<&Path>,
) -> Result<ControllerEvalReport> {
    let response_check = check_controller_offline_responses(prompt_bundle, responses)?;
    if !response_check.passed {
        bail!(
            "Controller offline response check failed: missing={}, extra={}, errors={}",
            response_check.missing_response_file_count,
            response_check.extra_response_file_count,
            response_check.errors.len()
        );
    }

    let bundle_verification = verify_prompt_bundle(prompt_bundle)?;
    if !bundle_verification.passed {
        bail!("Controller prompt bundle verification failed");
    }

    let bundle_manifest = read_prompt_bundle_manifest(prompt_bundle)?;
    let grammar_payload = parse_grammar_payload_name(&bundle_manifest.grammar_payload)?;
    let mut offline_responses = BTreeMap::new();
    let mut response_artifacts = Vec::new();
    let mut case_ids = BTreeSet::new();
    let mut prompt_modes = BTreeSet::new();
    let prompt_artifacts = bundle_manifest
        .artifacts
        .iter()
        .filter(|artifact| is_prompt_file_artifact(&artifact.path))
        .collect::<Vec<_>>();

    if prompt_artifacts.len() != bundle_manifest.prompt_file_count {
        bail!(
            "Prompt bundle manifest expected {} prompt files but listed {} case artifacts",
            bundle_manifest.prompt_file_count,
            prompt_artifacts.len()
        );
    }

    for artifact in prompt_artifacts {
        let prompt_path = prompt_bundle.join(&artifact.path);
        let prompt_file: PromptBundlePromptFile =
            serde_json::from_str(&fs::read_to_string(&prompt_path).with_context(|| {
                format!("Failed to read prompt file {}", prompt_path.display())
            })?)
            .with_context(|| format!("Failed to parse prompt file {}", prompt_path.display()))?;
        if prompt_file.grammar_payload != bundle_manifest.grammar_payload {
            bail!(
                "Prompt file {} grammarPayload `{}` does not match manifest `{}`",
                artifact.path,
                prompt_file.grammar_payload,
                bundle_manifest.grammar_payload
            );
        }

        let prompt_mode = parse_prompt_mode_name(&prompt_file.prompt_mode)?;
        let response_set = read_offline_response_set(
            responses,
            prompt_mode,
            &prompt_file.id,
            &mut response_artifacts,
        )?;
        offline_responses.insert(
            ControllerOfflineResponseKey {
                case_id: prompt_file.id.clone(),
                prompt_mode,
            },
            response_set,
        );
        case_ids.insert(prompt_file.id);
        prompt_modes.insert(prompt_mode);
    }

    if offline_responses.len() != bundle_manifest.prompt_file_count {
        bail!(
            "Loaded {} offline response sets but prompt bundle expected {}",
            offline_responses.len(),
            bundle_manifest.prompt_file_count
        );
    }
    let response_bundle_file_count = response_artifacts.len();
    let response_bundle_bytes = response_artifacts
        .iter()
        .map(|artifact| artifact.bytes)
        .sum::<u64>();
    let response_bundle_sha256 = prompt_bundle_overall_sha256(&response_artifacts);

    let selected_case_ids = case_ids.into_iter().collect::<Vec<_>>();
    let prompt_modes = prompt_modes.into_iter().collect::<Vec<_>>();
    let case_filter = ControllerEvalCaseFilter {
        case_ids: selected_case_ids.clone(),
        tags: vec![],
        families: vec![],
        profiles: vec![],
        limit: None,
    };
    let selected_cases = select_controller_eval_cases(&case_filter);
    if selected_cases.len() != selected_case_ids.len() {
        bail!(
            "Prompt bundle references {} case IDs, but {} are present in the current eval corpus",
            selected_case_ids.len(),
            selected_cases.len()
        );
    }

    let model = create_offline_response_controller_model(
        model_id.to_string(),
        parameter_class,
        grammar_payload,
        offline_responses,
    );
    let started_at_unix_seconds = current_unix_seconds()?;
    let git_commit = current_git_commit();
    let git_tree_dirty = current_git_tree_dirty();
    let manifest_config = ControllerEvalRunConfig {
        adapter_mode: glyph::eval::controller::ControllerAdapterMode::OfflineResponses,
        endpoint: None,
        api_key_env: None,
        api_key_provided: false,
        models: vec![ControllerEvalRunModel {
            parameter_class,
            model_id: model_id.to_string(),
        }],
        prompt_modes: prompt_modes.clone(),
        grammar_payload,
        case_filter: ControllerEvalRunCaseFilter::from(&case_filter),
        selected_case_count: selected_case_ids.len(),
        selected_case_ids,
        artifacts: ControllerEvalRunArtifacts {
            jsonl_path: jsonl.map(|path| path.display().to_string()),
            manifest_path: manifest.map(|path| path.display().to_string()),
            emit_prompts_path: Some(prompt_bundle.display().to_string()),
            prompt_bundle_overall_sha256: Some(bundle_manifest.overall_sha256.clone()),
            prompt_bundle_manifest_sha256: Some(prompt_bundle_manifest_sha256(prompt_bundle)?),
            response_bundle_path: Some(responses.display().to_string()),
            response_bundle_file_count: Some(response_bundle_file_count),
            response_bundle_bytes: Some(response_bundle_bytes),
            response_bundle_sha256: Some(response_bundle_sha256),
            stream_jsonl: false,
        },
    };

    if let Some(path) = manifest {
        let planned_manifest = build_controller_eval_run_manifest(
            started_at_unix_seconds,
            None,
            env!("CARGO_PKG_VERSION"),
            git_commit.clone(),
            git_tree_dirty,
            manifest_config.clone(),
            None,
        );
        write_json_file(path, &planned_manifest)?;
    }

    let report = run_controller_eval_with_options(ControllerEvalOptions {
        models: Some(vec![model]),
        prompt_modes,
        case_filter,
    });

    if let Some(path) = jsonl {
        write_eval_jsonl(path, &report.cases)?;
    }

    if let Some(path) = manifest {
        let completed_manifest = build_controller_eval_run_manifest(
            started_at_unix_seconds,
            Some(current_unix_seconds()?),
            env!("CARGO_PKG_VERSION"),
            git_commit,
            git_tree_dirty,
            manifest_config,
            Some(&report),
        );
        write_json_file(path, &completed_manifest)?;
    }

    Ok(report)
}

fn read_prompt_bundle_manifest(output_dir: &Path) -> Result<PromptBundleManifest> {
    let manifest_path = output_dir.join("prompt-bundle-manifest.json");
    let manifest_text = fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read {}", manifest_path.display()))?;
    serde_json::from_str(&manifest_text)
        .with_context(|| format!("Failed to parse {}", manifest_path.display()))
}

fn prompt_bundle_manifest_sha256(output_dir: &Path) -> Result<String> {
    let manifest_path = output_dir.join("prompt-bundle-manifest.json");
    let bytes = fs::read(&manifest_path)
        .with_context(|| format!("Failed to read {}", manifest_path.display()))?;
    Ok(sha256_hex(&bytes))
}

fn is_prompt_file_artifact(path: &str) -> bool {
    path.starts_with("cases/") && path.ends_with(".json") && path.matches('/').count() == 2
}

fn read_offline_response_set(
    responses: &Path,
    prompt_mode: ControllerPromptMode,
    case_id: &str,
    artifacts: &mut Vec<PromptBundleArtifactDigest>,
) -> Result<ControllerOfflineResponseSet> {
    Ok(ControllerOfflineResponseSet {
        glyph: read_offline_response_text(responses, prompt_mode, case_id, "glyph", artifacts)?,
        json_tool_plan: read_offline_response_text(
            responses,
            prompt_mode,
            case_id,
            "json-tool-plan",
            artifacts,
        )?,
        direct_prose: read_offline_response_text(
            responses,
            prompt_mode,
            case_id,
            "direct-prose",
            artifacts,
        )?,
    })
}

fn read_offline_response_text(
    responses: &Path,
    prompt_mode: ControllerPromptMode,
    case_id: &str,
    kind: &str,
    artifacts: &mut Vec<PromptBundleArtifactDigest>,
) -> Result<String> {
    let relative_path = offline_response_relative_path(prompt_mode, case_id, kind);
    let path = responses.join(&relative_path);
    let bytes = fs::read(&path)
        .with_context(|| format!("Missing offline response file {}", path.display()))?;
    artifacts.push(PromptBundleArtifactDigest {
        path: relative_path,
        bytes: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
    });
    String::from_utf8(bytes).with_context(|| {
        format!(
            "Offline response file {} is not valid UTF-8",
            path.display()
        )
    })
}

fn offline_response_relative_path(
    prompt_mode: ControllerPromptMode,
    case_id: &str,
    kind: &str,
) -> String {
    format!("cases/{}/{case_id}.{kind}.txt", prompt_mode.as_str())
}

fn collect_response_text_files(root: &Path) -> Result<Vec<String>> {
    if !root.exists() {
        return Ok(vec![]);
    }

    let mut files = Vec::new();
    collect_response_text_files_recursive(root, root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_response_text_files_recursive(
    root: &Path,
    directory: &Path,
    files: &mut Vec<String>,
) -> Result<()> {
    let mut entries = fs::read_dir(directory)
        .with_context(|| format!("Failed to read {}", directory.display()))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("Failed to list {}", directory.display()))?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("Failed to inspect {}", path.display()))?;
        if file_type.is_dir() {
            collect_response_text_files_recursive(root, &path, files)?;
        } else if file_type.is_file()
            && path.extension().is_some_and(|extension| extension == "txt")
        {
            let relative = path
                .strip_prefix(root)
                .with_context(|| format!("Failed to relativize {}", path.display()))?;
            files.push(path_to_slash(relative));
        }
    }

    Ok(())
}

fn path_to_slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn parse_prompt_mode_name(value: &str) -> Result<ControllerPromptMode> {
    match value {
        "constrained" => Ok(ControllerPromptMode::Constrained),
        "schema-only" => Ok(ControllerPromptMode::SchemaOnly),
        "plain" => Ok(ControllerPromptMode::Plain),
        _ => bail!("Invalid prompt mode {value:?}"),
    }
}

fn parse_grammar_payload_name(value: &str) -> Result<ControllerGrammarPayload> {
    match value {
        "none" => Ok(ControllerGrammarPayload::None),
        "gbnf" => Ok(ControllerGrammarPayload::Gbnf),
        _ => bail!("Invalid grammar payload {value:?}"),
    }
}

fn write_prompt_bundle_artifact(
    output_dir: &Path,
    relative_path: &str,
    contents: &str,
    artifacts: &mut Vec<PromptBundleArtifactDigest>,
) -> Result<()> {
    let path = output_dir.join(relative_path);
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    fs::write(&path, contents).with_context(|| format!("Failed to write {}", path.display()))?;
    artifacts.push(PromptBundleArtifactDigest {
        path: relative_path.to_string(),
        bytes: contents.len() as u64,
        sha256: sha256_hex(contents.as_bytes()),
    });
    Ok(())
}

fn prompt_bundle_overall_sha256(artifacts: &[PromptBundleArtifactDigest]) -> String {
    let mut overall = Sha256::new();
    for artifact in artifacts {
        overall.update(artifact.path.as_bytes());
        overall.update([0u8]);
        overall.update(artifact.bytes.to_string().as_bytes());
        overall.update([0u8]);
        overall.update(artifact.sha256.as_bytes());
        overall.update([0xff]);
    }

    let digest = overall.finalize();
    hex_digest(&digest)
}

fn preview_controller_requests(
    model_id: &str,
    prompt_modes: &[ControllerPromptMode],
    grammar_payload: ControllerGrammarPayload,
    case_filter: &ControllerEvalCaseFilter,
) -> serde_json::Value {
    let cases = select_controller_eval_cases(case_filter);
    let request_kinds = [
        ControllerRequestKind::Glyph,
        ControllerRequestKind::JsonToolPlan,
        ControllerRequestKind::DirectProse,
    ];
    let requests = prompt_modes
        .iter()
        .flat_map(|prompt_mode| {
            cases.iter().map(move |eval_case| {
                let bodies = request_kinds
                    .iter()
                    .map(|request_kind| {
                        (
                            request_kind.as_str().to_string(),
                            build_openai_compatible_request_body(
                                model_id,
                                eval_case,
                                *prompt_mode,
                                grammar_payload,
                                *request_kind,
                            ),
                        )
                    })
                    .collect::<serde_json::Map<_, _>>();

                json!({
                    "caseId": eval_case.id,
                    "tags": eval_case.tags,
                    "promptMode": prompt_mode.as_str(),
                    "grammarPayload": grammar_payload.as_str(),
                    "requests": bodies
                })
            })
        })
        .collect::<Vec<_>>();

    json!({
        "modelId": model_id,
        "promptModes": prompt_modes.iter().map(|mode| mode.as_str()).collect::<Vec<_>>(),
        "grammarPayload": grammar_payload.as_str(),
        "caseCount": cases.len(),
        "requestKinds": request_kinds.iter().map(|kind| kind.as_str()).collect::<Vec<_>>(),
        "requestCount": requests.len() * request_kinds.len(),
        "requests": requests
    })
}

#[derive(Debug, Serialize)]
struct ControllerFingerprintLockReport {
    version: &'static str,
    passed: bool,
    #[serde(rename = "lockPath")]
    lock_path: String,
    #[serde(rename = "currentOverallSha256")]
    current_overall_sha256: String,
    #[serde(rename = "lockedOverallSha256")]
    locked_overall_sha256: Option<String>,
    mismatches: Vec<ControllerFingerprintLockMismatch>,
}

#[derive(Debug, Serialize)]
struct ControllerFingerprintLockMismatch {
    section: String,
    locked: serde_json::Value,
    current: serde_json::Value,
}

fn check_controller_fingerprint_lock(lock_path: &Path) -> Result<ControllerFingerprintLockReport> {
    let current = controller_eval_fingerprint();
    let current_json = serde_json::to_value(&current)?;
    let locked_json = read_json_file(lock_path)?;
    let mut mismatches = Vec::new();

    for section in [
        "algorithm",
        "overallSha256",
        "specArtifacts",
        "evalCorpus",
        "requestContract",
    ] {
        let locked = locked_json
            .get(section)
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let current = current_json
            .get(section)
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        if locked != current {
            mismatches.push(ControllerFingerprintLockMismatch {
                section: section.to_string(),
                locked,
                current,
            });
        }
    }

    let locked_overall_sha256 = locked_json
        .get("overallSha256")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string);

    Ok(ControllerFingerprintLockReport {
        version: "glyph-controller-fingerprint-lock-check/0.1",
        passed: mismatches.is_empty(),
        lock_path: lock_path.display().to_string(),
        current_overall_sha256: current.overall_sha256,
        locked_overall_sha256,
        mismatches,
    })
}

fn export_controller_evidence_pack(
    output_dir: &Path,
    jsonl: Option<&PathBuf>,
    manifest: Option<&PathBuf>,
    model_id: &str,
    request_preview_limit: usize,
) -> Result<serde_json::Value> {
    match (jsonl, manifest) {
        (Some(_), None) | (None, Some(_)) => {
            bail!("--jsonl and --manifest must be supplied together for an evidence pack")
        }
        _ => {}
    }

    fs::create_dir_all(output_dir)
        .with_context(|| format!("Failed to create {}", output_dir.display()))?;

    let fingerprint = controller_eval_fingerprint();
    let fingerprint_path = output_dir.join("fingerprint.json");
    write_json_file(&fingerprint_path, &fingerprint)?;

    let fingerprint_lock =
        check_controller_fingerprint_lock(Path::new("spec/controller-fingerprint.lock.json"))?;
    let fingerprint_lock_path = output_dir.join("fingerprint-lock.json");
    write_json_file(&fingerprint_lock_path, &fingerprint_lock)?;

    let dataset_export = export_controller_dataset(ControllerDatasetOptions::default())
        .map_err(anyhow::Error::msg)?;
    let dataset_quality = assess_controller_dataset_quality(&dataset_export);
    let dataset_quality_path = output_dir.join("dataset-quality.json");
    write_json_file(&dataset_quality_path, &dataset_quality)?;

    let curriculum_export = export_controller_curriculum(ControllerCurriculumOptions::default())
        .map_err(anyhow::Error::msg)?;
    let curriculum_quality = assess_controller_curriculum_quality(&curriculum_export);
    let curriculum_quality_path = output_dir.join("curriculum-quality.json");
    write_json_file(&curriculum_quality_path, &curriculum_quality)?;

    let robustness = evaluate_controller_robustness();
    let robustness_path = output_dir.join("robustness.json");
    write_json_file(&robustness_path, &robustness)?;

    let conformance = glyph_conformance_report();
    let conformance_path = output_dir.join("conformance.json");
    write_json_file(&conformance_path, &conformance)?;

    let live_plan = plan_controller_live_run(ControllerLivePlanOptions::default());
    let live_plan_path = output_dir.join("live-plan.json");
    write_json_file(&live_plan_path, &live_plan)?;

    let offline_plan = plan_controller_offline_run(ControllerOfflinePlanOptions::default());
    let offline_plan_path = output_dir.join("offline-plan.json");
    write_json_file(&offline_plan_path, &offline_plan)?;

    let preview = preview_controller_requests(
        model_id,
        &[ControllerPromptMode::Constrained],
        ControllerGrammarPayload::Gbnf,
        &ControllerEvalCaseFilter {
            limit: Some(request_preview_limit),
            ..ControllerEvalCaseFilter::default()
        },
    );
    let preview_path = output_dir.join("request-preview.json");
    write_json_file(&preview_path, &preview)?;

    let cases = jsonl.map(|path| read_eval_jsonl(path)).transpose()?;
    let manifest_value = manifest.map(|path| read_json_file(path)).transpose()?;
    let jsonl_path = jsonl.map(|path| path.display().to_string());
    let audit = audit_controller_claim(ControllerClaimAuditInput {
        cases: cases.as_deref(),
        manifest: manifest_value.as_ref(),
        jsonl_path: jsonl_path.as_deref(),
    });
    let status = controller_claim_status_from_audit(audit.clone());
    let status_path = output_dir.join("status.json");
    write_json_file(&status_path, &status)?;
    let audit_path = output_dir.join("claim-audit.json");
    write_json_file(&audit_path, &audit)?;

    let mut files = vec![
        "fingerprint.json".to_string(),
        "fingerprint-lock.json".to_string(),
        "dataset-quality.json".to_string(),
        "curriculum-quality.json".to_string(),
        "robustness.json".to_string(),
        "conformance.json".to_string(),
        "live-plan.json".to_string(),
        "offline-plan.json".to_string(),
        "request-preview.json".to_string(),
        "status.json".to_string(),
        "claim-audit.json".to_string(),
    ];

    if let Some(cases) = cases.as_deref() {
        let coverage = controller_eval_coverage(cases);
        write_json_file(&output_dir.join("coverage.json"), &coverage)?;
        files.push("coverage.json".to_string());

        let gate = evaluate_controller_gate(cases);
        write_json_file(&output_dir.join("gate.json"), &gate)?;
        files.push("gate.json".to_string());

        let benchmark_report = controller_benchmark_report(cases);
        write_json_file(&output_dir.join("benchmark-report.json"), &benchmark_report)?;
        files.push("benchmark-report.json".to_string());

        if let (Some(manifest), Some(jsonl_path)) = (manifest_value.as_ref(), jsonl_path.as_deref())
        {
            let verification = verify_controller_run(cases, manifest, jsonl_path);
            write_json_file(&output_dir.join("verification.json"), &verification)?;
            files.push("verification.json".to_string());
        }
    }

    let mut pack_files = files.clone();
    pack_files.push("summary.json".to_string());
    pack_files.push("README.md".to_string());
    pack_files.push("evidence-manifest.json".to_string());

    let summary = json!({
        "output": output_dir.display().to_string(),
        "claimReady": audit.claim_ready,
        "claimAllowed": status.claim_allowed,
        "phase": status.phase,
        "auditPassed": audit.passed,
        "liveEvidenceSupplied": cases.is_some(),
        "fingerprintSha256": fingerprint.overall_sha256,
        "fingerprintLockPassed": fingerprint_lock.passed,
        "datasetQualityPassed": dataset_quality.passed,
        "curriculumQualityPassed": curriculum_quality.passed,
        "robustnessPassed": robustness.passed,
        "conformancePassed": conformance.passed,
        "requestPreviewCount": preview["requestCount"],
        "files": pack_files,
    });
    write_json_file(&output_dir.join("summary.json"), &summary)?;
    write_text_file(
        &output_dir.join("README.md"),
        &evidence_pack_readme(&summary, jsonl, manifest),
    )?;
    let sealed_files = pack_files
        .iter()
        .filter(|file| file.as_str() != "evidence-manifest.json")
        .cloned()
        .collect::<Vec<_>>();
    write_evidence_pack_manifest(output_dir, &sealed_files)?;

    Ok(summary)
}

fn evidence_pack_readme(
    summary: &serde_json::Value,
    jsonl: Option<&PathBuf>,
    manifest: Option<&PathBuf>,
) -> String {
    [
        "# Glyph Controller Evidence Pack".to_string(),
        String::new(),
        format!(
            "- Claim ready: `{}`",
            summary["claimReady"].as_bool().unwrap_or(false)
        ),
        format!(
            "- Live evidence supplied: `{}`",
            summary["liveEvidenceSupplied"].as_bool().unwrap_or(false)
        ),
        format!(
            "- Benchmark fingerprint: `{}`",
            summary["fingerprintSha256"].as_str().unwrap_or("missing")
        ),
        format!(
            "- Fingerprint lock passed: `{}`",
            summary["fingerprintLockPassed"]
                .as_bool()
                .unwrap_or(false)
        ),
        format!(
            "- Source JSONL: `{}`",
            jsonl
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "not supplied".to_string())
        ),
        format!(
            "- Source manifest: `{}`",
            manifest
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "not supplied".to_string())
        ),
        String::new(),
        "Review order:".to_string(),
        String::new(),
        "1. `evidence-manifest.json`".to_string(),
        "2. `fingerprint.json`".to_string(),
        "3. `fingerprint-lock.json`".to_string(),
        "4. `dataset-quality.json`".to_string(),
        "5. `curriculum-quality.json`".to_string(),
        "6. `robustness.json`".to_string(),
        "7. `conformance.json`".to_string(),
        "8. `live-plan.json`".to_string(),
        "9. `offline-plan.json`".to_string(),
        "10. `request-preview.json`".to_string(),
        "11. `status.json`".to_string(),
        "12. `verification.json` if live evidence was supplied".to_string(),
        "13. `benchmark-report.json` if live evidence was supplied".to_string(),
        "14. `coverage.json` and `gate.json` if live evidence was supplied".to_string(),
        "15. `claim-audit.json`".to_string(),
        String::new(),
        "`evidence-manifest.json` hashes every generated artifact except itself, so the pack can be archived and rechecked without circular hashing.".to_string(),
        String::new(),
        "A best-in-lane claim is allowed only when `claim-audit.json` has `passed: true`."
            .to_string(),
        String::new(),
    ]
    .join("\n")
}

#[derive(Debug, Deserialize, Serialize)]
struct EvidencePackManifest {
    version: String,
    algorithm: String,
    #[serde(rename = "artifactCount")]
    artifact_count: usize,
    #[serde(rename = "totalBytes")]
    total_bytes: u64,
    #[serde(rename = "overallSha256")]
    overall_sha256: String,
    artifacts: Vec<EvidencePackArtifactDigest>,
    #[serde(rename = "excludedArtifacts")]
    excluded_artifacts: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct EvidencePackArtifactDigest {
    path: String,
    bytes: u64,
    sha256: String,
}

#[derive(Debug, Serialize)]
struct EvidencePackVerificationReport {
    version: &'static str,
    passed: bool,
    #[serde(rename = "manifestPath")]
    manifest_path: String,
    #[serde(rename = "artifactCount")]
    artifact_count: usize,
    #[serde(rename = "checkedArtifacts")]
    checked_artifacts: usize,
    #[serde(rename = "missingArtifacts")]
    missing_artifacts: Vec<String>,
    #[serde(rename = "mismatchedArtifacts")]
    mismatched_artifacts: Vec<EvidencePackArtifactMismatch>,
    #[serde(rename = "expectedTotalBytes")]
    expected_total_bytes: u64,
    #[serde(rename = "actualTotalBytes")]
    actual_total_bytes: u64,
    #[serde(rename = "manifestOverallSha256")]
    manifest_overall_sha256: String,
    #[serde(rename = "computedManifestOverallSha256")]
    computed_manifest_overall_sha256: String,
    #[serde(rename = "actualOverallSha256")]
    actual_overall_sha256: String,
    #[serde(rename = "excludedArtifacts")]
    excluded_artifacts: Vec<String>,
    errors: Vec<String>,
}

#[derive(Debug, Serialize)]
struct EvidencePackArtifactMismatch {
    path: String,
    #[serde(rename = "expectedBytes")]
    expected_bytes: u64,
    #[serde(rename = "actualBytes")]
    actual_bytes: Option<u64>,
    #[serde(rename = "expectedSha256")]
    expected_sha256: String,
    #[serde(rename = "actualSha256")]
    actual_sha256: Option<String>,
}

fn write_evidence_pack_manifest(
    output_dir: &Path,
    artifact_files: &[String],
) -> Result<EvidencePackManifest> {
    let mut artifacts = Vec::with_capacity(artifact_files.len());
    for artifact_file in artifact_files {
        let path = output_dir.join(artifact_file);
        let bytes = fs::read(&path)
            .with_context(|| format!("Failed to read evidence artifact {}", path.display()))?;
        artifacts.push(EvidencePackArtifactDigest {
            path: artifact_file.clone(),
            bytes: bytes.len() as u64,
            sha256: sha256_hex(&bytes),
        });
    }

    let total_bytes = artifacts.iter().map(|artifact| artifact.bytes).sum();
    let overall_sha256 = evidence_pack_overall_sha256(&artifacts);

    let manifest = EvidencePackManifest {
        version: "glyph-evidence-pack-manifest/0.1".to_string(),
        algorithm: "sha256".to_string(),
        artifact_count: artifacts.len(),
        total_bytes,
        overall_sha256,
        artifacts,
        excluded_artifacts: vec!["evidence-manifest.json".to_string()],
    };
    write_json_file(&output_dir.join("evidence-manifest.json"), &manifest)?;
    Ok(manifest)
}

fn write_training_export_manifest(
    manifest_path: &Path,
    artifact_kind: &str,
    artifact_path: &Path,
    data_version: &str,
    counts: serde_json::Value,
    options: &ControllerDatasetOptions,
) -> Result<PathBuf> {
    let bytes = fs::read(artifact_path).with_context(|| {
        format!(
            "Failed to read training artifact {}",
            artifact_path.display()
        )
    })?;
    let manifest = json!({
        "version": "glyph-controller-training-export-manifest/0.1",
        "kind": artifact_kind,
        "dataVersion": data_version,
        "artifact": {
            "path": artifact_path.display().to_string(),
            "bytes": bytes.len(),
            "sha256": sha256_hex(&bytes)
        },
        "controllerFingerprintSha256": controller_eval_fingerprint().overall_sha256,
        "gitCommit": current_git_commit(),
        "gitTreeDirty": current_git_tree_dirty(),
        "counts": counts,
        "options": {
            "caseFilter": {
                "caseIds": &options.case_filter.case_ids,
                "tags": &options.case_filter.tags,
                "families": &options.case_filter.families,
                "profiles": &options.case_filter.profiles,
                "limit": options.case_filter.limit
            },
            "validationStride": options.validation_stride
        }
    });

    write_json_file(manifest_path, &manifest)?;
    Ok(manifest_path.to_path_buf())
}

#[derive(Debug, Deserialize)]
struct ControllerTrainingExportManifest {
    version: String,
    kind: String,
    #[serde(rename = "dataVersion")]
    data_version: String,
    artifact: ControllerTrainingExportManifestArtifact,
    #[serde(rename = "controllerFingerprintSha256")]
    controller_fingerprint_sha256: String,
    counts: serde_json::Value,
    options: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct ControllerTrainingExportManifestArtifact {
    path: String,
    bytes: u64,
    sha256: String,
}

#[derive(Debug, Serialize)]
struct ControllerTrainingExportVerificationReport {
    version: &'static str,
    passed: bool,
    #[serde(rename = "manifestPath")]
    manifest_path: String,
    kind: String,
    #[serde(rename = "dataVersion")]
    data_version: String,
    #[serde(
        rename = "expectedDataVersion",
        skip_serializing_if = "Option::is_none"
    )]
    expected_data_version: Option<String>,
    #[serde(rename = "artifactPath")]
    artifact_path: String,
    #[serde(
        rename = "resolvedArtifactPath",
        skip_serializing_if = "Option::is_none"
    )]
    resolved_artifact_path: Option<String>,
    #[serde(rename = "expectedBytes")]
    expected_bytes: u64,
    #[serde(rename = "actualBytes", skip_serializing_if = "Option::is_none")]
    actual_bytes: Option<u64>,
    #[serde(rename = "expectedSha256")]
    expected_sha256: String,
    #[serde(rename = "actualSha256", skip_serializing_if = "Option::is_none")]
    actual_sha256: Option<String>,
    #[serde(rename = "manifestFingerprintSha256")]
    manifest_fingerprint_sha256: String,
    #[serde(rename = "currentFingerprintSha256")]
    current_fingerprint_sha256: String,
    #[serde(rename = "recordCount")]
    record_count: Option<u64>,
    #[serde(rename = "optionsPresent")]
    options_present: bool,
    errors: Vec<String>,
}

fn verify_training_export_manifest(
    manifest_path: &Path,
) -> Result<ControllerTrainingExportVerificationReport> {
    let manifest_text = fs::read_to_string(manifest_path)
        .with_context(|| format!("Failed to read {}", manifest_path.display()))?;
    let manifest: ControllerTrainingExportManifest = serde_json::from_str(&manifest_text)
        .with_context(|| format!("Failed to parse {}", manifest_path.display()))?;

    let mut errors = Vec::new();
    if manifest.version != "glyph-controller-training-export-manifest/0.1" {
        errors.push(format!(
            "unsupported manifest version `{}`",
            manifest.version
        ));
    }

    let expected_data_version = match manifest.kind.as_str() {
        "dataset" => Some(CONTROLLER_DATASET_VERSION.to_string()),
        "curriculum" => Some(CONTROLLER_CURRICULUM_VERSION.to_string()),
        other => {
            errors.push(format!("unsupported training export kind `{other}`"));
            None
        }
    };
    if let Some(expected_data_version) = expected_data_version.as_deref()
        && manifest.data_version != expected_data_version
    {
        errors.push(format!(
            "dataVersion `{}` does not match expected `{}`",
            manifest.data_version, expected_data_version
        ));
    }

    let current_fingerprint_sha256 = controller_eval_fingerprint().overall_sha256;
    if manifest.controller_fingerprint_sha256 != current_fingerprint_sha256 {
        errors.push(
            "controllerFingerprintSha256 does not match current controller fingerprint".to_string(),
        );
    }

    let record_count = manifest
        .counts
        .get("recordCount")
        .and_then(serde_json::Value::as_u64);
    if !matches!(record_count, Some(count) if count > 0) {
        errors.push("counts.recordCount must be a positive integer".to_string());
    }
    let options_present = manifest.options.is_object();
    if !options_present {
        errors.push("options must be an object".to_string());
    }

    let resolved_artifact_path =
        resolve_training_artifact_path(manifest_path, &manifest.artifact.path);
    let (actual_bytes, actual_sha256) = match resolved_artifact_path
        .as_ref()
        .map(|path| fs::read(path).map(|bytes| (path, bytes)))
    {
        Some(Ok((_path, bytes))) => {
            let actual_bytes = bytes.len() as u64;
            let actual_sha256 = sha256_hex(&bytes);
            if actual_bytes != manifest.artifact.bytes {
                errors.push(format!(
                    "artifact bytes {} do not match expected {}",
                    actual_bytes, manifest.artifact.bytes
                ));
            }
            if actual_sha256 != manifest.artifact.sha256 {
                errors.push("artifact sha256 does not match manifest".to_string());
            }
            (Some(actual_bytes), Some(actual_sha256))
        }
        Some(Err(error)) => {
            errors.push(format!(
                "failed to read artifact `{}`: {error}",
                manifest.artifact.path
            ));
            (None, None)
        }
        None => {
            errors.push(format!(
                "artifact `{}` was not found",
                manifest.artifact.path
            ));
            (None, None)
        }
    };

    Ok(ControllerTrainingExportVerificationReport {
        version: "glyph-controller-training-export-verification/0.1",
        passed: errors.is_empty(),
        manifest_path: manifest_path.display().to_string(),
        kind: manifest.kind,
        data_version: manifest.data_version,
        expected_data_version,
        artifact_path: manifest.artifact.path,
        resolved_artifact_path: resolved_artifact_path.map(|path| path.display().to_string()),
        expected_bytes: manifest.artifact.bytes,
        actual_bytes,
        expected_sha256: manifest.artifact.sha256,
        actual_sha256,
        manifest_fingerprint_sha256: manifest.controller_fingerprint_sha256,
        current_fingerprint_sha256,
        record_count,
        options_present,
        errors,
    })
}

fn resolve_training_artifact_path(manifest_path: &Path, artifact_path: &str) -> Option<PathBuf> {
    let direct = PathBuf::from(artifact_path);
    if direct.is_file() {
        return Some(direct);
    }

    if direct.is_absolute() {
        return None;
    }

    let relative_to_manifest = manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(&direct);
    if relative_to_manifest.is_file() {
        Some(relative_to_manifest)
    } else {
        None
    }
}

fn verify_evidence_pack(output_dir: &Path) -> Result<EvidencePackVerificationReport> {
    let manifest_path = output_dir.join("evidence-manifest.json");
    let manifest_text = fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read {}", manifest_path.display()))?;
    let manifest: EvidencePackManifest = serde_json::from_str(&manifest_text)
        .with_context(|| format!("Failed to parse {}", manifest_path.display()))?;

    let mut errors = Vec::new();
    if manifest.version != "glyph-evidence-pack-manifest/0.1" {
        errors.push(format!(
            "unsupported manifest version `{}`",
            manifest.version
        ));
    }
    if manifest.algorithm != "sha256" {
        errors.push(format!(
            "unsupported manifest algorithm `{}`",
            manifest.algorithm
        ));
    }
    if manifest.artifact_count != manifest.artifacts.len() {
        errors.push(format!(
            "artifactCount {} does not match artifact list length {}",
            manifest.artifact_count,
            manifest.artifacts.len()
        ));
    }
    if !manifest
        .excluded_artifacts
        .iter()
        .any(|artifact| artifact == "evidence-manifest.json")
    {
        errors.push("excludedArtifacts must include evidence-manifest.json".to_string());
    }
    if manifest
        .artifacts
        .iter()
        .any(|artifact| artifact.path == "evidence-manifest.json")
    {
        errors.push("evidence-manifest.json must not hash itself".to_string());
    }

    let expected_total_bytes: u64 = manifest
        .artifacts
        .iter()
        .map(|artifact| artifact.bytes)
        .sum();
    if manifest.total_bytes != expected_total_bytes {
        errors.push(format!(
            "totalBytes {} does not match artifact byte sum {}",
            manifest.total_bytes, expected_total_bytes
        ));
    }

    let computed_manifest_overall_sha256 = evidence_pack_overall_sha256(&manifest.artifacts);
    if manifest.overall_sha256 != computed_manifest_overall_sha256 {
        errors.push("overallSha256 does not match manifest artifact entries".to_string());
    }

    let mut actual_artifacts = Vec::new();
    let mut missing_artifacts = Vec::new();
    let mut mismatched_artifacts = Vec::new();
    for expected in &manifest.artifacts {
        let artifact_path = output_dir.join(&expected.path);
        match fs::read(&artifact_path) {
            Ok(bytes) => {
                let actual = EvidencePackArtifactDigest {
                    path: expected.path.clone(),
                    bytes: bytes.len() as u64,
                    sha256: sha256_hex(&bytes),
                };
                if actual.bytes != expected.bytes || actual.sha256 != expected.sha256 {
                    mismatched_artifacts.push(EvidencePackArtifactMismatch {
                        path: expected.path.clone(),
                        expected_bytes: expected.bytes,
                        actual_bytes: Some(actual.bytes),
                        expected_sha256: expected.sha256.clone(),
                        actual_sha256: Some(actual.sha256.clone()),
                    });
                }
                actual_artifacts.push(actual);
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                missing_artifacts.push(expected.path.clone());
                mismatched_artifacts.push(EvidencePackArtifactMismatch {
                    path: expected.path.clone(),
                    expected_bytes: expected.bytes,
                    actual_bytes: None,
                    expected_sha256: expected.sha256.clone(),
                    actual_sha256: None,
                });
            }
            Err(error) => {
                bail!("Failed to read {}: {error}", artifact_path.display());
            }
        }
    }

    let actual_total_bytes: u64 = actual_artifacts.iter().map(|artifact| artifact.bytes).sum();
    let actual_overall_sha256 = evidence_pack_overall_sha256(&actual_artifacts);
    let passed = errors.is_empty()
        && missing_artifacts.is_empty()
        && mismatched_artifacts.is_empty()
        && manifest.total_bytes == actual_total_bytes
        && manifest.overall_sha256 == actual_overall_sha256;

    Ok(EvidencePackVerificationReport {
        version: "glyph-evidence-pack-verification/0.1",
        passed,
        manifest_path: manifest_path.display().to_string(),
        artifact_count: manifest.artifact_count,
        checked_artifacts: actual_artifacts.len(),
        missing_artifacts,
        mismatched_artifacts,
        expected_total_bytes: manifest.total_bytes,
        actual_total_bytes,
        manifest_overall_sha256: manifest.overall_sha256,
        computed_manifest_overall_sha256,
        actual_overall_sha256,
        excluded_artifacts: manifest.excluded_artifacts,
        errors,
    })
}

fn evidence_pack_overall_sha256(artifacts: &[EvidencePackArtifactDigest]) -> String {
    let mut overall = Sha256::new();
    for artifact in artifacts {
        overall.update(artifact.path.as_bytes());
        overall.update([0u8]);
        overall.update(artifact.bytes.to_string().as_bytes());
        overall.update([0u8]);
        overall.update(artifact.sha256.as_bytes());
        overall.update([0xff]);
    }

    let digest = overall.finalize();
    hex_digest(&digest)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex_digest(&digest)
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn write_eval_jsonl(path: &Path, cases: &[ControllerEvalCaseResult]) -> Result<()> {
    let mut file = create_eval_jsonl_writer(path)?;
    for case in cases {
        write_eval_jsonl_case(&mut file, case)?;
    }
    file.flush()?;
    Ok(())
}

fn write_offline_queue_jsonl(path: &Path, records: &[OfflineQueueRecord]) -> Result<()> {
    let mut file = create_eval_jsonl_writer(path)?;
    for record in records {
        writeln!(file, "{}", serde_json::to_string(record)?)?;
    }
    file.flush()?;
    Ok(())
}

fn write_dataset_jsonl(path: &Path, records: &[ControllerDatasetRecord]) -> Result<()> {
    let mut file = create_eval_jsonl_writer(path)?;
    for record in records {
        writeln!(file, "{}", serde_json::to_string(record)?)?;
    }
    file.flush()?;
    Ok(())
}

fn write_curriculum_jsonl(path: &Path, records: &[ControllerCurriculumRecord]) -> Result<()> {
    let mut file = create_eval_jsonl_writer(path)?;
    for record in records {
        writeln!(file, "{}", serde_json::to_string(record)?)?;
    }
    file.flush()?;
    Ok(())
}

fn create_eval_jsonl_writer(path: &Path) -> Result<io::BufWriter<fs::File>> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    fs::File::create(path)
        .map(io::BufWriter::new)
        .with_context(|| format!("Failed to create {}", path.display()))
}

fn write_eval_jsonl_case(writer: &mut impl Write, case: &ControllerEvalCaseResult) -> Result<()> {
    writeln!(writer, "{}", serde_json::to_string(case)?)?;
    Ok(())
}

fn write_json_file(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))
        .with_context(|| format!("Failed to write {}", path.display()))
}

fn write_text_file(path: &Path, value: &str) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    fs::write(path, value).with_context(|| format!("Failed to write {}", path.display()))
}

fn read_json_file(path: &Path) -> Result<serde_json::Value> {
    serde_json::from_str(
        &fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?,
    )
    .with_context(|| format!("Failed to parse JSON from {}", path.display()))
}

#[derive(Debug, Serialize)]
struct ControllerShardVerificationReport {
    version: &'static str,
    passed: bool,
    #[serde(rename = "planPath")]
    plan_path: String,
    #[serde(rename = "planVersion")]
    plan_version: Option<String>,
    #[serde(rename = "shardCount")]
    shard_count: usize,
    #[serde(rename = "verifiedShards")]
    verified_shards: usize,
    #[serde(rename = "expectedRows")]
    expected_rows: usize,
    #[serde(rename = "actualRows")]
    actual_rows: usize,
    shards: Vec<ControllerShardVerification>,
    errors: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ControllerShardVerification {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bucket: Option<String>,
    #[serde(rename = "jsonlPath", skip_serializing_if = "Option::is_none")]
    jsonl_path: Option<String>,
    #[serde(rename = "manifestPath", skip_serializing_if = "Option::is_none")]
    manifest_path: Option<String>,
    #[serde(rename = "expectedRows", skip_serializing_if = "Option::is_none")]
    expected_rows: Option<usize>,
    #[serde(rename = "actualRows", skip_serializing_if = "Option::is_none")]
    actual_rows: Option<usize>,
    passed: bool,
    errors: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    verification: Option<ControllerRunVerificationReport>,
}

fn verify_controller_shards(plan_path: &Path) -> Result<ControllerShardVerificationReport> {
    let plan = read_json_file(plan_path)?;
    let plan_version = plan
        .get("version")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string);
    let mut errors = Vec::new();
    if !is_supported_controller_shard_plan_version(plan_version.as_deref()) {
        errors.push(format!(
            "plan version `{}` must match `{}` or `{}`",
            plan_version.as_deref().unwrap_or("missing"),
            CONTROLLER_LIVE_PLAN_VERSION,
            CONTROLLER_OFFLINE_PLAN_VERSION
        ));
    }

    let shard_values = plan
        .get("shards")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_else(|| {
            errors.push("plan must include a shards array".to_string());
            vec![]
        });

    let shards = shard_values
        .iter()
        .enumerate()
        .map(|(index, shard)| verify_controller_shard(plan_path, index, shard))
        .collect::<Vec<_>>();

    let expected_rows = shards
        .iter()
        .map(|shard| shard.expected_rows.unwrap_or(0))
        .sum::<usize>();
    let actual_rows = shards
        .iter()
        .map(|shard| shard.actual_rows.unwrap_or(0))
        .sum::<usize>();
    let verified_shards = shards.iter().filter(|shard| shard.passed).count();

    if let Some(total_expected_rows) = plan
        .get("totalExpectedRows")
        .and_then(serde_json::Value::as_u64)
        .map(|value| value as usize)
        && total_expected_rows != expected_rows
    {
        errors.push(format!(
            "totalExpectedRows {total_expected_rows} does not match shard expected row sum {expected_rows}"
        ));
    }

    let passed = errors.is_empty() && !shards.is_empty() && shards.iter().all(|shard| shard.passed);

    Ok(ControllerShardVerificationReport {
        version: "glyph-controller-shard-verification/0.1",
        passed,
        plan_path: plan_path.display().to_string(),
        plan_version,
        shard_count: shards.len(),
        verified_shards,
        expected_rows,
        actual_rows,
        shards,
        errors,
    })
}

fn verify_controller_shard(
    plan_path: &Path,
    index: usize,
    shard: &serde_json::Value,
) -> ControllerShardVerification {
    let id = shard
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
        .unwrap_or_else(|| format!("shard_{index}"));
    let family = shard
        .get("family")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string);
    let bucket = shard
        .get("bucket")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string);
    let jsonl_path = shard
        .get("jsonlPath")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string);
    let manifest_path = shard
        .get("manifestPath")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string);
    let expected_rows = shard
        .get("expectedRows")
        .and_then(serde_json::Value::as_u64)
        .map(|value| value as usize);
    let mut errors = Vec::new();

    let Some(jsonl_path_string) = jsonl_path.as_deref() else {
        errors.push("missing jsonlPath".to_string());
        return ControllerShardVerification {
            id,
            family,
            bucket,
            jsonl_path,
            manifest_path,
            expected_rows,
            actual_rows: None,
            passed: false,
            errors,
            verification: None,
        };
    };
    let Some(manifest_path_string) = manifest_path.as_deref() else {
        errors.push("missing manifestPath".to_string());
        return ControllerShardVerification {
            id,
            family,
            bucket,
            jsonl_path,
            manifest_path,
            expected_rows,
            actual_rows: None,
            passed: false,
            errors,
            verification: None,
        };
    };
    let Some(expected_rows_value) = expected_rows else {
        errors.push("missing expectedRows".to_string());
        return ControllerShardVerification {
            id,
            family,
            bucket,
            jsonl_path,
            manifest_path,
            expected_rows,
            actual_rows: None,
            passed: false,
            errors,
            verification: None,
        };
    };

    let resolved_jsonl = resolve_plan_artifact_path(plan_path, jsonl_path_string);
    let resolved_manifest = resolve_plan_artifact_path(plan_path, manifest_path_string);
    let cases = match read_eval_jsonl(&resolved_jsonl) {
        Ok(cases) => cases,
        Err(error) => {
            errors.push(format!(
                "failed to read jsonl `{jsonl_path_string}`: {error}"
            ));
            return ControllerShardVerification {
                id,
                family,
                bucket,
                jsonl_path,
                manifest_path,
                expected_rows,
                actual_rows: None,
                passed: false,
                errors,
                verification: None,
            };
        }
    };
    let actual_rows = Some(cases.len());
    if cases.len() != expected_rows_value {
        errors.push(format!(
            "actual rows {} do not match expected rows {}",
            cases.len(),
            expected_rows_value
        ));
    }

    let manifest = match read_json_file(&resolved_manifest) {
        Ok(manifest) => manifest,
        Err(error) => {
            errors.push(format!(
                "failed to read manifest `{manifest_path_string}`: {error}"
            ));
            return ControllerShardVerification {
                id,
                family,
                bucket,
                jsonl_path,
                manifest_path,
                expected_rows,
                actual_rows,
                passed: false,
                errors,
                verification: None,
            };
        }
    };
    let verification = verify_controller_run(&cases, &manifest, jsonl_path_string);
    if !verification.passed {
        errors.push("run manifest verification failed".to_string());
    }
    let passed = errors.is_empty() && verification.passed;

    ControllerShardVerification {
        id,
        family,
        bucket,
        jsonl_path,
        manifest_path,
        expected_rows,
        actual_rows,
        passed,
        errors,
        verification: Some(verification),
    }
}

fn is_supported_controller_shard_plan_version(version: Option<&str>) -> bool {
    matches!(
        version,
        Some(CONTROLLER_LIVE_PLAN_VERSION) | Some(CONTROLLER_OFFLINE_PLAN_VERSION)
    )
}

fn resolve_plan_artifact_path(plan_path: &Path, artifact_path: &str) -> PathBuf {
    let direct = PathBuf::from(artifact_path);
    if direct.is_absolute() || direct.exists() {
        return direct;
    }

    plan_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(direct)
}

fn read_eval_jsonl(path: &Path) -> Result<Vec<ControllerEvalCaseResult>> {
    let file =
        fs::File::open(path).with_context(|| format!("Failed to open {}", path.display()))?;
    let reader = io::BufReader::new(file);
    let mut cases = Vec::new();

    for (index, line) in reader.lines().enumerate() {
        let line = line.with_context(|| {
            format!("Failed to read line {} from {}", index + 1, path.display())
        })?;
        if line.trim().is_empty() {
            continue;
        }
        cases.push(serde_json::from_str(&line).with_context(|| {
            format!(
                "Failed to parse controller eval JSONL line {} from {}",
                index + 1,
                path.display()
            )
        })?);
    }

    Ok(cases)
}

fn verified_source_manifests(
    jsonl_paths: &[PathBuf],
    manifest_paths: &[PathBuf],
    case_sets: &[Vec<ControllerEvalCaseResult>],
) -> Result<Vec<ControllerEvalSourceManifest>> {
    if manifest_paths.len() != jsonl_paths.len() {
        bail!(
            "--manifest requires one --source-manifest per input JSONL file; got {} source manifests for {} JSONL files",
            manifest_paths.len(),
            jsonl_paths.len()
        );
    }

    jsonl_paths
        .iter()
        .zip(manifest_paths)
        .zip(case_sets)
        .map(|((jsonl_path, manifest_path), cases)| {
            let manifest = read_json_file(manifest_path)?;
            let jsonl_path_string = jsonl_path.display().to_string();
            let verification = verify_controller_run(cases, &manifest, &jsonl_path_string);
            if !verification.passed {
                bail!(
                    "Source manifest {} did not verify against {}",
                    manifest_path.display(),
                    jsonl_path.display()
                );
            }

            Ok(ControllerEvalSourceManifest {
                manifest_path: manifest_path.display().to_string(),
                jsonl_path: jsonl_path_string,
                fingerprint_sha256: manifest
                    .get("fingerprint")
                    .and_then(|value| value.get("overallSha256"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("missing")
                    .to_string(),
                case_rows: cases.len(),
                verified: true,
            })
        })
        .collect()
}

fn current_unix_seconds() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("System clock is before the Unix epoch")?
        .as_secs())
}

fn current_git_commit() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn current_git_tree_dirty() -> Option<bool> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()?;
    output.status.success().then_some(!output.stdout.is_empty())
}
