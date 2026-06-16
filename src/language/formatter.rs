use super::ast::{CallArgAst, ExpressionAst, FlowAst, ObjectEntryAst, ProgramAst, StatementAst};
use super::errors::GlyphSyntaxError;
use super::parser::parse_glyph;

const INDENT: &str = "  ";

pub fn format_glyph(source: &str) -> Result<String, GlyphSyntaxError> {
    Ok(format!("{}\n", format_program(&parse_glyph(source)?)))
}

pub fn format_program(program: &ProgramAst) -> String {
    let mut chunks = Vec::new();

    if let Some(goal) = &program.goal {
        chunks.push(format!("goal {}", quote(goal)));
    }

    if let Some(context) = &program.context {
        let mut lines = vec!["ctx {".to_string()];
        for entry in &context.entries {
            lines.push(format!(
                "{INDENT}{}: {}",
                format_key(&entry.key),
                format_expression(&entry.value)
            ));
        }
        lines.push("}".to_string());
        chunks.push(lines.join("\n"));
    }

    for flow in &program.flows {
        chunks.push(format_flow(flow));
    }

    chunks.join("\n\n")
}

fn format_flow(flow: &FlowAst) -> String {
    let mut lines = vec![format!("flow {} {{", flow.name)];
    for step in &flow.steps {
        lines.extend(format_statement(step, 1));
    }
    lines.push("}".to_string());
    lines.join("\n")
}

fn format_statement(step: &StatementAst, depth: usize) -> Vec<String> {
    let prefix = INDENT.repeat(depth);
    match step {
        StatementAst::ToolCall(call) => {
            let assign = call
                .assign_to
                .as_ref()
                .map(|name| format!(" -> {name}"))
                .unwrap_or_default();
            vec![format!(
                "{prefix}{}({}){assign}",
                call.op.to_uppercase(),
                call.args
                    .iter()
                    .map(format_arg)
                    .collect::<Vec<_>>()
                    .join(", ")
            )]
        }
        StatementAst::RepairBlock(block) => {
            let mut lines = vec![format!(
                "{prefix}repair {} with {} max {} {{",
                block.target, block.report, block.max
            )];
            for inner in &block.steps {
                lines.extend(format_statement(inner, depth + 1));
            }
            lines.push(format!("{prefix}}}"));
            lines
        }
    }
}

fn format_arg(arg: &CallArgAst) -> String {
    let value = format_expression(&arg.value);
    arg.name
        .as_ref()
        .map(|name| format!("{name}={value}"))
        .unwrap_or(value)
}

fn format_expression(expression: &ExpressionAst) -> String {
    match expression {
        ExpressionAst::String(value) => quote(value),
        ExpressionAst::Number(value) => value.to_string(),
        ExpressionAst::Boolean(value) => value.to_string(),
        ExpressionAst::Array(items) => format!(
            "[{}]",
            items
                .iter()
                .map(format_expression)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ExpressionAst::Object(entries) => format!(
            "{{ {} }}",
            entries
                .iter()
                .map(format_object_entry)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ExpressionAst::VarRef(name) => name.clone(),
        ExpressionAst::CtxRef(path) => format!("ctx.{}", path.join(".")),
    }
}

fn format_object_entry(entry: &ObjectEntryAst) -> String {
    format!(
        "{}: {}",
        format_key(&entry.key),
        format_expression(&entry.value)
    )
}

fn format_key(key: &str) -> String {
    if is_identifier(key) {
        key.to_string()
    } else {
        quote(key)
    }
}

fn quote(value: &str) -> String {
    serde_json::to_string(value).expect("string serialization cannot fail")
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}
