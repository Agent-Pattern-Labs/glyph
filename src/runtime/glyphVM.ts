import type { ToolStatus } from "../harness/types.js";
import type { ToolRegistry } from "../harness/toolRegistry.js";
import { parseGlyphToIR, type GlyphIR, type GlyphIRStep, type GlyphIRValue, type GlyphRepairStep, type GlyphToolStep } from "../ir/glyphIR.js";
import { validateIR } from "../ir/validateIR.js";
import { RuntimeContext } from "./context.js";
import { GlyphRuntimeError } from "./errors.js";
import type { TraceEvent } from "./trace.js";

export interface GlyphVMRunResult {
  ir: GlyphIR;
  trace: TraceEvent[];
  outputs: unknown[];
  variables: Record<string, unknown>;
  mockFS: Record<string, unknown>;
}

export interface GlyphVMOptions {
  mockFS?: Record<string, unknown>;
}

export class GlyphVM {
  constructor(private readonly registry: ToolRegistry) {}

  async runSource(source: string, options: GlyphVMOptions = {}): Promise<GlyphVMRunResult> {
    const ir = validateIR(parseGlyphToIR(source));
    return this.execute(ir, options);
  }

  async execute(ir: GlyphIR, options: GlyphVMOptions = {}): Promise<GlyphVMRunResult> {
    const validIR = validateIR(ir);
    const ctx = new RuntimeContext({ context: validIR.context, mockFS: options.mockFS });

    for (const flow of validIR.flows) {
      for (const step of flow.steps) {
        await this.executeStep(step, ctx);
      }
    }

    return {
      ir: validIR,
      trace: ctx.trace.all(),
      outputs: [...ctx.outputs],
      variables: ctx.variables.snapshot(),
      mockFS: ctx.snapshotFS()
    };
  }

  private async executeStep(step: GlyphIRStep, ctx: RuntimeContext, iteration?: number): Promise<void> {
    if (step.kind === "tool") {
      await this.executeToolStep(step, ctx, iteration);
      return;
    }

    await this.executeRepairStep(step, ctx);
  }

  private async executeToolStep(step: GlyphToolStep, ctx: RuntimeContext, iteration?: number): Promise<void> {
    const started = Date.now();
    let resolvedArgs: Record<string, unknown> = {};

    try {
      resolvedArgs = this.resolveArgs(step.args, ctx, step.id);
      const tool = this.registry.get(step.op);

      if (!tool) {
        throw new GlyphRuntimeError(`Unknown tool "${step.op}"`, step.id);
      }

      const result = await tool(resolvedArgs, ctx);

      if (step.assignTo) {
        ctx.variables.set(step.assignTo, result.value);
      }

      if (step.op === "EXPORT") {
        ctx.outputs.push(result.value);
      }

      ctx.trace.add({
        stepId: step.id,
        operation: step.op,
        resolvedArgs,
        outputSummary: result.summary,
        status: result.status,
        durationMs: Date.now() - started,
        errors: result.status === "fail" ? result.warnings : undefined,
        iteration
      });
    } catch (error) {
      ctx.trace.add({
        stepId: step.id,
        operation: step.op,
        resolvedArgs,
        outputSummary: "Step failed",
        status: "fail",
        durationMs: Date.now() - started,
        errors: [error instanceof Error ? error.message : String(error)],
        iteration
      });
      throw error;
    }
  }

  private async executeRepairStep(step: GlyphRepairStep, ctx: RuntimeContext): Promise<void> {
    const started = Date.now();

    try {
      this.requireVariable(step.targetVar, ctx, step.id);
      this.requireVariable(step.reportVar, ctx, step.id);

      let iterations = 0;
      for (let index = 0; index < step.maxIterations; index += 1) {
        if (this.reportStatus(ctx.variables.get(step.reportVar)) === "pass") {
          break;
        }

        iterations = index + 1;
        for (const innerStep of step.steps) {
          await this.executeStep(innerStep, ctx, iterations);
        }

        if (this.reportStatus(ctx.variables.get(step.reportVar)) === "pass") {
          break;
        }
      }

      const finalStatus = this.reportStatus(ctx.variables.get(step.reportVar));
      ctx.trace.add({
        stepId: step.id,
        operation: "REPAIR",
        resolvedArgs: {
          target: ctx.variables.get(step.targetVar),
          report: ctx.variables.get(step.reportVar),
          max: step.maxIterations
        },
        outputSummary: `Repair loop completed after ${iterations} iteration${iterations === 1 ? "" : "s"}`,
        status: finalStatus,
        durationMs: Date.now() - started
      });
    } catch (error) {
      ctx.trace.add({
        stepId: step.id,
        operation: "REPAIR",
        resolvedArgs: {
          targetVar: step.targetVar,
          reportVar: step.reportVar,
          max: step.maxIterations
        },
        outputSummary: "Repair loop failed",
        status: "fail",
        durationMs: Date.now() - started,
        errors: [error instanceof Error ? error.message : String(error)]
      });
      throw error;
    }
  }

  private resolveArgs(args: Record<string, GlyphIRValue>, ctx: RuntimeContext, stepId: string): Record<string, unknown> {
    return Object.fromEntries(Object.entries(args).map(([key, value]) => [key, this.resolveValue(value, ctx, stepId)]));
  }

  private resolveValue(value: GlyphIRValue, ctx: RuntimeContext, stepId: string): unknown {
    if (Array.isArray(value)) {
      return value.map((item) => this.resolveValue(item, ctx, stepId));
    }

    if (value !== null && typeof value === "object") {
      const keys = Object.keys(value);

      if (keys.length === 1 && "var" in value && typeof value.var === "string") {
        return this.requireVariable(value.var, ctx, stepId);
      }

      if (keys.length === 1 && "ctx" in value && typeof value.ctx === "string") {
        return this.resolveContext(value.ctx, ctx, stepId);
      }

      return Object.fromEntries(Object.entries(value).map(([key, nested]) => [key, this.resolveValue(nested, ctx, stepId)]));
    }

    return value;
  }

  private requireVariable(name: string, ctx: RuntimeContext, stepId: string): unknown {
    if (!ctx.variables.has(name)) {
      throw new GlyphRuntimeError(`Unknown variable "${name}"`, stepId);
    }

    return ctx.variables.get(name);
  }

  private resolveContext(path: string, ctx: RuntimeContext, stepId: string): unknown {
    const parts = path.split(".");
    let current: unknown = ctx.context;

    for (const part of parts) {
      if (current === null || typeof current !== "object" || !(part in current)) {
        throw new GlyphRuntimeError(`Unknown ctx reference "ctx.${path}"`, stepId);
      }

      current = (current as Record<string, unknown>)[part];
    }

    return current;
  }

  private reportStatus(value: unknown): ToolStatus {
    if (value !== null && typeof value === "object" && "status" in value) {
      const status = (value as { status?: unknown }).status;
      if (status === "pass" || status === "warning" || status === "fail") {
        return status;
      }
    }

    return "warning";
  }
}
