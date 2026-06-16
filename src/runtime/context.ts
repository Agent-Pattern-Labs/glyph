import { ExecutionTrace } from "./trace.js";
import { VariableStore } from "./variables.js";

export interface RuntimeContextOptions {
  context?: Record<string, unknown>;
  mockFS?: Record<string, unknown>;
}

export class RuntimeContext {
  readonly context: Record<string, unknown>;
  readonly variables = new VariableStore();
  readonly trace = new ExecutionTrace();
  readonly outputs: unknown[] = [];
  private readonly mockFS = new Map<string, unknown>();

  constructor(options: RuntimeContextOptions = {}) {
    this.context = options.context ?? {};

    for (const [path, value] of Object.entries(options.mockFS ?? {})) {
      this.mockFS.set(path, value);
    }
  }

  readFile(path: string): unknown {
    return this.mockFS.get(path);
  }

  writeFile(path: string, content: unknown): void {
    this.mockFS.set(path, content);
  }

  snapshotFS(): Record<string, unknown> {
    return Object.fromEntries(this.mockFS.entries());
  }
}
