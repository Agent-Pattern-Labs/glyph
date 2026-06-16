export class GlyphSyntaxError extends Error {
  readonly line: number;
  readonly column: number;

  constructor(message: string, line: number, column: number) {
    super(`${message} at ${line}:${column}`);
    this.name = "GlyphSyntaxError";
    this.line = line;
    this.column = column;
  }
}

export function syntaxError(message: string, line: number, column: number): never {
  throw new GlyphSyntaxError(message, line, column);
}
