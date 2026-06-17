use std::collections::BTreeSet;

use regex::Regex;
use serde_json::{Map, Value};
use thiserror::Error;

use crate::language::grammar::GLYPH_PRIMITIVES;

use super::glyph_ir::{GLYPH_IR_VERSION, GlyphIr, GlyphIrStep};

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

        let mut variables = BTreeSet::new();
        for step in &flow.steps {
            validate_step(
                step,
                &identifier,
                &step_id,
                &op,
                &ir.context,
                &mut variables,
            )?;
        }
    }

    Ok(ir)
}

fn validate_step(
    step: &GlyphIrStep,
    identifier: &Regex,
    step_id: &Regex,
    op_regex: &Regex,
    context: &Map<String, Value>,
    variables: &mut BTreeSet<String>,
) -> Result<(), GlyphIrValidationError> {
    match step {
        GlyphIrStep::Tool(tool) => {
            if !step_id.is_match(&tool.id) {
                return Err(GlyphIrValidationError(format!(
                    "Invalid step id {:?}",
                    tool.id
                )));
            }

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
                validate_value_refs(value, context, variables, &tool.id)?;
            }

            if let Some(assign_to) = &tool.assign_to
                && !identifier.is_match(assign_to)
            {
                return Err(GlyphIrValidationError(format!(
                    "Invalid assignment target {:?}",
                    assign_to
                )));
            }

            if let Some(assign_to) = &tool.assign_to {
                variables.insert(assign_to.clone());
            }
        }
        GlyphIrStep::Repair(repair) => {
            if !step_id.is_match(&repair.id) {
                return Err(GlyphIrValidationError(format!(
                    "Invalid step id {:?}",
                    repair.id
                )));
            }

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

            for inner in &repair.steps {
                validate_step(inner, identifier, step_id, op_regex, context, variables)?;
            }
        }
    }

    Ok(())
}

fn validate_value_refs(
    value: &Value,
    context: &Map<String, Value>,
    variables: &BTreeSet<String>,
    step_id: &str,
) -> Result<(), GlyphIrValidationError> {
    match value {
        Value::Array(items) => {
            for item in items {
                validate_value_refs(item, context, variables, step_id)?;
            }
        }
        Value::Object(object) => {
            if object.len() == 1
                && let Some(Value::String(name)) = object.get("var")
            {
                if !variables.contains(name) {
                    return Err(GlyphIrValidationError(format!(
                        "Unknown variable {:?} at {}",
                        name, step_id
                    )));
                }
                return Ok(());
            }

            if object.len() == 1
                && let Some(Value::String(path)) = object.get("ctx")
            {
                if resolve_context_path(path, context).is_none() {
                    return Err(GlyphIrValidationError(format!(
                        "Unknown ctx reference \"ctx.{path}\" at {step_id}"
                    )));
                }
                return Ok(());
            }

            for nested in object.values() {
                validate_value_refs(nested, context, variables, step_id)?;
            }
        }
        _ => {}
    }

    Ok(())
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
