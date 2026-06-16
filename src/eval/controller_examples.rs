#[derive(Debug, Clone, Copy)]
pub struct ControllerEvalCase {
    pub id: &'static str,
    pub request: &'static str,
    pub direct_natural_language_plan: &'static str,
    pub direct_failure_reason: &'static str,
    pub expected_glyph: &'static str,
    pub expects_repair_loop: bool,
    pub tags: &'static [&'static str],
}

pub const CONTROLLER_EVAL_CASES: &[ControllerEvalCase] = &[
    ControllerEvalCase {
        id: "hello_summary",
        request: "Say hello through the harness, summarize the result, and export it.",
        direct_natural_language_plan: "Capture hello world, summarize it, and export the summary. This is clear to a human, but it is not an executable Glyph program.",
        direct_failure_reason: "The prose plan has no typed primitive calls, assignments, or executable flow block.",
        expects_repair_loop: false,
        tags: &["simple", "summary", "export"],
        expected_glyph: include_str!("../../spec/fixtures/hello.glyph"),
    },
    ControllerEvalCase {
        id: "bounded_test_repair",
        request: "Read the app bundle, run tests, repair failures at most three times, and export the repaired files.",
        direct_natural_language_plan: "Read the app, run tests, fix issues until the tests pass, then export. If the tests keep failing, keep trying.",
        direct_failure_reason: "The prose plan has an unbounded repair instruction and no machine-checkable max iteration limit.",
        expects_repair_loop: true,
        tags: &["repair", "bounded-loop", "tests"],
        expected_glyph: include_str!("../../spec/fixtures/repair.glyph"),
    },
    ControllerEvalCase {
        id: "landing_page_checks",
        request: "Generate a responsive landing page for Glyph and check accessibility, responsiveness, and copy before export.",
        direct_natural_language_plan: "Make a nice landing page with a hero, features, pricing, and FAQ. Check it, improve it if needed, and return the final page.",
        direct_failure_reason: "The prose plan does not bind intermediate artifacts or specify typed check dimensions for the harness.",
        expects_repair_loop: false,
        tags: &["codegen", "checks", "export"],
        expected_glyph: include_str!("../examples/generate_landing_page.glyph"),
    },
    ControllerEvalCase {
        id: "data_cleanup_pipeline",
        request: "Build a local customer-record cleanup pipeline with dedupe, email normalization, validation, patching, and export.",
        direct_natural_language_plan: "Create a cleanup script for customers that deduplicates records, normalizes emails, validates the output, patches issues, and exports the result.",
        direct_failure_reason: "The prose plan does not expose structured rule arguments, command targets, or patch inputs.",
        expects_repair_loop: false,
        tags: &["data", "run", "patch"],
        expected_glyph: include_str!("../examples/data_cleanup_pipeline.glyph"),
    },
    ControllerEvalCase {
        id: "security_review_report",
        request: "Review a local app bundle for secrets, dependency risks, and authorization issues, then export a security report.",
        direct_natural_language_plan: "Look through the app for security problems like secrets, dependency issues, and authz bugs. Summarize the findings and produce a report.",
        direct_failure_reason: "The prose plan has no typed read/check/summarize/export sequence that the runtime can execute deterministically.",
        expects_repair_loop: false,
        tags: &["security", "review", "summary"],
        expected_glyph: include_str!("../examples/security_review.glyph"),
    },
];
