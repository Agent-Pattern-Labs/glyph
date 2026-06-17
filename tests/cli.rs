use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::str::contains;
use serde_json::Value;

use etymonoetic_interlingua::{
    load_schema, make_capsule_template, training_records, validate_capsule, validate_file,
    wiktionary_source,
};

fn repo_path(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn capsule_paths() -> Vec<PathBuf> {
    let mut paths = fs::read_dir(repo_path("capsules/en"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

fn command() -> Command {
    Command::cargo_bin("ei").unwrap()
}

#[test]
fn schema_loads() {
    let schema = load_schema().unwrap();

    assert_eq!(schema["title"], "Etymonoetic Semantic Capsule");
    assert_eq!(schema["properties"]["schema_version"]["const"], "0.1.0");
}

#[test]
fn examples_validate() {
    for path in ["examples/iconoclast.json", "examples/radical.json"] {
        let capsule = validate_file(repo_path(path)).unwrap();
        assert_eq!(capsule["schema_version"], "0.1.0");
    }
}

#[test]
fn curated_capsules_validate() {
    let paths = capsule_paths();

    assert!(paths.len() >= 10);
    for path in paths {
        let capsule = validate_file(&path).unwrap();
        assert!(capsule["id"].as_str().unwrap().starts_with("ei:en:"));
        assert!(
            capsule["provenance"]
                .as_array()
                .unwrap()
                .iter()
                .any(|source| source["source_type"] == "dictionary")
        );
    }
}

#[test]
fn capsule_manifest_paths_match_ids() {
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(repo_path("capsules/manifest.json")).unwrap())
            .unwrap();

    assert_eq!(manifest["set_id"], "ei:capsules:en:cited-v0");
    for item in manifest["capsules"].as_array().unwrap() {
        let path = repo_path(item["path"].as_str().unwrap());
        let capsule = validate_file(&path).unwrap();
        assert!(path.exists());
        assert_eq!(capsule["id"], item["id"]);
        assert_eq!(capsule["surface"]["form"], item["surface_form"]);
    }
}

#[test]
fn template_generates_valid_capsule() {
    let capsule = make_capsule_template("Sincere", "en", "adjective", None, None).unwrap();
    let validated = validate_capsule(capsule).unwrap();

    assert_eq!(validated["id"], "ei:en:sincere");
    assert_eq!(validated["surface"]["normalized_form"], "sincere");
    assert_eq!(validated["uncertainty"]["overall"], "unknown");
}

#[test]
fn unknown_provenance_refs_are_rejected() {
    let mut capsule = validate_file(repo_path("examples/radical.json")).unwrap();
    capsule["morphology"]["segments"][0]["provenance_refs"] = serde_json::json!(["missing-source"]);

    let error = validate_capsule(capsule).unwrap_err().to_string();
    assert!(error.contains("unknown provenance ref \"missing-source\""));
}

#[test]
fn wiktionary_source_can_seed_template() {
    let source = wiktionary_source("sincere", "en").unwrap().to_provenance();
    let capsule = make_capsule_template("sincere", "en", "adjective", None, Some(source)).unwrap();
    let validated = validate_capsule(capsule).unwrap();

    assert_eq!(validated["provenance"][0]["id"], "wiktionary-en-sincere");
    assert_eq!(
        validated["provenance"][0]["url"],
        "https://en.wiktionary.org/wiki/sincere"
    );
}

#[test]
fn template_source_id_overrides_provenance_id_consistently() {
    let source = wiktionary_source("sincere", "en").unwrap().to_provenance();
    let capsule = make_capsule_template(
        "sincere",
        "en",
        "adjective",
        Some("custom-source"),
        Some(source),
    )
    .unwrap();
    let validated = validate_capsule(capsule).unwrap();

    assert_eq!(validated["provenance"][0]["id"], "custom-source");
    assert_eq!(
        validated["morphology"]["provenance_refs"],
        serde_json::json!(["custom-source"])
    );
}

#[test]
fn training_records_emit_two_tasks_per_capsule() {
    let capsule = validate_file(repo_path("examples/iconoclast.json")).unwrap();
    let records = training_records(&[capsule]);

    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["task"], "text_to_capsule");
    assert_eq!(records[1]["task"], "capsule_to_expansion");
    assert_eq!(records[0]["output"]["id"], "ei:en:iconoclast");
}

#[test]
fn cli_validate_examples() {
    command()
        .args([
            "validate",
            "examples/iconoclast.json",
            "examples/radical.json",
        ])
        .assert()
        .success()
        .stdout(contains("OK examples/iconoclast.json"));
}

#[test]
fn cli_show_summary() {
    command()
        .args(["show", "examples/iconoclast.json"])
        .assert()
        .success()
        .stdout(contains("iconoclast (en)"))
        .stdout(contains("Present senses:"));
}

#[test]
fn cli_new_writes_valid_starter() {
    let temp = std::env::temp_dir().join(format!("ei-test-{}", std::process::id()));
    fs::create_dir_all(&temp).unwrap();
    let output = temp.join("sincere.json");

    command()
        .args([
            "new",
            "sincere",
            "--part-of-speech",
            "adjective",
            "--output",
            output.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(contains("WROTE"));

    assert_eq!(
        validate_file(&output).unwrap()["surface"]["form"],
        "sincere"
    );
}

#[test]
fn cli_new_can_seed_wiktionary_source() {
    let temp = std::env::temp_dir().join(format!("ei-test-wiktionary-{}", std::process::id()));
    fs::create_dir_all(&temp).unwrap();
    let output = temp.join("sincere.json");

    command()
        .args([
            "new",
            "sincere",
            "--part-of-speech",
            "adjective",
            "--wiktionary-source",
            "--output",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    let capsule = validate_file(&output).unwrap();
    assert_eq!(capsule["provenance"][0]["source_type"], "dictionary");
    assert_eq!(
        capsule["provenance"][0]["url"],
        "https://en.wiktionary.org/wiki/sincere"
    );
}

#[test]
fn cli_expand_with_trace() {
    command()
        .args(["expand", "examples/radical.json", "--trace"])
        .assert()
        .success()
        .stdout(contains("Radical should not be reduced to extreme."))
        .stdout(contains("Trace:"));
}

#[test]
fn cli_export_training_jsonl() {
    let temp = std::env::temp_dir().join(format!("ei-test-training-{}", std::process::id()));
    fs::create_dir_all(&temp).unwrap();
    let output = temp.join("training.jsonl");

    command()
        .args([
            "export-training",
            "examples/iconoclast.json",
            "--output",
            output.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(contains("WROTE"));

    let lines = fs::read_to_string(output).unwrap();
    let records = lines
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();

    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["task"], "text_to_capsule");
    assert_eq!(records[1]["task"], "capsule_to_expansion");
}
