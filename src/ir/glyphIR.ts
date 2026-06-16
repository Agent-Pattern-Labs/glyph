import type {
  CallArgAst,
  ContextAst,
  ExpressionAst,
  ObjectEntryAst,
  ProgramAst,
  StatementAst,
  ToolCallAst
} from "../language/ast.js";
import { parseGlyph } from "../language/parser.js";

export const GLYPH_IR_VERSION = "0.1" as const;

export type GlyphPrimitiveValue = string | number | boolean | null;

export type GlyphIRValue =
  | GlyphPrimitiveValue
  | GlyphIRValue[]
  | { [key: string]: GlyphIRValue }
  | { var: string }
  | { ctx: string };

export interface GlyphIR {
  version: typeof GLYPH_IR_VERSION;
  goal?: string;
  context: Record<string, GlyphPrimitiveValue | GlyphIRValue[] | { [key: string]: GlyphIRValue }>;
  flows: GlyphIRFlow[];
}

export interface GlyphIRFlow {
  name: string;
  steps: GlyphIRStep[];
}

export type GlyphIRStep = GlyphToolStep | GlyphRepairStep;

export interface GlyphToolStep {
  kind: "tool";
  id: string;
  op: string;
  args: Record<string, GlyphIRValue>;
  assignTo?: string;
}

export interface GlyphRepairStep {
  kind: "repair";
  id: string;
  targetVar: string;
  reportVar: string;
  maxIterations: number;
  steps: GlyphIRStep[];
}

const POSITIONAL_ARG_NAMES: Record<string, string[]> = {
  SPEC: ["input"],
  PLAN: ["input"],
  GEN: ["input"],
  CHECK: ["target"],
  FIX: ["target", "report"],
  PATCH: ["target", "instructions"],
  SUM: ["target"],
  SUMMARIZE: ["target"],
  ASK: ["question", "options"],
  EXPORT: ["target", "format"],
  RUN: ["command", "target"],
  READ: ["path"],
  WRITE: ["path", "content"]
};

export function parseGlyphToIR(source: string): GlyphIR {
  return compileAstToIR(parseGlyph(source));
}

export function compileAstToIR(ast: ProgramAst): GlyphIR {
  let stepCounter = 0;
  const nextStepId = () => `step_${++stepCounter}`;

  const compileStep = (step: StatementAst): GlyphIRStep => {
    if (step.kind === "ToolCall") {
      return compileToolStep(step, nextStepId());
    }

    return {
      kind: "repair",
      id: nextStepId(),
      targetVar: step.target,
      reportVar: step.report,
      maxIterations: step.max,
      steps: step.steps.map(compileStep)
    };
  };

  return {
    version: GLYPH_IR_VERSION,
    goal: ast.goal,
    context: ast.context ? compileContext(ast.context) : {},
    flows: ast.flows.map((flow) => ({
      name: flow.name,
      steps: flow.steps.map(compileStep)
    }))
  };
}

function compileToolStep(step: ToolCallAst, id: string): GlyphToolStep {
  return {
    kind: "tool",
    id,
    op: step.op.toUpperCase(),
    args: compileCallArgs(step.op.toUpperCase(), step.args),
    assignTo: step.assignTo
  };
}

function compileCallArgs(op: string, args: CallArgAst[]): Record<string, GlyphIRValue> {
  const record: Record<string, GlyphIRValue> = {};
  const positionalNames = POSITIONAL_ARG_NAMES[op] ?? ["input"];
  let positionalIndex = 0;

  for (const arg of args) {
    const name = arg.name ?? positionalNames[positionalIndex] ?? `arg${positionalIndex + 1}`;
    positionalIndex += arg.name ? 0 : 1;

    if (record[name] !== undefined) {
      throw new Error(`Duplicate argument "${name}" for ${op}`);
    }

    record[name] = expressionToIRValue(arg.value);
  }

  return record;
}

function compileContext(context: ContextAst): GlyphIR["context"] {
  return Object.fromEntries(context.entries.map((entry) => [entry.key, expressionToJsonLiteral(entry.value)]));
}

function expressionToJsonLiteral(expression: ExpressionAst): GlyphIR["context"][string] {
  switch (expression.kind) {
    case "StringLiteral":
    case "NumberLiteral":
    case "BooleanLiteral":
      return expression.value;
    case "ArrayLiteral":
      return expression.items.map(expressionToJsonLiteral);
    case "ObjectLiteral":
      return Object.fromEntries(expression.entries.map((entry) => [entry.key, expressionToJsonLiteral(entry.value)]));
    case "VarRef":
    case "CtxRef":
      throw new Error("Context declarations must use literal JSON-compatible values");
  }
}

function expressionToIRValue(expression: ExpressionAst): GlyphIRValue {
  switch (expression.kind) {
    case "StringLiteral":
    case "NumberLiteral":
    case "BooleanLiteral":
      return expression.value;
    case "ArrayLiteral":
      return expression.items.map(expressionToIRValue);
    case "ObjectLiteral":
      return objectEntriesToIR(expression.entries);
    case "VarRef":
      return { var: expression.name };
    case "CtxRef":
      return { ctx: expression.path.join(".") };
  }
}

function objectEntriesToIR(entries: ObjectEntryAst[]): Record<string, GlyphIRValue> {
  const result: Record<string, GlyphIRValue> = {};

  for (const entry of entries) {
    if (result[entry.key] !== undefined) {
      throw new Error(`Duplicate object key "${entry.key}"`);
    }
    result[entry.key] = expressionToIRValue(entry.value);
  }

  return result;
}
