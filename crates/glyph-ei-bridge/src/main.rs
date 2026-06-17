use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use glyph_ei_bridge::{
    KILLER_REQUEST, comparison_summary, improvement_summary, loop_comparison_summary,
    report_summary, run_codex_comparison_eval, run_codex_comparison_eval_with_direct_output,
    run_improvement_loop, run_killer_eval, run_loop_comparison, run_semantic_control_suite,
    semantic_control_suite_summary, write_comparison_report, write_comparison_text_outputs,
    write_eval_report, write_improvement_artifacts, write_loop_comparison_artifacts,
    write_semantic_control_suite_report,
};

#[derive(Debug, Parser)]
#[command(
    name = "glyph-ei-bridge",
    about = "Run Glyph + Etymonoetic Interlingua bridge evals."
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the first meaning-gated bridge eval.
    Eval {
        /// Write the full JSON report to this path.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Compare a direct Codex-style answer against the EI + Glyph route.
    Compare {
        /// Read a real direct Codex output from this file instead of the built-in fixture.
        #[arg(long)]
        direct_output: Option<PathBuf>,
        /// Write direct output, EI+Glyph prompt, and EI+Glyph output into this directory.
        #[arg(long)]
        text_output_dir: Option<PathBuf>,
        /// Write the full JSON report to this path.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Run the reusable EI + Glyph content-improvement loop.
    Improve {
        /// Read the content request from this file. Defaults to the killer eval request.
        #[arg(long)]
        input: Option<PathBuf>,
        /// Pass the content request inline. Cannot be combined with --input.
        #[arg(long)]
        request: Option<String>,
        /// Write request, capsules, Glyph source, trace, prompt, outputs, report, and verdict here.
        #[arg(long, default_value = "out/improve")]
        output_dir: PathBuf,
    },
    /// Compare a Codex-style self-loop against the EI + Glyph trace-loop.
    LoopCompare {
        /// Read the content request from this file. Defaults to the killer eval request.
        #[arg(long)]
        input: Option<PathBuf>,
        /// Pass the content request inline. Cannot be combined with --input.
        #[arg(long)]
        request: Option<String>,
        /// Write prompts, traces, outputs, side-by-side markdown, report, and verdict here.
        #[arg(long, default_value = "out/loop-compare")]
        output_dir: PathBuf,
    },
    /// Run the 10-case semantic-control route comparison suite.
    SemanticSuite {
        /// Write the full JSON report to this path.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Eval { output } => {
            let report = run_killer_eval()?;
            if let Some(output) = output {
                write_eval_report(&report, &output)?;
                println!("WROTE {}", output.display());
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&report_summary(&report))?
            );
        }
        Command::Compare {
            direct_output,
            text_output_dir,
            output,
        } => {
            let report = if let Some(direct_output) = direct_output {
                let direct_output_text = fs::read_to_string(&direct_output)?;
                run_codex_comparison_eval_with_direct_output(&direct_output_text)?
            } else {
                run_codex_comparison_eval()?
            };
            if let Some(output) = output {
                write_comparison_report(&report, &output)?;
                println!("WROTE {}", output.display());
            }
            if let Some(text_output_dir) = text_output_dir {
                let text_outputs = write_comparison_text_outputs(&report, &text_output_dir)?;
                println!("WROTE {}", text_outputs.direct_path.display());
                println!("WROTE {}", text_outputs.ei_glyph_prompt_path.display());
                println!("WROTE {}", text_outputs.ei_glyph_path.display());
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&comparison_summary(&report))?
            );
        }
        Command::Improve {
            input,
            request,
            output_dir,
        } => {
            let request = match (input, request) {
                (Some(_), Some(_)) => anyhow::bail!("use either --input or --request, not both"),
                (Some(input), None) => fs::read_to_string(&input)?,
                (None, Some(request)) => request,
                (None, None) => KILLER_REQUEST.to_string(),
            };
            let report = run_improvement_loop(&request)?;
            let artifacts = write_improvement_artifacts(&report, &output_dir)?;
            println!("WROTE {}", artifacts.request_path.display());
            println!("WROTE {}", artifacts.capsules_path.display());
            println!("WROTE {}", artifacts.glyph_source_path.display());
            println!("WROTE {}", artifacts.glyph_trace_path.display());
            println!("WROTE {}", artifacts.writer_prompt_path.display());
            println!("WROTE {}", artifacts.baseline_output_path.display());
            println!("WROTE {}", artifacts.improved_output_path.display());
            println!("WROTE {}", artifacts.report_path.display());
            println!("WROTE {}", artifacts.verdict_path.display());
            println!(
                "{}",
                serde_json::to_string_pretty(&improvement_summary(&report))?
            );
        }
        Command::LoopCompare {
            input,
            request,
            output_dir,
        } => {
            let request = match (input, request) {
                (Some(_), Some(_)) => anyhow::bail!("use either --input or --request, not both"),
                (Some(input), None) => fs::read_to_string(&input)?,
                (None, Some(request)) => request,
                (None, None) => KILLER_REQUEST.to_string(),
            };
            let report = run_loop_comparison(&request)?;
            let artifacts = write_loop_comparison_artifacts(&report, &output_dir)?;
            println!("WROTE {}", artifacts.codex_self_loop_prompt_path.display());
            println!("WROTE {}", artifacts.codex_self_loop_trace_path.display());
            println!("WROTE {}", artifacts.codex_self_loop_output_path.display());
            println!("WROTE {}", artifacts.ei_glyph_prompt_path.display());
            println!("WROTE {}", artifacts.ei_glyph_trace_path.display());
            println!("WROTE {}", artifacts.ei_glyph_output_path.display());
            println!("WROTE {}", artifacts.side_by_side_path.display());
            println!("WROTE {}", artifacts.report_path.display());
            println!("WROTE {}", artifacts.verdict_path.display());
            println!(
                "{}",
                serde_json::to_string_pretty(&loop_comparison_summary(&report))?
            );
        }
        Command::SemanticSuite { output } => {
            let report = run_semantic_control_suite()?;
            if let Some(output) = output {
                write_semantic_control_suite_report(&report, &output)?;
                println!("WROTE {}", output.display());
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&semantic_control_suite_summary(&report))?
            );
        }
    }

    Ok(())
}
