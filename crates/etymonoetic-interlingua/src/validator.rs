use std::collections::HashSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::Value;

const SCHEMA: &str = include_str!("../schemas/semantic-capsule.schema.json");

pub fn load_schema() -> Result<Value> {
    serde_json::from_str(SCHEMA).context("failed to parse bundled semantic capsule schema")
}

pub fn load_capsule(path: impl AsRef<Path>) -> Result<Value> {
    let path = path.as_ref();
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read capsule {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("failed to parse capsule JSON {}", path.display()))
}

pub fn validate_file(path: impl AsRef<Path>) -> Result<Value> {
    validate_capsule(load_capsule(path)?)
}

pub fn validate_capsule(capsule: Value) -> Result<Value> {
    let schema = load_schema()?;
    let validator =
        jsonschema::validator_for(&schema).context("failed to compile capsule schema")?;
    let mut errors = validator
        .iter_errors(&capsule)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect::<Vec<_>>();
    errors.sort();

    errors.extend(validate_provenance_refs(&capsule));

    if !errors.is_empty() {
        bail!(errors.join("\n"));
    }

    Ok(capsule)
}

pub fn validate_provenance_refs(capsule: &Value) -> Vec<String> {
    let provenance_ids = capsule
        .get("provenance")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("id").and_then(Value::as_str).map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut errors = Vec::new();
    let mut seen = HashSet::new();
    let mut duplicates = HashSet::new();

    for id in &provenance_ids {
        if !seen.insert(id.clone()) {
            duplicates.insert(id.clone());
        }
    }

    for duplicate in sorted_strings(duplicates) {
        errors.push(format!("provenance: duplicate id {duplicate:?}"));
    }

    let known = provenance_ids.into_iter().collect::<HashSet<_>>();
    collect_unknown_provenance_refs(capsule, "", &known, &mut errors);
    errors
}

fn collect_unknown_provenance_refs(
    value: &Value,
    path: &str,
    known: &HashSet<String>,
    errors: &mut Vec<String>,
) {
    match value {
        Value::Object(map) => {
            for (key, nested) in map {
                let nested_path = append_path(path, key);
                if key == "provenance_refs" {
                    if let Some(refs) = nested.as_array() {
                        for reference in refs.iter().filter_map(Value::as_str) {
                            if !known.contains(reference) {
                                errors.push(format!(
                                    "{nested_path}: unknown provenance ref {reference:?}"
                                ));
                            }
                        }
                    }
                } else {
                    collect_unknown_provenance_refs(nested, &nested_path, known, errors);
                }
            }
        }
        Value::Array(items) => {
            for (index, nested) in items.iter().enumerate() {
                collect_unknown_provenance_refs(
                    nested,
                    &append_path(path, &index.to_string()),
                    known,
                    errors,
                );
            }
        }
        _ => {}
    }
}

fn append_path(path: &str, part: &str) -> String {
    if path.is_empty() {
        part.to_owned()
    } else {
        format!("{path}.{part}")
    }
}

fn sorted_strings(values: HashSet<String>) -> Vec<String> {
    let mut values = values.into_iter().collect::<Vec<_>>();
    values.sort();
    values
}
