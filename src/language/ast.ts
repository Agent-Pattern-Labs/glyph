export interface ProgramAst {
  kind: "Program";
  goal?: string;
  context?: ContextAst;
  flows: FlowAst[];
}

export interface ContextAst {
  kind: "Context";
  entries: ObjectEntryAst[];
}

export interface FlowAst {
  kind: "Flow";
  name: string;
  steps: StatementAst[];
}

export type StatementAst = ToolCallAst | RepairBlockAst;

export interface ToolCallAst {
  kind: "ToolCall";
  op: string;
  args: CallArgAst[];
  assignTo?: string;
}

export interface RepairBlockAst {
  kind: "RepairBlock";
  target: string;
  report: string;
  max: number;
  steps: StatementAst[];
}

export interface CallArgAst {
  name?: string;
  value: ExpressionAst;
}

export type ExpressionAst =
  | StringLiteralAst
  | NumberLiteralAst
  | BooleanLiteralAst
  | ArrayLiteralAst
  | ObjectLiteralAst
  | VarRefAst
  | CtxRefAst;

export interface StringLiteralAst {
  kind: "StringLiteral";
  value: string;
}

export interface NumberLiteralAst {
  kind: "NumberLiteral";
  value: number;
}

export interface BooleanLiteralAst {
  kind: "BooleanLiteral";
  value: boolean;
}

export interface ArrayLiteralAst {
  kind: "ArrayLiteral";
  items: ExpressionAst[];
}

export interface ObjectLiteralAst {
  kind: "ObjectLiteral";
  entries: ObjectEntryAst[];
}

export interface ObjectEntryAst {
  key: string;
  value: ExpressionAst;
}

export interface VarRefAst {
  kind: "VarRef";
  name: string;
}

export interface CtxRefAst {
  kind: "CtxRef";
  path: string[];
}
