use std::collections::BTreeMap;
use std::time::Instant;

use serde::Serialize;
use serde_json::{Map, Value};

use crate::harness::tool_registry::ToolRegistry;
use crate::harness::types::ToolStatus;
use crate::ir::glyph_ir::{
    GlyphIr, GlyphIrStep, GlyphRepairStep, GlyphToolStep, parse_glyph_to_ir,
};
use crate::ir::validate_ir::{GlyphIrValidationError, validate_ir};

use super::context::RuntimeContext;
use super::errors::GlyphRuntimeError;
use super::trace::TraceEvent;

#[derive(Debug, Clone, Serialize)]
pub struct GlyphVmRunResult {
    pub ir: GlyphIr,
    pub trace: Vec<TraceEvent>,
    pub outputs: Vec<Value>,
    pub variables: BTreeMap<String, Value>,
    #[serde(rename = "mockFS")]
    pub mock_fs: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default)]
pub struct GlyphVmOptions {
    pub mock_fs: BTreeMap<String, Value>,
}

#[derive(Debug, thiserror::Error)]
pub enum GlyphVmError {
    #[error(transparent)]
    Syntax(#[from] crate::language::errors::GlyphSyntaxError),
    #[error(transparent)]
    Validation(#[from] GlyphIrValidationError),
    #[error(transparent)]
    Runtime(#[from] GlyphRuntimeError),
}

pub struct GlyphVm {
    registry: ToolRegistry,
}

impl GlyphVm {
    pub fn new(registry: ToolRegistry) -> Self {
        Self { registry }
    }

    pub fn run_source(&self, source: &str) -> Result<GlyphVmRunResult, GlyphVmError> {
        let ir = validate_ir(parse_glyph_to_ir(source)?)?;
        self.execute(ir, GlyphVmOptions::default())
    }

    pub fn execute(
        &self,
        ir: GlyphIr,
        options: GlyphVmOptions,
    ) -> Result<GlyphVmRunResult, GlyphVmError> {
        let ir = validate_ir(ir)?;
        let mut ctx = RuntimeContext::new(ir.context.clone(), options.mock_fs);

        for flow in &ir.flows {
            for step in &flow.steps {
                self.execute_step(step, &mut ctx, None)?;
            }
        }

        Ok(GlyphVmRunResult {
            ir,
            trace: ctx.trace.all(),
            outputs: ctx.outputs.clone(),
            variables: ctx.variables.snapshot(),
            mock_fs: ctx.snapshot_fs(),
        })
    }

    fn execute_step(
        &self,
        step: &GlyphIrStep,
        ctx: &mut RuntimeContext,
        iteration: Option<usize>,
    ) -> Result<(), GlyphRuntimeError> {
        match step {
            GlyphIrStep::Tool(tool) => self.execute_tool_step(tool, ctx, iteration),
            GlyphIrStep::Repair(repair) => self.execute_repair_step(repair, ctx),
        }
    }

    fn execute_tool_step(
        &self,
        step: &GlyphToolStep,
        ctx: &mut RuntimeContext,
        iteration: Option<usize>,
    ) -> Result<(), GlyphRuntimeError> {
        let started = Instant::now();
        let mut resolved_args = Map::new();

        let result: Result<(), GlyphRuntimeError> = (|| {
            resolved_args = self.resolve_args(&step.args, ctx, &step.id)?;
            let tool = self.registry.get(&step.op).ok_or_else(|| {
                GlyphRuntimeError::new(
                    format!("Unknown tool \"{}\"", step.op),
                    Some(step.id.clone()),
                )
            })?;
            let result = tool(resolved_args.clone(), ctx)?;

            if let Some(assign_to) = &step.assign_to {
                ctx.variables.set(assign_to, result.value.clone());
            }

            if step.op == "EXPORT" {
                ctx.outputs.push(result.value.clone());
            }

            ctx.trace.add(TraceEvent {
                step_id: step.id.clone(),
                operation: step.op.clone(),
                resolved_args: resolved_args.clone(),
                output_summary: result.summary.clone(),
                status: result.status,
                duration_ms: started.elapsed().as_millis(),
                errors: if result.status == ToolStatus::Fail {
                    result.warnings.clone()
                } else {
                    vec![]
                },
                iteration,
            });

            Ok(())
        })();

        if let Err(error) = result {
            ctx.trace.add(TraceEvent {
                step_id: step.id.clone(),
                operation: step.op.clone(),
                resolved_args,
                output_summary: "Step failed".to_string(),
                status: ToolStatus::Fail,
                duration_ms: started.elapsed().as_millis(),
                errors: vec![error.to_string()],
                iteration,
            });
            return Err(error);
        }

        Ok(())
    }

    fn execute_repair_step(
        &self,
        step: &GlyphRepairStep,
        ctx: &mut RuntimeContext,
    ) -> Result<(), GlyphRuntimeError> {
        let started = Instant::now();

        let result: Result<(), GlyphRuntimeError> = (|| {
            self.require_variable(&step.target_var, ctx, &step.id)?;
            self.require_variable(&step.report_var, ctx, &step.id)?;

            let mut iterations = 0usize;
            for index in 0..step.max_iterations {
                if self.report_status(ctx.variables.get(&step.report_var)) == ToolStatus::Pass {
                    break;
                }

                iterations = index + 1;
                for inner in &step.steps {
                    self.execute_step(inner, ctx, Some(iterations))?;
                }

                if self.report_status(ctx.variables.get(&step.report_var)) == ToolStatus::Pass {
                    break;
                }
            }

            let final_status = self.report_status(ctx.variables.get(&step.report_var));
            ctx.trace.add(TraceEvent {
                step_id: step.id.clone(),
                operation: "REPAIR".to_string(),
                resolved_args: Map::from_iter([
                    (
                        "target".to_string(),
                        ctx.variables
                            .get(&step.target_var)
                            .cloned()
                            .unwrap_or(Value::Null),
                    ),
                    (
                        "report".to_string(),
                        ctx.variables
                            .get(&step.report_var)
                            .cloned()
                            .unwrap_or(Value::Null),
                    ),
                    ("max".to_string(), Value::from(step.max_iterations)),
                ]),
                output_summary: format!(
                    "Repair loop completed after {iterations} iteration{}",
                    if iterations == 1 { "" } else { "s" }
                ),
                status: final_status,
                duration_ms: started.elapsed().as_millis(),
                errors: vec![],
                iteration: None,
            });

            Ok(())
        })();

        if let Err(error) = result {
            ctx.trace.add(TraceEvent {
                step_id: step.id.clone(),
                operation: "REPAIR".to_string(),
                resolved_args: Map::from_iter([
                    (
                        "targetVar".to_string(),
                        Value::String(step.target_var.clone()),
                    ),
                    (
                        "reportVar".to_string(),
                        Value::String(step.report_var.clone()),
                    ),
                    ("max".to_string(), Value::from(step.max_iterations)),
                ]),
                output_summary: "Repair loop failed".to_string(),
                status: ToolStatus::Fail,
                duration_ms: started.elapsed().as_millis(),
                errors: vec![error.to_string()],
                iteration: None,
            });
            return Err(error);
        }

        Ok(())
    }

    fn resolve_args(
        &self,
        args: &Map<String, Value>,
        ctx: &RuntimeContext,
        step_id: &str,
    ) -> Result<Map<String, Value>, GlyphRuntimeError> {
        args.iter()
            .map(|(key, value)| Ok((key.clone(), self.resolve_value(value, ctx, step_id)?)))
            .collect()
    }

    fn resolve_value(
        &self,
        value: &Value,
        ctx: &RuntimeContext,
        step_id: &str,
    ) -> Result<Value, GlyphRuntimeError> {
        match value {
            Value::Array(items) => items
                .iter()
                .map(|item| self.resolve_value(item, ctx, step_id))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::Array),
            Value::Object(object) => {
                if object.len() == 1
                    && let Some(Value::String(name)) = object.get("var")
                {
                    return self.require_variable(name, ctx, step_id).cloned();
                }

                if object.len() == 1
                    && let Some(Value::String(path)) = object.get("ctx")
                {
                    return self.resolve_context(path, ctx, step_id);
                }

                object
                    .iter()
                    .map(|(key, nested)| {
                        Ok((key.clone(), self.resolve_value(nested, ctx, step_id)?))
                    })
                    .collect::<Result<Map<_, _>, _>>()
                    .map(Value::Object)
            }
            _ => Ok(value.clone()),
        }
    }

    fn require_variable<'a>(
        &self,
        name: &str,
        ctx: &'a RuntimeContext,
        step_id: &str,
    ) -> Result<&'a Value, GlyphRuntimeError> {
        ctx.variables.get(name).ok_or_else(|| {
            GlyphRuntimeError::new(
                format!("Unknown variable \"{name}\""),
                Some(step_id.to_string()),
            )
        })
    }

    fn resolve_context(
        &self,
        path: &str,
        ctx: &RuntimeContext,
        step_id: &str,
    ) -> Result<Value, GlyphRuntimeError> {
        let mut current = Value::Object(ctx.context.clone());

        for part in path.split('.') {
            match current {
                Value::Object(object) => {
                    current = object.get(part).cloned().ok_or_else(|| {
                        GlyphRuntimeError::new(
                            format!("Unknown ctx reference \"ctx.{path}\""),
                            Some(step_id.to_string()),
                        )
                    })?;
                }
                _ => {
                    return Err(GlyphRuntimeError::new(
                        format!("Unknown ctx reference \"ctx.{path}\""),
                        Some(step_id.to_string()),
                    ));
                }
            }
        }

        Ok(current)
    }

    fn report_status(&self, value: Option<&Value>) -> ToolStatus {
        value
            .and_then(|value| value.get("status"))
            .and_then(Value::as_str)
            .map(|status| match status {
                "pass" => ToolStatus::Pass,
                "warning" => ToolStatus::Warning,
                "fail" => ToolStatus::Fail,
                _ => ToolStatus::Warning,
            })
            .unwrap_or(ToolStatus::Warning)
    }
}
