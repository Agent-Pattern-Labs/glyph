use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::language::ast::{
    CallArgAst, ContextAst, ExpressionAst, ObjectEntryAst, ProgramAst, StatementAst, ToolCallAst,
};
use crate::language::errors::GlyphSyntaxError;
use crate::language::parser::parse_glyph;

pub const GLYPH_IR_VERSION: &str = "0.1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlyphIr {
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    pub context: Map<String, Value>,
    pub flows: Vec<GlyphIrFlow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlyphIrFlow {
    pub name: String,
    pub steps: Vec<GlyphIrStep>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum GlyphIrStep {
    #[serde(rename = "tool")]
    Tool(GlyphToolStep),
    #[serde(rename = "repair")]
    Repair(GlyphRepairStep),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlyphToolStep {
    pub id: String,
    pub op: String,
    pub args: Map<String, Value>,
    #[serde(rename = "assignTo", skip_serializing_if = "Option::is_none")]
    pub assign_to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlyphRepairStep {
    pub id: String,
    #[serde(rename = "targetVar")]
    pub target_var: String,
    #[serde(rename = "reportVar")]
    pub report_var: String,
    #[serde(rename = "maxIterations")]
    pub max_iterations: usize,
    pub steps: Vec<GlyphIrStep>,
}

pub fn parse_glyph_to_ir(source: &str) -> Result<GlyphIr, GlyphSyntaxError> {
    Ok(compile_ast_to_ir(&parse_glyph(source)?))
}

pub fn compile_ast_to_ir(ast: &ProgramAst) -> GlyphIr {
    let mut counter = 0usize;
    let mut next_id = || {
        counter += 1;
        format!("step_{counter}")
    };

    GlyphIr {
        version: GLYPH_IR_VERSION.to_string(),
        goal: ast.goal.clone(),
        context: ast
            .context
            .as_ref()
            .map(compile_context)
            .unwrap_or_default(),
        flows: ast
            .flows
            .iter()
            .map(|flow| GlyphIrFlow {
                name: flow.name.clone(),
                steps: flow
                    .steps
                    .iter()
                    .map(|step| compile_step(step, &mut next_id))
                    .collect(),
            })
            .collect(),
    }
}

fn compile_step(step: &StatementAst, next_id: &mut impl FnMut() -> String) -> GlyphIrStep {
    match step {
        StatementAst::ToolCall(call) => GlyphIrStep::Tool(compile_tool_step(call, next_id())),
        StatementAst::RepairBlock(block) => GlyphIrStep::Repair(GlyphRepairStep {
            id: next_id(),
            target_var: block.target.clone(),
            report_var: block.report.clone(),
            max_iterations: block.max,
            steps: block
                .steps
                .iter()
                .map(|inner| compile_step(inner, next_id))
                .collect(),
        }),
    }
}

fn compile_tool_step(step: &ToolCallAst, id: String) -> GlyphToolStep {
    let op = step.op.to_uppercase();
    GlyphToolStep {
        id,
        op: op.clone(),
        args: compile_call_args(&op, &step.args),
        assign_to: step.assign_to.clone(),
    }
}

fn compile_call_args(op: &str, args: &[CallArgAst]) -> Map<String, Value> {
    let mut record = Map::new();
    let positional_names = positional_arg_names(op);
    let mut positional_index = 0usize;

    for arg in args {
        let name = match &arg.name {
            Some(name) => name.clone(),
            None => {
                let name = positional_names
                    .get(positional_index)
                    .copied()
                    .unwrap_or("input")
                    .to_string();
                positional_index += 1;
                name
            }
        };

        record.insert(name, expression_to_ir_value(&arg.value));
    }

    record
}

fn compile_context(context: &ContextAst) -> Map<String, Value> {
    context
        .entries
        .iter()
        .map(|entry| (entry.key.clone(), expression_to_json_literal(&entry.value)))
        .collect()
}

fn expression_to_json_literal(expression: &ExpressionAst) -> Value {
    match expression {
        ExpressionAst::String(value) => Value::String(value.clone()),
        ExpressionAst::Number(value) => Value::Number(value.clone()),
        ExpressionAst::Boolean(value) => Value::Bool(*value),
        ExpressionAst::Array(items) => {
            Value::Array(items.iter().map(expression_to_json_literal).collect())
        }
        ExpressionAst::Object(entries) => object_entries_to_json(entries),
        ExpressionAst::VarRef(_) | ExpressionAst::CtxRef(_) => {
            panic!("context declarations must use literal JSON-compatible values")
        }
    }
}

fn expression_to_ir_value(expression: &ExpressionAst) -> Value {
    match expression {
        ExpressionAst::String(value) => Value::String(value.clone()),
        ExpressionAst::Number(value) => Value::Number(value.clone()),
        ExpressionAst::Boolean(value) => Value::Bool(*value),
        ExpressionAst::Array(items) => {
            Value::Array(items.iter().map(expression_to_ir_value).collect())
        }
        ExpressionAst::Object(entries) => object_entries_to_ir(entries),
        ExpressionAst::VarRef(name) => json!({ "var": name }),
        ExpressionAst::CtxRef(path) => json!({ "ctx": path.join(".") }),
    }
}

fn object_entries_to_json(entries: &[ObjectEntryAst]) -> Value {
    Value::Object(
        entries
            .iter()
            .map(|entry| (entry.key.clone(), expression_to_json_literal(&entry.value)))
            .collect(),
    )
}

fn object_entries_to_ir(entries: &[ObjectEntryAst]) -> Value {
    Value::Object(
        entries
            .iter()
            .map(|entry| (entry.key.clone(), expression_to_ir_value(&entry.value)))
            .collect(),
    )
}

fn positional_arg_names(op: &str) -> &'static [&'static str] {
    match op {
        "SPEC" => &["input"],
        "PLAN" => &["input"],
        "GEN" => &["input"],
        "CHECK" => &["target"],
        "FIX" => &["target", "report"],
        "PATCH" => &["target", "instructions"],
        "SUM" | "SUMMARIZE" => &["target"],
        "ASK" => &["question", "options"],
        "EXPORT" => &["target", "format"],
        "RUN" => &["command", "target"],
        "READ" => &["path"],
        "WRITE" => &["path", "content"],
        _ => &["input"],
    }
}
