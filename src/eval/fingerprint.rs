use std::collections::BTreeSet;

use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::language::grammar::{
    GLYPH_CONTROLLER_OUTPUT_JSON_SCHEMA, GLYPH_EBNF, GLYPH_GBNF, GLYPH_PRIMITIVES,
};

use super::controller::GENERIC_TOOL_PLAN_JSON_SCHEMA;
use super::controller_examples::controller_eval_cases;

const GLYPH_IR_JSON_SCHEMA: &str = include_str!("../../spec/glyph-ir.schema.json");

#[derive(Debug, Clone, Serialize)]
pub struct ControllerEvalFingerprint {
    pub algorithm: String,
    #[serde(rename = "overallSha256")]
    pub overall_sha256: String,
    #[serde(rename = "specArtifacts")]
    pub spec_artifacts: Vec<ArtifactFingerprint>,
    #[serde(rename = "evalCorpus")]
    pub eval_corpus: EvalCorpusFingerprint,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtifactFingerprint {
    pub name: String,
    pub bytes: usize,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvalCorpusFingerprint {
    #[serde(rename = "caseCount")]
    pub case_count: usize,
    pub families: Vec<String>,
    pub profiles: Vec<String>,
    pub sha256: String,
}

pub fn controller_eval_fingerprint() -> ControllerEvalFingerprint {
    let spec_artifacts = vec![
        artifact_fingerprint("glyph.ebnf", GLYPH_EBNF),
        artifact_fingerprint("glyph.gbnf", GLYPH_GBNF),
        artifact_fingerprint(
            "controller-output.schema.json",
            GLYPH_CONTROLLER_OUTPUT_JSON_SCHEMA,
        ),
        artifact_fingerprint(
            "generic-tool-plan.schema.json",
            GENERIC_TOOL_PLAN_JSON_SCHEMA,
        ),
        artifact_fingerprint("glyph-ir.schema.json", GLYPH_IR_JSON_SCHEMA),
        artifact_fingerprint("glyph-primitives", &GLYPH_PRIMITIVES.join("\n")),
    ];
    let eval_corpus = eval_corpus_fingerprint();
    let overall_sha256 = sha256_hex(
        serde_json::to_string(&json!({
            "specArtifacts": spec_artifacts,
            "evalCorpus": eval_corpus,
        }))
        .expect("fingerprint payload serializes")
        .as_bytes(),
    );

    ControllerEvalFingerprint {
        algorithm: "sha256".to_string(),
        overall_sha256,
        spec_artifacts,
        eval_corpus,
    }
}

fn artifact_fingerprint(name: &str, contents: &str) -> ArtifactFingerprint {
    ArtifactFingerprint {
        name: name.to_string(),
        bytes: contents.len(),
        sha256: sha256_hex(contents.as_bytes()),
    }
}

fn eval_corpus_fingerprint() -> EvalCorpusFingerprint {
    let cases = controller_eval_cases();
    let mut families = BTreeSet::new();
    let mut profiles = BTreeSet::new();
    let payload = cases
        .iter()
        .map(|case| {
            for tag in &case.tags {
                if let Some(family) = tag.strip_prefix("family:") {
                    families.insert(family.to_string());
                }
                if let Some(profile) = tag.strip_prefix("profile:") {
                    profiles.insert(profile.to_string());
                }
            }
            json!({
                "id": case.id,
                "request": case.request,
                "directNaturalLanguagePlan": case.direct_natural_language_plan,
                "directFailureReason": case.direct_failure_reason,
                "expectedGlyph": case.expected_glyph,
                "expectsRepairLoop": case.expects_repair_loop,
                "tags": case.tags,
            })
        })
        .collect::<Vec<_>>();
    let serialized = serde_json::to_string(&payload).expect("eval corpus payload serializes");

    EvalCorpusFingerprint {
        case_count: cases.len(),
        families: families.into_iter().collect(),
        profiles: profiles.into_iter().collect(),
        sha256: sha256_hex(serialized.as_bytes()),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}
