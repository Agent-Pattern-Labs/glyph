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
    ControllerGrammarPayload, ControllerParameterClass, ControllerPromptMode,
    ControllerRequestKind, GENERIC_TOOL_PLAN_JSON_SCHEMA, build_controller_prompt_with_payload,
    build_direct_prose_prompt, build_json_tool_plan_prompt, build_openai_compatible_request_body,
    create_openai_compatible_controller_models, run_controller_eval_with_observer,
    run_controller_eval_with_options, select_controller_eval_cases,
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
use glyph::eval::live_plan::{ControllerLivePlanOptions, plan_controller_live_run};
use glyph::eval::manifest::{
    ControllerEvalMergedManifestInput, ControllerEvalRunArtifacts, ControllerEvalRunCaseFilter,
    ControllerEvalRunConfig, ControllerEvalRunModel, ControllerEvalSourceManifest,
    build_controller_eval_run_manifest, build_merged_controller_eval_manifest,
};
use glyph::eval::preflight::{
    ControllerPreflightModel, ControllerPreflightOptions, preflight_controller_eval,
};
use glyph::eval::results::merge_controller_eval_cases;
use glyph::eval::robustness::evaluate_controller_robustness;
use glyph::eval::status::{
    ControllerClaimStatusInput, controller_claim_status, controller_claim_status_from_audit,
};
use glyph::eval::verify::verify_controller_run;
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
    /// Print stable hashes for controller eval specs and corpus.
    FingerprintController,
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
    /// Verify a controller JSONL trace matches its manifest and current benchmark fingerprint.
    VerifyControllerRun {
        jsonl: PathBuf,
        manifest: PathBuf,
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
            if let Some(output_dir) = emit_prompts {
                emit_prompt_bundle(&output_dir, &prompt_modes, grammar_payload, &case_filter)?;
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
        Commands::FingerprintController => {
            print_json(&controller_eval_fingerprint())?;
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
        "dataset-quality.json".to_string(),
        "curriculum-quality.json".to_string(),
        "robustness.json".to_string(),
        "conformance.json".to_string(),
        "live-plan.json".to_string(),
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
        "3. `dataset-quality.json`".to_string(),
        "4. `curriculum-quality.json`".to_string(),
        "5. `robustness.json`".to_string(),
        "6. `conformance.json`".to_string(),
        "7. `live-plan.json`".to_string(),
        "8. `request-preview.json`".to_string(),
        "9. `status.json`".to_string(),
        "10. `verification.json` if live evidence was supplied".to_string(),
        "11. `benchmark-report.json` if live evidence was supplied".to_string(),
        "12. `coverage.json` and `gate.json` if live evidence was supplied".to_string(),
        "13. `claim-audit.json`".to_string(),
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
