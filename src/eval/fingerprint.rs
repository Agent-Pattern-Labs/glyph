use std::collections::BTreeSet;

use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::language::grammar::{
    GLYPH_CONTROLLER_OUTPUT_JSON_SCHEMA, GLYPH_EBNF, GLYPH_GBNF, GLYPH_PRIMITIVES,
};

use super::controller::{
    ControllerGrammarPayload, ControllerPromptMode, ControllerRequestKind,
    GENERIC_TOOL_PLAN_JSON_SCHEMA, build_openai_compatible_request_body,
};
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
    #[serde(rename = "requestContract")]
    pub request_contract: RequestContractFingerprint,
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

#[derive(Debug, Clone, Serialize)]
pub struct RequestContractFingerprint {
    #[serde(rename = "modelId")]
    pub model_id: String,
    #[serde(rename = "requestCount")]
    pub request_count: usize,
    #[serde(rename = "promptModes")]
    pub prompt_modes: Vec<String>,
    #[serde(rename = "grammarPayloads")]
    pub grammar_payloads: Vec<String>,
    #[serde(rename = "requestKinds")]
    pub request_kinds: Vec<String>,
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
    let request_contract = request_contract_fingerprint();
    let overall_sha256 = sha256_hex(
        serde_json::to_string(&json!({
            "specArtifacts": spec_artifacts,
            "evalCorpus": eval_corpus,
            "requestContract": request_contract,
        }))
        .expect("fingerprint payload serializes")
        .as_bytes(),
    );

    ControllerEvalFingerprint {
        algorithm: "sha256".to_string(),
        overall_sha256,
        spec_artifacts,
        eval_corpus,
        request_contract,
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

fn request_contract_fingerprint() -> RequestContractFingerprint {
    let model_id = "glyph-fingerprint-model".to_string();
    let prompt_modes = ControllerPromptMode::all();
    let grammar_payloads = [
        ControllerGrammarPayload::None,
        ControllerGrammarPayload::Gbnf,
    ];
    let request_kinds = [
        ControllerRequestKind::Glyph,
        ControllerRequestKind::JsonToolPlan,
        ControllerRequestKind::DirectProse,
    ];
    let cases = controller_eval_cases();
    let mut payload = Vec::new();
    for case in &cases {
        for prompt_mode in &prompt_modes {
            for grammar_payload in &grammar_payloads {
                for request_kind in &request_kinds {
                    payload.push(json!({
                        "caseId": case.id,
                        "promptMode": prompt_mode.as_str(),
                        "grammarPayload": grammar_payload.as_str(),
                        "requestKind": request_kind.as_str(),
                        "body": build_openai_compatible_request_body(
                            &model_id,
                            case,
                            *prompt_mode,
                            *grammar_payload,
                            *request_kind,
                        ),
                    }));
                }
            }
        }
    }
    let serialized = serde_json::to_string(&payload).expect("request contract serializes");

    RequestContractFingerprint {
        model_id,
        request_count: payload.len(),
        prompt_modes: prompt_modes
            .into_iter()
            .map(|mode| mode.as_str().to_string())
            .collect(),
        grammar_payloads: grammar_payloads
            .into_iter()
            .map(|payload| payload.as_str().to_string())
            .collect(),
        request_kinds: request_kinds
            .into_iter()
            .map(|kind| kind.as_str().to_string())
            .collect(),
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
