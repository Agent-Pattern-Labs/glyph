import type { CallArgAst, ExpressionAst, FlowAst, ObjectEntryAst, ProgramAst, StatementAst } from "./ast.js";
import { parseGlyph } from "./parser.js";

const INDENT = "  ";

export function formatGlyph(source: string): string {
  return `${formatProgram(parseGlyph(source))}\n`;
}

export function formatProgram(program: ProgramAst): string {
  const chunks: string[] = [];

  if (program.goal) {
    chunks.push(`goal ${quote(program.goal)}`);
  }

  if (program.context) {
    chunks.push(["ctx {", ...program.context.entries.map((entry) => `${INDENT}${formatKey(entry.key)}: ${formatExpression(entry.value)}`), "}"].join("\n"));
  }

  for (const flow of program.flows) {
    chunks.push(formatFlow(flow));
  }

  return chunks.join("\n\n");
}

function formatFlow(flow: FlowAst): string {
  return [`flow ${flow.name} {`, ...flow.steps.flatMap((step) => formatStatement(step, 1)), "}"].join("\n");
}

function formatStatement(step: StatementAst, depth: number): string[] {
  const prefix = INDENT.repeat(depth);

  if (step.kind === "ToolCall") {
    const assign = step.assignTo ? ` -> ${step.assignTo}` : "";
    return [`${prefix}${step.op.toUpperCase()}(${step.args.map(formatArg).join(", ")})${assign}`];
  }

  return [
    `${prefix}repair ${step.target} with ${step.report} max ${step.max} {`,
    ...step.steps.flatMap((inner) => formatStatement(inner, depth + 1)),
    `${prefix}}`
  ];
}

function formatArg(arg: CallArgAst): string {
  const value = formatExpression(arg.value);
  return arg.name ? `${arg.name}=${value}` : value;
}

function formatExpression(expression: ExpressionAst): string {
  switch (expression.kind) {
    case "StringLiteral":
      return quote(expression.value);
    case "NumberLiteral":
      return String(expression.value);
    case "BooleanLiteral":
      return String(expression.value);
    case "ArrayLiteral":
      return `[${expression.items.map(formatExpression).join(", ")}]`;
    case "ObjectLiteral":
      return `{ ${expression.entries.map(formatObjectEntry).join(", ")} }`;
    case "VarRef":
      return expression.name;
    case "CtxRef":
      return `ctx.${expression.path.join(".")}`;
  }
}

function formatObjectEntry(entry: ObjectEntryAst): string {
  return `${formatKey(entry.key)}: ${formatExpression(entry.value)}`;
}

function formatKey(key: string): string {
  return /^[A-Za-z_][A-Za-z0-9_]*$/.test(key) ? key : quote(key);
}

function quote(value: string): string {
  return JSON.stringify(value);
}
