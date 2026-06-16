use regex::Regex;
use thiserror::Error;

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

        for step in &flow.steps {
            validate_step(step, &identifier, &step_id, &op)?;
        }
    }

    Ok(ir)
}

fn validate_step(
    step: &GlyphIrStep,
    identifier: &Regex,
    step_id: &Regex,
    op_regex: &Regex,
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

            if let Some(assign_to) = &tool.assign_to
                && !identifier.is_match(assign_to)
            {
                return Err(GlyphIrValidationError(format!(
                    "Invalid assignment target {:?}",
                    assign_to
                )));
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

            for inner in &repair.steps {
                validate_step(inner, identifier, step_id, op_regex)?;
            }
        }
    }

    Ok(())
}
