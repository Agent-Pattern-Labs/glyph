use std::collections::BTreeSet;

use regex::Regex;
use serde_json::{Map, Value};
use thiserror::Error;

use crate::language::grammar::GLYPH_PRIMITIVES;

use super::glyph_ir::{GLYPH_IR_VERSION, GlyphIr, GlyphIrStep};

const MAX_REPAIR_ITERATIONS: usize = 10;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("{0}")]
pub struct GlyphIrValidationError(pub String);

pub fn validate_ir(ir: GlyphIr) -> Result<GlyphIr, GlyphIrValidationError> {
    let identifier = Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$").expect("valid identifier regex");
    let step_id = Regex::new(r"^step_[0-9]+$").expect("valid step id regex");
    let op = Regex::new(r"^[A-Z][A-Z0-9_]*$").expect("valid op regex");

    if ir.version != GLYPH_IR_VERSION {
        return Err(GlyphIrValidationError(format!(
            "Invalid IR version {:?}",
            ir.version
        )));
    }

    if ir.flows.is_empty() {
        return Err(GlyphIrValidationError(
            "IR must contain at least one flow".to_string(),
        ));
    }

    for flow in &ir.flows {
        if !identifier.is_match(&flow.name) {
            return Err(GlyphIrValidationError(format!(
                "Invalid flow name {:?}",
                flow.name
            )));
        }

        if flow.steps.is_empty() {
            return Err(GlyphIrValidationError(format!(
                "Flow {:?} must contain at least one step",
                flow.name
            )));
        }

        let mut variables = BTreeSet::new();
        let mut step_ids = BTreeSet::new();
        let mut has_top_level_export = false;
        for step in &flow.steps {
            if is_top_level_export(step) {
                has_top_level_export = true;
            }

            validate_step(
                step,
                &identifier,
                &step_id,
                &op,
                &ir.context,
                &mut variables,
                &mut step_ids,
            )?;
        }

        if !has_top_level_export {
            return Err(GlyphIrValidationError(format!(
                "Flow {:?} must contain a top-level EXPORT step",
                flow.name
            )));
        }
    }

    Ok(ir)
}

fn is_top_level_export(step: &GlyphIrStep) -> bool {
    matches!(step, GlyphIrStep::Tool(tool) if tool.op == "EXPORT")
}

fn validate_step(
    step: &GlyphIrStep,
    identifier: &Regex,
    step_id: &Regex,
    op_regex: &Regex,
    context: &Map<String, Value>,
    variables: &mut BTreeSet<String>,
    step_ids: &mut BTreeSet<String>,
) -> Result<(), GlyphIrValidationError> {
    match step {
        GlyphIrStep::Tool(tool) => {
            validate_step_id(&tool.id, step_id, step_ids)?;

            if !op_regex.is_match(&tool.op) {
                return Err(GlyphIrValidationError(format!(
                    "Invalid operation {:?}",
                    tool.op
                )));
            }

            if !GLYPH_PRIMITIVES.contains(&tool.op.as_str()) {
                return Err(GlyphIrValidationError(format!(
                    "Unknown tool {:?}",
                    tool.op
                )));
            }

            for value in tool.args.values() {
                validate_value_refs(value, context, variables, identifier, &tool.id)?;
            }

            if let Some(assign_to) = &tool.assign_to {
                if !identifier.is_match(assign_to) {
                    return Err(GlyphIrValidationError(format!(
                        "Invalid assignment target {:?}",
                        assign_to
                    )));
                }
                variables.insert(assign_to.clone());
            }
        }
        GlyphIrStep::Repair(repair) => {
            validate_step_id(&repair.id, step_id, step_ids)?;

            if !identifier.is_match(&repair.target_var) {
                return Err(GlyphIrValidationError(format!(
                    "Invalid repair target {:?}",
                    repair.target_var
                )));
            }

            if !identifier.is_match(&repair.report_var) {
                return Err(GlyphIrValidationError(format!(
                    "Invalid repair report {:?}",
                    repair.report_var
                )));
            }

            if !variables.contains(&repair.target_var) {
                return Err(GlyphIrValidationError(format!(
                    "Unknown repair target variable {:?} at {}",
                    repair.target_var, repair.id
                )));
            }

            if !variables.contains(&repair.report_var) {
                return Err(GlyphIrValidationError(format!(
                    "Unknown repair report variable {:?} at {}",
                    repair.report_var, repair.id
                )));
            }

            if repair.max_iterations == 0 || repair.max_iterations > MAX_REPAIR_ITERATIONS {
                return Err(GlyphIrValidationError(format!(
                    "Repair maxIterations at {} must be between 1 and {}",
                    repair.id, MAX_REPAIR_ITERATIONS
                )));
            }

            if repair.steps.is_empty() {
                return Err(GlyphIrValidationError(format!(
                    "Repair block at {} must contain at least one step",
                    repair.id
                )));
            }

            if !steps_assign_to(&repair.steps, &repair.target_var) {
                return Err(GlyphIrValidationError(format!(
                    "Repair block at {} must assign target variable {:?}",
                    repair.id, repair.target_var
                )));
            }

            if !steps_assign_to(&repair.steps, &repair.report_var) {
                return Err(GlyphIrValidationError(format!(
                    "Repair block at {} must assign report variable {:?}",
                    repair.id, repair.report_var
                )));
            }

            for inner in &repair.steps {
                validate_step(
                    inner, identifier, step_id, op_regex, context, variables, step_ids,
                )?;
            }
        }
    }

    Ok(())
}

