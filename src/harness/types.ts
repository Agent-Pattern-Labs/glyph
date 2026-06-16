import type { RuntimeContext } from "../runtime/context.js";

export type ToolStatus = "pass" | "warning" | "fail";

export interface ToolResult {
  status: ToolStatus;
  value: unknown;
  summary: string;
  warnings?: string[];
}

export type ToolHandler = (args: Record<string, unknown>, ctx: RuntimeContext) => Promise<ToolResult> | ToolResult;
