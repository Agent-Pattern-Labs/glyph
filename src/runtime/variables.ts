export class VariableStore {
  private readonly values = new Map<string, unknown>();

  set(name: string, value: unknown): void {
    this.values.set(name, value);
  }

  get(name: string): unknown {
    return this.values.get(name);
  }

  has(name: string): boolean {
    return this.values.has(name);
  }

  snapshot(): Record<string, unknown> {
    return Object.fromEntries(this.values.entries());
  }
}
