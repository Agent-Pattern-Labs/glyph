use serde_json::{Value, json};

pub fn training_records(capsules: &[Value]) -> Vec<Value> {
    capsules.iter().flat_map(records_for_capsule).collect()
}

pub fn records_for_capsule(capsule: &Value) -> Vec<Value> {
    let capsule_id = capsule["id"].as_str().unwrap_or("unknown");
    let surface = &capsule["surface"];
    let form = surface["form"].as_str().unwrap_or("unknown");
    let language = surface["language"].as_str().unwrap_or("unknown");

    vec![
        json!({
            "id": format!("{capsule_id}::text_to_capsule"),
            "task": "text_to_capsule",
            "input": {
                "form": form,
                "language": language,
                "instruction": "Represent this lexical item as an etymonoetic semantic capsule."
            },
            "output": capsule
        }),
        json!({
            "id": format!("{capsule_id}::capsule_to_expansion"),
            "task": "capsule_to_expansion",
            "input": {
                "capsule": capsule,
                "instruction": "Expand this etymonoetic semantic capsule into an explainable paragraph."
            },
            "output": {
                "paragraph": capsule["expansion"]["paragraph"].clone(),
                "trace": capsule["expansion"]["trace"].clone()
            }
        }),
    ]
}
