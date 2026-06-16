export class GlyphRuntimeError extends Error {
  readonly stepId?: string;

  constructor(message: string, stepId?: string) {
    super(stepId ? `${message} at ${stepId}` : message);
    this.name = "GlyphRuntimeError";
    this.stepId = stepId;
  }
}
