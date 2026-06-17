use serde::Serialize;
use serde_json::Value;

use super::controller::ControllerEvalCaseResult;
use super::evidence::{
    ControllerClaimAuditInput, ControllerClaimAuditReport, ControllerClaimAuditStatus,
    audit_controller_claim,
};

pub const CONTROLLER_CLAIM_STATUS_VERSION: &str = "glyph-controller-claim-status/0.1";

#[derive(Debug, Clone, Copy)]
pub struct ControllerClaimStatusInput<'a> {
    pub cases: Option<&'a [ControllerEvalCaseResult]>,
    pub manifest: Option<&'a Value>,
    pub jsonl_path: Option<&'a str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerClaimStatusReport {
    pub version: String,
    pub claim: String,
    #[serde(rename = "claimAllowed")]
    pub claim_allowed: bool,
    pub phase: ControllerClaimStatusPhase,
    #[serde(rename = "staticReadinessPassed")]
    pub static_readiness_passed: bool,
    #[serde(rename = "liveEvidenceSupplied")]
    pub live_evidence_supplied: bool,
    #[serde(rename = "passedChecks")]
    pub passed_checks: Vec<ControllerClaimStatusCheckSummary>,
    #[serde(rename = "failedChecks")]
    pub failed_checks: Vec<ControllerClaimStatusCheckSummary>,
    #[serde(rename = "blockingReasons")]
    pub blocking_reasons: Vec<String>,
    #[serde(rename = "nextActions")]
    pub next_actions: Vec<String>,
    pub audit: ControllerClaimAuditReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ControllerClaimStatusPhase {
    StaticReadinessFailing,
    AwaitingLiveEvidence,
    LiveEvidenceFailing,
    ClaimReady,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerClaimStatusCheckSummary {
    pub id: String,
    pub observed: String,
    pub required: String,
}

pub fn controller_claim_status(
    input: ControllerClaimStatusInput<'_>,
) -> ControllerClaimStatusReport {
    let audit = audit_controller_claim(ControllerClaimAuditInput {
        cases: input.cases,
        manifest: input.manifest,
        jsonl_path: input.jsonl_path,
    });
    controller_claim_status_from_audit(audit)
}

pub fn controller_claim_status_from_audit(
    audit: ControllerClaimAuditReport,
) -> ControllerClaimStatusReport {
    let static_readiness_passed = static_check_ids().iter().all(|id| check_passed(&audit, id));
    let live_evidence_supplied =
        check_passed(&audit, "live_jsonl_supplied") && check_passed(&audit, "manifest_supplied");
    let phase = if audit.claim_ready {
        ControllerClaimStatusPhase::ClaimReady
    } else if !static_readiness_passed {
        ControllerClaimStatusPhase::StaticReadinessFailing
    } else if !live_evidence_supplied {
        ControllerClaimStatusPhase::AwaitingLiveEvidence
    } else {
        ControllerClaimStatusPhase::LiveEvidenceFailing
    };

    let passed_checks = audit
        .checks
        .iter()
        .filter(|check| check.status == ControllerClaimAuditStatus::Pass)
        .map(|check| ControllerClaimStatusCheckSummary {
            id: check.id.clone(),
            observed: check.observed.clone(),
            required: check.required.clone(),
        })
        .collect::<Vec<_>>();
    let failed_checks = audit
        .checks
        .iter()
        .filter(|check| check.status == ControllerClaimAuditStatus::Fail)
        .map(|check| ControllerClaimStatusCheckSummary {
            id: check.id.clone(),
            observed: check.observed.clone(),
            required: check.required.clone(),
        })
        .collect::<Vec<_>>();
    let blocking_reasons = failed_checks
        .iter()
        .map(|check| {
            format!(
                "{} failed: observed {}; required {}",
                check.id, check.observed, check.required
            )
        })
        .collect::<Vec<_>>();

    ControllerClaimStatusReport {
        version: CONTROLLER_CLAIM_STATUS_VERSION.to_string(),
        claim: "Glyph is best-in-lane for tiny controller model harness control".to_string(),
        claim_allowed: audit.claim_ready,
        phase,
        static_readiness_passed,
        live_evidence_supplied,
        passed_checks,
        failed_checks,
        blocking_reasons,
        next_actions: next_actions(phase),
        audit,
    }
}

fn static_check_ids() -> [&'static str; 5] {
    [
        "spec_fingerprint",
        "controller_dataset",
        "controller_curriculum",
        "benchmark_gate_documented",
        "adjacent_systems_documented",
    ]
}

fn check_passed(audit: &ControllerClaimAuditReport, id: &str) -> bool {
    audit
        .checks
        .iter()
        .any(|check| check.id == id && check.status == ControllerClaimAuditStatus::Pass)
}

fn next_actions(phase: ControllerClaimStatusPhase) -> Vec<String> {
    match phase {
        ControllerClaimStatusPhase::ClaimReady => vec![
            "Export the controller evidence pack with --require-claim-ready before publishing the claim."
                .to_string(),
            "Archive the JSONL, manifest, evidence pack, git commit, and model identifiers."
                .to_string(),
        ],
        ControllerClaimStatusPhase::StaticReadinessFailing => vec![
            "Run check-controller-dataset and check-controller-curriculum, then repair any failing static readiness checks."
                .to_string(),
            "Regenerate fingerprints and evidence pack after static proof artifacts pass.".to_string(),
        ],
        ControllerClaimStatusPhase::AwaitingLiveEvidence => vec![
            "Run a live OpenAI-compatible controller eval with 1b, 3b, 7b, and frontier buckets."
                .to_string(),
            "Use --prompt-mode all, --grammar-payload gbnf, --jsonl, --stream-jsonl, and --manifest."
                .to_string(),
            "Run verify-controller-run, coverage-controller, gate-controller, and audit-controller-claim on the completed live artifacts."
                .to_string(),
        ],
        ControllerClaimStatusPhase::LiveEvidenceFailing => vec![
            "Inspect failed live checks, then repair the failed layer: prompt, grammar, validator, corpus, harness contract, or model data."
                .to_string(),
            "Rerun only staged shards needed by coverage-controller before re-running the full gate."
                .to_string(),
        ],
    }
}
