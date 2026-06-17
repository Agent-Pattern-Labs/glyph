use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::controller::{
    ControllerEvalCaseResult, ControllerGrammarPayload, ControllerParameterClass,
    ControllerPromptMode,
};
use super::controller_examples::controller_eval_cases;

const REQUIRED_BUCKETS: &[&str] = &["1b", "3b", "7b", "frontier"];
const REQUIRED_PROMPT_MODES: &[&str] = &["constrained", "schema-only", "plain"];
const REQUIRED_PROFILES: &[&str] = &["normal", "terse", "noisy", "adversarial"];
const MISSING_COMPARISON_ROW_EXAMPLE_LIMIT: usize = 50;

#[derive(Debug, Clone, Serialize)]
pub struct ControllerCoverageReport {
    #[serde(rename = "caseRows")]
    pub case_rows: usize,
    #[serde(rename = "liveCaseRows")]
    pub live_case_rows: usize,
    #[serde(rename = "targetRows")]
    pub target_rows: usize,
    #[serde(rename = "requiredTargetRows")]
    pub required_target_rows: usize,
    #[serde(rename = "missingTargetRows")]
    pub missing_target_rows: usize,
    #[serde(rename = "requiredComparisonRows")]
    pub required_comparison_rows: usize,
    #[serde(rename = "observedComparisonRows")]
    pub observed_comparison_rows: usize,
    #[serde(rename = "missingComparisonRows")]
    pub missing_comparison_rows: usize,
    #[serde(rename = "coverageComplete")]
    pub coverage_complete: bool,
    #[serde(rename = "observedBuckets")]
    pub observed_buckets: Vec<String>,
    #[serde(rename = "missingBuckets")]
    pub missing_buckets: Vec<String>,
    #[serde(rename = "observedPromptModes")]
    pub observed_prompt_modes: Vec<String>,
    #[serde(rename = "missingPromptModes")]
    pub missing_prompt_modes: Vec<String>,
    #[serde(rename = "missingTargetCaseIds")]
    pub missing_target_case_ids: Vec<String>,
    #[serde(rename = "missingComparisonRowExamples")]
    pub missing_comparison_row_examples: Vec<ControllerMissingComparisonRow>,
    #[serde(rename = "familyProfiles")]
    pub family_profiles: Vec<ControllerFamilyProfileCoverage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ControllerMissingComparisonRow {
    #[serde(rename = "caseId")]
    pub case_id: String,
    pub bucket: String,
    #[serde(rename = "promptMode")]
    pub prompt_mode: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerFamilyProfileCoverage {
    pub family: String,
    #[serde(rename = "observedProfiles")]
    pub observed_profiles: Vec<String>,
    #[serde(rename = "missingProfiles")]
    pub missing_profiles: Vec<String>,
    #[serde(rename = "observedTargetRows")]
    pub observed_target_rows: usize,
    #[serde(rename = "requiredTargetRows")]
    pub required_target_rows: usize,
    #[serde(rename = "missingCaseIds")]
    pub missing_case_ids: Vec<String>,
}

pub fn controller_eval_coverage(cases: &[ControllerEvalCaseResult]) -> ControllerCoverageReport {
    let live_cases = cases
        .iter()
        .filter(|case| case.adapter_mode.is_live_evidence())
        .collect::<Vec<_>>();
    let target_cases = live_cases
        .iter()
        .copied()
        .filter(|case| is_target_case(case))
        .collect::<Vec<_>>();
    let expected = controller_eval_cases();
    let expected_case_ids = expected
        .iter()
        .map(|case| case.id.clone())
        .collect::<BTreeSet<_>>();
    let observed_target_case_ids = target_cases
        .iter()
        .map(|case| case.case_id.clone())
        .collect::<BTreeSet<_>>();
    let missing_target_case_ids = expected_case_ids
        .difference(&observed_target_case_ids)
        .cloned()
        .collect::<Vec<_>>();
    let missing_target_rows = missing_target_case_ids.len();
    let observed_buckets = observed_buckets(&live_cases);
    let observed_prompt_modes = observed_prompt_modes(&live_cases);
    let missing_buckets = missing_values(REQUIRED_BUCKETS, &observed_buckets);
    let missing_prompt_modes = missing_values(REQUIRED_PROMPT_MODES, &observed_prompt_modes);
    let observed_comparison_rows = observed_comparison_rows(&live_cases, &expected_case_ids);
    let required_comparison_rows =
        expected.len() * REQUIRED_BUCKETS.len() * REQUIRED_PROMPT_MODES.len();
    let missing_comparison_row_examples =
        missing_comparison_row_examples(&expected_case_ids, &observed_comparison_rows);
    let missing_comparison_rows = required_comparison_rows - observed_comparison_rows.len();
    let family_profiles = family_profile_coverage(&expected, &target_cases);
    let coverage_complete = missing_target_rows == 0
        && missing_comparison_rows == 0
        && missing_buckets.is_empty()
        && missing_prompt_modes.is_empty()
        && family_profiles
            .iter()
            .all(|family| family.missing_profiles.is_empty() && family.missing_case_ids.is_empty());

    ControllerCoverageReport {
        case_rows: cases.len(),
        live_case_rows: live_cases.len(),
        target_rows: target_cases.len(),
        required_target_rows: expected.len(),
        missing_target_rows,
        required_comparison_rows,
        observed_comparison_rows: observed_comparison_rows.len(),
        missing_comparison_rows,
        coverage_complete,
        observed_buckets,
        missing_buckets,
        observed_prompt_modes,
        missing_prompt_modes,
        missing_target_case_ids,
        missing_comparison_row_examples,
        family_profiles,
    }
}

fn is_target_case(case: &ControllerEvalCaseResult) -> bool {
    case.parameter_class == ControllerParameterClass::OneB
        && case.prompt_mode == ControllerPromptMode::Constrained
        && case.grammar_payload == ControllerGrammarPayload::Gbnf
}

fn observed_buckets(cases: &[&ControllerEvalCaseResult]) -> Vec<String> {
    cases
        .iter()
        .map(|case| case.parameter_class.as_str().to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn observed_prompt_modes(cases: &[&ControllerEvalCaseResult]) -> Vec<String> {
    cases
        .iter()
        .map(|case| case.prompt_mode.as_str().to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn missing_values(required: &[&str], observed: &[String]) -> Vec<String> {
    required
        .iter()
        .filter(|value| !observed.contains(&value.to_string()))
        .map(|value| (*value).to_string())
        .collect()
}

fn observed_comparison_rows(
    live_cases: &[&ControllerEvalCaseResult],
    expected_case_ids: &BTreeSet<String>,
) -> BTreeSet<(String, String, String)> {
    live_cases
        .iter()
        .filter(|case| expected_case_ids.contains(&case.case_id))
        .filter(|case| REQUIRED_BUCKETS.contains(&case.parameter_class.as_str()))
        .filter(|case| REQUIRED_PROMPT_MODES.contains(&case.prompt_mode.as_str()))
        .map(|case| {
            (
                case.case_id.clone(),
                case.parameter_class.as_str().to_string(),
                case.prompt_mode.as_str().to_string(),
            )
        })
        .collect()
}

fn missing_comparison_row_examples(
    expected_case_ids: &BTreeSet<String>,
    observed: &BTreeSet<(String, String, String)>,
) -> Vec<ControllerMissingComparisonRow> {
    let mut missing = Vec::new();
    for case_id in expected_case_ids {
        for bucket in REQUIRED_BUCKETS {
            for prompt_mode in REQUIRED_PROMPT_MODES {
                if !observed.contains(&(
                    case_id.clone(),
                    (*bucket).to_string(),
                    (*prompt_mode).to_string(),
                )) {
                    missing.push(ControllerMissingComparisonRow {
                        case_id: case_id.clone(),
                        bucket: (*bucket).to_string(),
                        prompt_mode: (*prompt_mode).to_string(),
                    });
                    if missing.len() >= MISSING_COMPARISON_ROW_EXAMPLE_LIMIT {
                        return missing;
                    }
                }
            }
        }
    }
    missing
}

fn family_profile_coverage(
    expected: &[super::controller_examples::ControllerEvalCase],
    target_cases: &[&ControllerEvalCaseResult],
) -> Vec<ControllerFamilyProfileCoverage> {
    let mut families = BTreeMap::<String, FamilyCoverageBuilder>::new();

    for expected_case in expected {
        let family = tag_value(&expected_case.tags, "family:").unwrap_or_else(|| "unknown".into());
        let profile =
            tag_value(&expected_case.tags, "profile:").unwrap_or_else(|| "unknown".into());
        let entry = families.entry(family).or_default();
        entry.required_case_ids.insert(expected_case.id.clone());
        entry.required_profiles.insert(profile);
    }

    for case in target_cases {
        let family = tag_value(&case.tags, "family:").unwrap_or_else(|| "unknown".into());
        let profile = tag_value(&case.tags, "profile:").unwrap_or_else(|| "unknown".into());
        let entry = families.entry(family).or_default();
        entry.observed_case_ids.insert(case.case_id.clone());
        entry.observed_profiles.insert(profile);
    }

    families
        .into_iter()
        .map(|(family, coverage)| {
            let observed_profiles = coverage
                .observed_profiles
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            let missing_profiles = REQUIRED_PROFILES
                .iter()
                .filter(|profile| !coverage.observed_profiles.contains(**profile))
                .map(|profile| (*profile).to_string())
                .collect::<Vec<_>>();
            let missing_case_ids = coverage
                .required_case_ids
                .difference(&coverage.observed_case_ids)
                .cloned()
                .collect::<Vec<_>>();

            ControllerFamilyProfileCoverage {
                family,
                observed_profiles,
                missing_profiles,
                observed_target_rows: coverage.observed_case_ids.len(),
                required_target_rows: coverage.required_case_ids.len(),
                missing_case_ids,
            }
        })
        .collect()
}

#[derive(Debug, Default)]
struct FamilyCoverageBuilder {
    required_profiles: BTreeSet<String>,
    observed_profiles: BTreeSet<String>,
    required_case_ids: BTreeSet<String>,
    observed_case_ids: BTreeSet<String>,
}

fn tag_value(tags: &[String], prefix: &str) -> Option<String> {
    tags.iter()
        .find_map(|tag| tag.strip_prefix(prefix).map(ToString::to_string))
}
