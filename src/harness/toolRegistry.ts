import type { ToolHandler } from "./types.js";

export class ToolRegistry {
  private readonly tools = new Map<string, ToolHandler>();

  register(name: string, handler: ToolHandler): this {
    this.tools.set(name.toUpperCase(), handler);
    return this;
  }

  get(name: string): ToolHandler | undefined {
    return this.tools.get(name.toUpperCase());
  }

  has(name: string): boolean {
    return this.tools.has(name.toUpperCase());
  }

  names(): string[] {
    return [...this.tools.keys()].sort();
  }
}
