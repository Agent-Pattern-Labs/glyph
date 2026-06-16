import { z } from "zod";
import type { GlyphIR, GlyphIRStep, GlyphIRValue, GlyphRepairStep } from "./glyphIR.js";

export class GlyphIRValidationError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "GlyphIRValidationError";
  }
}

const identifierSchema = z.string().regex(/^[A-Za-z_][A-Za-z0-9_]*$/);
const opSchema = z.string().regex(/^[A-Z][A-Z0-9_]*$/);

export const glyphIRValueSchema: z.ZodType<GlyphIRValue> = z.lazy(() =>
  z.union([
    z.string(),
    z.number(),
    z.boolean(),
    z.null(),
    z.array(glyphIRValueSchema),
    z.record(glyphIRValueSchema)
  ])
);

const toolStepSchema = z
  .object({
    kind: z.literal("tool"),
    id: identifierSchema.or(z.string().regex(/^step_[0-9]+$/)),
    op: opSchema,
    args: z.record(glyphIRValueSchema),
    assignTo: identifierSchema.optional()
  })
  .strict();

let stepSchema: z.ZodType<GlyphIRStep>;

const repairStepSchema: z.ZodType<GlyphRepairStep> = z.lazy(() =>
  z
    .object({
      kind: z.literal("repair"),
      id: identifierSchema.or(z.string().regex(/^step_[0-9]+$/)),
      targetVar: identifierSchema,
      reportVar: identifierSchema,
      maxIterations: z.number().int().min(0),
      steps: z.array(stepSchema)
    })
    .strict()
);

stepSchema = z.lazy(() => z.union([toolStepSchema, repairStepSchema]));

export const glyphIRSchema: z.ZodType<GlyphIR> = z
  .object({
    version: z.literal("0.1"),
    goal: z.string().optional(),
    context: z.record(glyphIRValueSchema),
    flows: z
      .array(
        z
          .object({
            name: identifierSchema,
            steps: z.array(stepSchema)
          })
          .strict()
      )
      .min(1)
  })
  .strict();

export function validateIR(ir: unknown): GlyphIR {
  const result = glyphIRSchema.safeParse(ir);

  if (!result.success) {
    throw new GlyphIRValidationError(result.error.issues.map((issue) => issue.message).join("; "));
  }

  return result.data;
}
