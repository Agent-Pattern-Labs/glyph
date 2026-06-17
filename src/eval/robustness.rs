use std::collections::BTreeMap;

use serde::Serialize;

use crate::ir::glyph_ir::parse_glyph_to_ir;
use crate::ir::validate_ir::validate_ir;

use super::controller_examples::controller_eval_cases;

const MIN_CASES: usize = 72;
const MIN_REPAIR_MUTATIONS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ControllerRobustnessDecision {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ControllerRobustnessStatus {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerRobustnessReport {
    pub decision: ControllerRobustnessDecision,
    pub passed: bool,
    pub metrics: ControllerRobustnessMetrics,
    pub checks: Vec<ControllerRobustnessCheck>,
    #[serde(rename = "acceptedMutations")]
    pub accepted_mutations: Vec<ControllerRobustnessAcceptedMutation>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerRobustnessMetrics {
    #[serde(rename = "caseCount")]
    pub case_count: usize,
    #[serde(rename = "mutationCount")]
    pub mutation_count: usize,
    #[serde(rename = "rejectedMutations")]
    pub rejected_mutations: usize,
    #[serde(rename = "acceptedMutationCount")]
    pub accepted_mutation_count: usize,
    #[serde(rename = "byKind")]
    pub by_kind: BTreeMap<String, ControllerRobustnessKindMetrics>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ControllerRobustnessKindMetrics {
    #[serde(rename = "mutationCount")]
    pub mutation_count: usize,
    #[serde(rename = "rejectedMutations")]
    pub rejected_mutations: usize,
    #[serde(rename = "acceptedMutations")]
    pub accepted_mutations: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerRobustnessCheck {
    pub id: String,
    pub status: ControllerRobustnessStatus,
    pub observed: String,
    pub required: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerRobustnessAcceptedMutation {
    #[serde(rename = "caseId")]
    pub case_id: String,
    pub kind: String,
}

struct RobustnessMutation {
    kind: &'static str,
    source: String,
}

pub fn evaluate_controller_robustness() -> ControllerRobustnessReport {
    let cases = controller_eval_cases();
    let mut by_kind = BTreeMap::<String, ControllerRobustnessKindMetrics>::new();
    let mut accepted_mutations = Vec::new();
    let mut mutation_count = 0;
    let mut rejected_mutations = 0;

    for case in &cases {
        for mutation in mutations_for_target(&case.expected_glyph) {
            mutation_count += 1;
            let kind_metrics = by_kind.entry(mutation.kind.to_string()).or_default();
            kind_metrics.mutation_count += 1;

            if mutation_is_rejected(&mutation.source) {
                rejected_mutations += 1;
                kind_metrics.rejected_mutations += 1;
            } else {
                kind_metrics.accepted_mutations += 1;
                accepted_mutations.push(ControllerRobustnessAcceptedMutation {
                    case_id: case.id.clone(),
                    kind: mutation.kind.to_string(),
                });
            }
        }
    }

    let metrics = ControllerRobustnessMetrics {
        case_count: cases.len(),
        mutation_count,
        rejected_mutations,
        accepted_mutation_count: accepted_mutations.len(),
        by_kind,
    };

    let checks = vec![
        robustness_check(
            "case_coverage",
            metrics.case_count >= MIN_CASES,
            metrics.case_count.to_string(),
            format!(">= {MIN_CASES} controller eval cases"),
        ),
        robustness_check(
            "all_mutations_rejected",
            metrics.mutation_count > 0 && metrics.accepted_mutation_count == 0,
            format!(
                "accepted={}, rejected={}, total={}",
                metrics.accepted_mutation_count, metrics.rejected_mutations, metrics.mutation_count
            ),
            "every deterministic invalid mutation is rejected by parse or semantic validation"
                .to_string(),
        ),
        kind_check(
            &metrics,
            "unknown_tool",
            MIN_CASES,
            "unknown tool mutations are rejected for every case",
        ),
        kind_check(
            &metrics,
            "unknown_variable",
            MIN_CASES,
            "unknown variable mutations are rejected for every case",
        ),
        kind_check(
            &metrics,
            "invalid_repair_max",
            MIN_REPAIR_MUTATIONS,
            "invalid repair max mutations are rejected for repair-loop cases",
        ),
    ];
    let passed = checks
        .iter()
        .all(|check| check.status == ControllerRobustnessStatus::Pass);

    ControllerRobustnessReport {
        decision: if passed {
            ControllerRobustnessDecision::Pass
        } else {
            ControllerRobustnessDecision::Fail
        },
        passed,
        metrics,
        checks,
        accepted_mutations,
    }
}

fn mutations_for_target(target: &str) -> Vec<RobustnessMutation> {
    let mut mutations = vec![
        RobustnessMutation {
            kind: "unknown_tool",
            source: replace_first_or_append_unknown_tool(target),
        },
        RobustnessMutation {
            kind: "unknown_variable",
            source: format!(
                "{}\n\nflow invalid_probe {{\n  PLAN(missing) -> plan\n}}\n",
                target.trim_end()
            ),
        },
    ];

    if target.contains("repair ") && target.contains(" max 3 ") {
        mutations.push(RobustnessMutation {
            kind: "invalid_repair_max",
            source: target.replacen(" max 3 ", " max 0 ", 1),
        });
    }

    mutations
}

fn replace_first_or_append_unknown_tool(target: &str) -> String {
    if target.contains("EXPORT(") {
        target.replacen("EXPORT(", "NOPE(", 1)
    } else {
        format!(
            "{}\n\nflow invalid_probe {{\n  NOPE(input=\"invalid\") -> result\n}}\n",
            target.trim_end()
        )
    }
}

fn mutation_is_rejected(source: &str) -> bool {
    match parse_glyph_to_ir(source) {
        Ok(ir) => validate_ir(ir).is_err(),
        Err(_) => true,
    }
}

fn kind_check(
    metrics: &ControllerRobustnessMetrics,
    kind: &str,
    min_mutations: usize,
    required: &str,
) -> ControllerRobustnessCheck {
    let kind_metrics = metrics.by_kind.get(kind).cloned().unwrap_or_default();
    robustness_check(
        kind,
        kind_metrics.mutation_count >= min_mutations && kind_metrics.accepted_mutations == 0,
        format!(
            "mutations={}, rejected={}, accepted={}",
            kind_metrics.mutation_count,
            kind_metrics.rejected_mutations,
            kind_metrics.accepted_mutations
        ),
        required.to_string(),
    )
}

fn robustness_check(
    id: &str,
    passed: bool,
    observed: String,
    required: String,
) -> ControllerRobustnessCheck {
    ControllerRobustnessCheck {
        id: id.to_string(),
        status: if passed {
            ControllerRobustnessStatus::Pass
        } else {
            ControllerRobustnessStatus::Fail
        },
        observed,
        required,
    }
}
