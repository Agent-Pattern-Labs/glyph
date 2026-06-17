use std::fs;
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use glyph::eval::compression::compare_compression;
use glyph::eval::controller::{
    ControllerEvalOptions, ControllerParameterClass, build_controller_prompt,
    create_openai_compatible_controller_models, run_controller_eval,
    run_controller_eval_with_options,
};
use glyph::eval::controller_examples::controller_eval_cases;
use glyph::eval::examples::find_compression_example;
use glyph::harness::mock_tools::create_mock_tool_registry;
use glyph::ir::glyph_ir::parse_glyph_to_ir;
use glyph::ir::validate_ir::validate_ir;
use glyph::language::formatter::format_glyph;
use glyph::language::grammar::{
    GLYPH_CONTROLLER_OUTPUT_JSON_SCHEMA, GLYPH_GBNF, get_grammar_artifact,
};
use glyph::language::parser::parse_glyph;
use glyph::runtime::glyph_vm::GlyphVm;
use serde_json::json;

#[derive(Debug, Parser)]
#[command(name = "glyph", version, about = "GlyphVM CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
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
    /// Run the controller eval harness with fixture or OpenAI-compatible adapters.
    EvalController {
        #[arg(long, value_enum, default_value_t = EvalAdapter::Fixture)]
        adapter: EvalAdapter,
        #[arg(long, default_value = "http://localhost:11434/v1")]
        endpoint: String,
        #[arg(long, default_value = "GLYPH_EVAL_API_KEY")]
        api_key_env: String,
        #[arg(short, long)]
        model: Vec<String>,
        #[arg(long)]
        emit_prompts: Option<PathBuf>,
        #[arg(long)]
        jsonl: Option<PathBuf>,
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
        Commands::EvalController {
            adapter,
            endpoint,
            api_key_env,
            model,
            emit_prompts,
            jsonl,
        } => {
            if let Some(output_dir) = emit_prompts {
                emit_prompt_bundle(&output_dir)?;
            }

            let report = match adapter {
                EvalAdapter::Fixture => run_controller_eval(),
                EvalAdapter::OpenaiCompatible => {
                    let models = create_openai_compatible_controller_models(
                        endpoint,
                        std::env::var(api_key_env).ok(),
                        resolve_model_mappings(&model)?,
                    );
                    run_controller_eval_with_options(ControllerEvalOptions {
                        models: Some(models),
                    })
                }
            };

            if let Some(path) = jsonl {
                write_eval_jsonl(&path, &report.cases)?;
            }

            print_json(&report)?;
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

fn parse_parameter_class(value: &str) -> Result<ControllerParameterClass> {
    match value {
        "1b" => Ok(ControllerParameterClass::OneB),
        "3b" => Ok(ControllerParameterClass::ThreeB),
        "7b" => Ok(ControllerParameterClass::SevenB),
        "frontier" => Ok(ControllerParameterClass::Frontier),
        _ => bail!("Invalid model bucket {value:?}. Expected 1b, 3b, 7b, or frontier."),
    }
}

fn emit_prompt_bundle(output_dir: &Path) -> Result<()> {
    fs::create_dir_all(output_dir)
        .with_context(|| format!("Failed to create {}", output_dir.display()))?;
    fs::create_dir_all(output_dir.join("cases"))
        .with_context(|| format!("Failed to create {}", output_dir.join("cases").display()))?;

    fs::write(output_dir.join("glyph.gbnf"), GLYPH_GBNF)?;
    fs::write(
        output_dir.join("controller-output.schema.json"),
        GLYPH_CONTROLLER_OUTPUT_JSON_SCHEMA,
    )?;

    for eval_case in controller_eval_cases() {
        let path = output_dir
            .join("cases")
            .join(format!("{}.json", eval_case.id));
        fs::write(
            path,
            serde_json::to_string_pretty(&json!({
                "id": eval_case.id,
                "request": eval_case.request,
                "tags": eval_case.tags,
                "grammar": {
                    "gbnf": "glyph.gbnf",
                    "jsonSchema": "controller-output.schema.json"
                },
                "prompt": build_controller_prompt(&eval_case)
            }))?,
        )?;
    }

    Ok(())
}

fn write_eval_jsonl(
    path: &Path,
    cases: &[glyph::eval::controller::ControllerEvalCaseResult],
) -> Result<()> {
    let mut file =
        fs::File::create(path).with_context(|| format!("Failed to create {}", path.display()))?;
    for case in cases {
        writeln!(file, "{}", serde_json::to_string(case)?)?;
    }
    Ok(())
}
