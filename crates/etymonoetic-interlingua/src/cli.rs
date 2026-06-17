use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use crate::sources::wiktionary_source;
use crate::templates::make_capsule_template;
use crate::training::training_records;
use crate::validator::{load_schema, validate_capsule, validate_file};

#[derive(Debug, Parser)]
#[command(
    name = "ei",
    about = "Validate and inspect etymonoetic semantic capsules."
)]
pub struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Validate one or more capsule files.
    Validate { paths: Vec<PathBuf> },
    /// Create a valid starter capsule.
    New {
        form: String,
        #[arg(long, default_value = "en")]
        language: String,
        #[arg(long, default_value = "unknown")]
        part_of_speech: String,
        #[arg(long)]
        wiktionary_source: bool,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Print a compact capsule summary.
    Show { path: PathBuf },
    /// Print the capsule expansion paragraph.
    Expand {
        path: PathBuf,
        #[arg(long)]
        trace: bool,
    },
    /// Export validated capsules as JSONL training records.
    ExportTraining {
        paths: Vec<PathBuf>,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Print the bundled capsule JSON Schema.
    Schema,
}

pub fn run() -> Result<i32> {
    run_with_args(Args::parse())
}

pub fn run_with_args(args: Args) -> Result<i32> {
    match args.command {
        Command::Validate { paths } => run_validate(paths),
        Command::New {
            form,
            language,
            part_of_speech,
            wiktionary_source: seed_wiktionary,
            output,
        } => run_new(&form, &language, &part_of_speech, seed_wiktionary, output),
        Command::Show { path } => run_show(path),
        Command::Expand { path, trace } => run_expand(path, trace),
        Command::ExportTraining { paths, output } => run_export_training(paths, output),
        Command::Schema => run_schema(),
    }
}

fn run_validate(paths: Vec<PathBuf>) -> Result<i32> {
    let mut ok = true;

    for path in paths {
        match validate_file(&path) {
            Ok(_) => println!("OK {}", path.display()),
            Err(error) => {
                ok = false;
                eprintln!("FAIL {}", path.display());
                eprintln!("{error}");
            }
        }
    }

    Ok(if ok { 0 } else { 1 })
}

fn run_new(
    form: &str,
    language: &str,
    part_of_speech: &str,
    seed_wiktionary: bool,
    output: Option<PathBuf>,
) -> Result<i32> {
    let provenance = if seed_wiktionary {
        Some(wiktionary_source(form, language)?.to_provenance())
    } else {
        None
    };
    let capsule = validate_capsule(make_capsule_template(
        form,
        language,
        part_of_speech,
        None,
        provenance,
    )?)?;
    write_json_or_stdout(&capsule, output.as_ref())?;

    if let Some(output) = output {
        println!("WROTE {}", output.display());
    }

    Ok(0)
}

fn run_show(path: PathBuf) -> Result<i32> {
    let capsule = validate_file(path)?;
    let surface = &capsule["surface"];

    println!(
        "{} ({})",
        surface["form"].as_str().unwrap_or("unknown"),
        surface["language"].as_str().unwrap_or("unknown")
    );
    println!("{}", capsule["capsule_summary"].as_str().unwrap_or(""));
    println!();
    println!("Present senses:");

    if let Some(senses) = capsule["present_usage"]["senses"].as_array() {
        for sense in senses {
            println!(
                "- {}: {}",
                sense["id"].as_str().unwrap_or("unknown"),
                sense["definition"].as_str().unwrap_or("")
            );
        }
    }

    Ok(0)
}

fn run_expand(path: PathBuf, trace: bool) -> Result<i32> {
    let capsule = validate_file(path)?;
    println!(
        "{}",
        capsule["expansion"]["paragraph"].as_str().unwrap_or("")
    );

    if trace {
        println!();
        println!("Trace:");
        if let Some(steps) = capsule["expansion"]["trace"].as_array() {
            for step in steps {
                println!(
                    "- {}: {}",
                    step["layer"].as_str().unwrap_or("unknown"),
                    step["contribution"].as_str().unwrap_or("")
                );
            }
        }
    }

    Ok(0)
}

fn run_export_training(paths: Vec<PathBuf>, output: Option<PathBuf>) -> Result<i32> {
    let capsules = paths
        .iter()
        .map(validate_file)
        .collect::<Result<Vec<_>>>()?;
    let content = training_records(&capsules)
        .into_iter()
        .map(|record| serde_json::to_string(&record))
        .collect::<std::result::Result<Vec<_>, _>>()?
        .join("\n")
        + "\n";

    write_text_or_stdout(&content, output.as_ref())?;

    if let Some(output) = output {
        println!("WROTE {}", output.display());
    }

    Ok(0)
}

fn run_schema() -> Result<i32> {
    println!("{}", serde_json::to_string_pretty(&load_schema()?)?);
    Ok(0)
}

fn write_json_or_stdout(value: &serde_json::Value, output: Option<&PathBuf>) -> Result<()> {
    let content = serde_json::to_string_pretty(value)? + "\n";
    write_text_or_stdout(&content, output)
}

fn write_text_or_stdout(content: &str, output: Option<&PathBuf>) -> Result<()> {
    if let Some(output) = output {
        if let Some(parent) = output.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
        }
        fs::write(output, content)
            .with_context(|| format!("failed to write {}", output.display()))?;
    } else {
        print!("{content}");
    }
    Ok(())
}
