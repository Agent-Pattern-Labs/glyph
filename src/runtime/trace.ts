import type { ToolStatus } from "../harness/types.js";

export interface TraceEvent {
  stepId: string;
  operation: string;
  resolvedArgs: Record<string, unknown>;
  outputSummary: string;
  status: ToolStatus;
  durationMs: number;
  errors?: string[];
  iteration?: number;
}

export class ExecutionTrace {
  private readonly events: TraceEvent[] = [];

  add(event: TraceEvent): void {
    this.events.push(event);
  }

  all(): TraceEvent[] {
    return [...this.events];
  }
}
