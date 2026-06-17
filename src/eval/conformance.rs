use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::harness::mock_tools::create_mock_tool_registry;
use crate::ir::glyph_ir::parse_glyph_to_ir;
use crate::ir::validate_ir::validate_ir;
use crate::runtime::glyph_vm::GlyphVm;

pub const GLYPH_CONFORMANCE_VERSION: &str = "glyph-conformance/0.1";

const EXAMPLES: &[(&str, &str)] = &[
    (
        "src/examples/build_crud_app.glyph",
        include_str!("../examples/build_crud_app.glyph"),
    ),
    (
        "src/examples/customer_support_reply_stub.glyph",
        include_str!("../examples/customer_support_reply_stub.glyph"),
    ),
    (
        "src/examples/data_cleanup_pipeline.glyph",
        include_str!("../examples/data_cleanup_pipeline.glyph"),
    ),
    (
        "src/examples/generate_landing_page.glyph",
        include_str!("../examples/generate_landing_page.glyph"),
    ),
    (
        "src/examples/hello.glyph",
        include_str!("../examples/hello.glyph"),
    ),
    (
        "src/examples/meeting_notes_to_tasks.glyph",
        include_str!("../examples/meeting_notes_to_tasks.glyph"),
    ),
    (
        "src/examples/repair_failing_tests.glyph",
        include_str!("../examples/repair_failing_tests.glyph"),
    ),
    (
        "src/examples/security_review.glyph",
        include_str!("../examples/security_review.glyph"),
    ),
    (
        "src/examples/summarize_docs.glyph",
        include_str!("../examples/summarize_docs.glyph"),
    ),
];

#[derive(Debug, Clone, Serialize)]
pub struct GlyphConformanceReport {
    pub version: String,
    pub passed: bool,
    #[serde(rename = "exampleCount")]
    pub example_count: usize,
    #[serde(rename = "parsePassed")]
    pub parse_passed: usize,
    #[serde(rename = "validationPassed")]
    pub validation_passed: usize,
    #[serde(rename = "runPassed")]
    pub run_passed: usize,
    pub examples: Vec<GlyphConformanceExample>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlyphConformanceExample {
    pub path: String,
    #[serde(rename = "sourceSha256")]
    pub source_sha256: String,
    #[serde(rename = "sourceBytes")]
    pub source_bytes: usize,
    #[serde(rename = "parseOk")]
    pub parse_ok: bool,
    #[serde(rename = "validationOk")]
    pub validation_ok: bool,
    #[serde(rename = "runOk")]
    pub run_ok: bool,
    #[serde(rename = "flowCount")]
    pub flow_count: usize,
    #[serde(rename = "traceEventCount")]
    pub trace_event_count: usize,
    #[serde(rename = "finalOutputCount")]
    pub final_output_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn glyph_conformance_report() -> GlyphConformanceReport {
    let vm = GlyphVm::new(create_mock_tool_registry());
    let examples = EXAMPLES
        .iter()
        .map(|(path, source)| conformance_example(&vm, path, source))
        .collect::<Vec<_>>();
    let parse_passed = examples.iter().filter(|example| example.parse_ok).count();
    let validation_passed = examples
        .iter()
        .filter(|example| example.validation_ok)
        .count();
    let run_passed = examples.iter().filter(|example| example.run_ok).count();
    let passed = !examples.is_empty()
        && parse_passed == examples.len()
        && validation_passed == examples.len()
        && run_passed == examples.len();

    GlyphConformanceReport {
        version: GLYPH_CONFORMANCE_VERSION.to_string(),
        passed,
        example_count: examples.len(),
        parse_passed,
        validation_passed,
        run_passed,
        examples,
    }
}

fn conformance_example(vm: &GlyphVm, path: &str, source: &str) -> GlyphConformanceExample {
    let mut example = GlyphConformanceExample {
        path: path.to_string(),
        source_sha256: sha256(source.as_bytes()),
        source_bytes: source.len(),
        parse_ok: false,
        validation_ok: false,
        run_ok: false,
        flow_count: 0,
        trace_event_count: 0,
        final_output_count: 0,
        error: None,
    };

    let ir = match parse_glyph_to_ir(source) {
        Ok(ir) => {
            example.parse_ok = true;
            example.flow_count = ir.flows.len();
            ir
        }
        Err(error) => {
            example.error = Some(error.to_string());
            return example;
        }
    };

    let ir = match validate_ir(ir) {
        Ok(ir) => {
            example.validation_ok = true;
            ir
        }
        Err(error) => {
            example.error = Some(error.to_string());
            return example;
        }
    };

    match vm.execute(ir, Default::default()) {
        Ok(result) => {
            example.run_ok = true;
            example.trace_event_count = result.trace.len();
            example.final_output_count = result.outputs.len();
        }
        Err(error) => example.error = Some(error.to_string()),
    }

    example
}

fn sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