fn validate_step_id(
    id: &str,
    step_id: &Regex,
    step_ids: &mut BTreeSet<String>,
) -> Result<(), GlyphIrValidationError> {
    if !step_id.is_match(id) {
        return Err(GlyphIrValidationError(format!("Invalid step id {id:?}")));
    }

    if !step_ids.insert(id.to_string()) {
        return Err(GlyphIrValidationError(format!("Duplicate step id {id:?}")));
    }

    Ok(())
}

fn steps_assign_to(steps: &[GlyphIrStep], variable: &str) -> bool {
    steps.iter().any(|step| match step {
        GlyphIrStep::Tool(tool) => tool.assign_to.as_deref() == Some(variable),
        GlyphIrStep::Repair(repair) => steps_assign_to(&repair.steps, variable),
    })
}

fn validate_value_refs(
    value: &Value,
    context: &Map<String, Value>,
    variables: &BTreeSet<String>,
    identifier: &Regex,
    step_id: &str,
) -> Result<(), GlyphIrValidationError> {
    match value {
        Value::Array(items) => {
            for item in items {
                validate_value_refs(item, context, variables, identifier, step_id)?;
            }
        }
        Value::Object(object) => {
            if object.len() == 1 && object.contains_key("var") {
                let Some(Value::String(name)) = object.get("var") else {
                    return Err(GlyphIrValidationError(format!(
                        "Invalid variable reference at {step_id}"
                    )));
                };

                if !identifier.is_match(name) {
                    return Err(GlyphIrValidationError(format!(
                        "Invalid variable reference {:?} at {}",
                        name, step_id
                    )));
                }

                if !variables.contains(name) {
                    return Err(GlyphIrValidationError(format!(
                        "Unknown variable {:?} at {}",
                        name, step_id
                    )));
                }
                return Ok(());
            }

            if object.len() == 1 && object.contains_key("ctx") {
                let Some(Value::String(path)) = object.get("ctx") else {
                    return Err(GlyphIrValidationError(format!(
                        "Invalid ctx reference at {step_id}"
                    )));
                };

                if !valid_context_path(path, identifier) {
                    return Err(GlyphIrValidationError(format!(
                        "Invalid ctx reference \"ctx.{path}\" at {step_id}"
                    )));
                }

                if resolve_context_path(path, context).is_none() {
                    return Err(GlyphIrValidationError(format!(
                        "Unknown ctx reference \"ctx.{path}\" at {step_id}"
                    )));
                }
                return Ok(());
            }

            for nested in object.values() {
                validate_value_refs(nested, context, variables, identifier, step_id)?;
            }
        }
        _ => {}
    }

    Ok(())
}

fn valid_context_path(path: &str, identifier: &Regex) -> bool {
    !path.is_empty() && path.split('.').all(|part| identifier.is_match(part))
}

fn resolve_context_path<'a>(path: &str, context: &'a Map<String, Value>) -> Option<&'a Value> {
    let mut parts = path.split('.');
    let first = parts.next()?;
    let mut current = context.get(first)?;

    for part in parts {
        current = current.as_object()?.get(part)?;
    }

    Some(current)
}
