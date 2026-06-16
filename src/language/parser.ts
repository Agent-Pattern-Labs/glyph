import type {
  CallArgAst,
  ContextAst,
  ExpressionAst,
  FlowAst,
  ObjectEntryAst,
  ProgramAst,
  RepairBlockAst,
  StatementAst,
  ToolCallAst
} from "./ast.js";
import { GlyphSyntaxError } from "./errors.js";
import { tokenize, type SymbolTokenValue, type Token } from "./tokenizer.js";

export function parseGlyph(source: string): ProgramAst {
  return new Parser(tokenize(source)).parseProgram();
}

class Parser {
  private index = 0;

  constructor(private readonly tokens: Token[]) {}

  parseProgram(): ProgramAst {
    let goal: string | undefined;
    let context: ContextAst | undefined;
    const flows: FlowAst[] = [];

    while (!this.isEof()) {
      if (this.matchIdentifier("goal")) {
        if (goal !== undefined) {
          this.failHere("Duplicate goal declaration");
        }
        goal = this.parseGoal();
        continue;
      }

      if (this.matchIdentifier("ctx")) {
        if (context !== undefined) {
          this.failHere("Duplicate ctx declaration");
        }
        context = this.parseContext();
        continue;
      }

      if (this.matchIdentifier("flow")) {
        flows.push(this.parseFlow());
        continue;
      }

      if (this.peek().type === "identifier") {
        this.failHere(`Unknown block type "${this.peek().value}"`);
      }

      this.failHere(`Unexpected token ${this.describe(this.peek())}`);
    }

    if (flows.length === 0) {
      this.failAtLast("Program must declare at least one flow");
    }

    return { kind: "Program", goal, context, flows };
  }

  private parseGoal(): string {
    this.consumeIdentifier("goal");
    const token = this.consume("string", "Expected goal string");
    return token.value;
  }

  private parseContext(): ContextAst {
    this.consumeIdentifier("ctx");
    this.consumeSymbol("{", "Expected opening brace after ctx");
    const entries = this.parseObjectEntries("}");
    this.consumeSymbol("}", "Missing closing brace for ctx block");
    return { kind: "Context", entries };
  }

  private parseFlow(): FlowAst {
    this.consumeIdentifier("flow");
    const name = this.consume("identifier", "Expected flow name").value;
    this.consumeSymbol("{", "Expected opening brace after flow name");

    const steps: StatementAst[] = [];
    while (!this.isEof() && !this.matchSymbol("}")) {
      steps.push(this.parseStatement());
    }

    this.consumeSymbol("}", `Missing closing brace for flow "${name}"`);
    return { kind: "Flow", name, steps };
  }

  private parseStatement(): StatementAst {
    if (this.matchIdentifier("repair")) {
      return this.parseRepairBlock();
    }

    if (this.peek().type === "identifier") {
      return this.parseToolCall();
    }

    this.failHere(`Unexpected token in flow: ${this.describe(this.peek())}`);
  }

  private parseToolCall(): ToolCallAst {
    const op = this.consume("identifier", "Expected tool operation").value;

    if (!this.matchSymbol("(")) {
      this.failHere(`Invalid tool call "${op}": expected argument list`);
    }

    this.consumeSymbol("(", `Expected opening parenthesis after ${op}`);
    const args = this.parseCallArgs();
    this.consumeSymbol(")", `Missing closing parenthesis for ${op}`);

    let assignTo: string | undefined;
    if (this.matchArrow()) {
      this.consumeArrow();
      if (this.peek().type !== "identifier") {
        this.failHere("Invalid assignment: expected variable name after ->");
      }
      assignTo = this.advance().value as string;
    }

    return { kind: "ToolCall", op, args, assignTo };
  }

  private parseCallArgs(): CallArgAst[] {
    const args: CallArgAst[] = [];
    if (this.matchSymbol(")")) {
      return args;
    }

    while (!this.isEof()) {
      let name: string | undefined;
      if (this.peek().type === "identifier" && this.peek(1).type === "symbol" && this.peek(1).value === "=") {
        name = this.advance().value as string;
        this.consumeSymbol("=", `Expected = after argument name "${name}"`);
      }

      args.push({ name, value: this.parseExpression() });

      if (this.matchSymbol(",")) {
        this.advance();
        if (this.matchSymbol(")")) {
          break;
        }
        continue;
      }

      break;
    }

    return args;
  }

  private parseRepairBlock(): RepairBlockAst {
    this.consumeIdentifier("repair");
    const target = this.consume("identifier", "Invalid repair block: expected target variable").value;
    this.consumeIdentifier("with", "Invalid repair block: expected with");
    const report = this.consume("identifier", "Invalid repair block: expected report variable").value;
    this.consumeIdentifier("max", "Invalid repair block: expected max");
    const maxToken = this.consume("number", "Invalid repair block: max must be a number");

    if (!Number.isInteger(maxToken.value) || maxToken.value < 0) {
      throw new GlyphSyntaxError("Invalid repair block: max must be a non-negative integer", maxToken.line, maxToken.column);
    }

    this.consumeSymbol("{", "Invalid repair block: expected opening brace");
    const steps: StatementAst[] = [];
    while (!this.isEof() && !this.matchSymbol("}")) {
      steps.push(this.parseStatement());
    }
    this.consumeSymbol("}", "Invalid repair block: missing closing brace");

    return { kind: "RepairBlock", target, report, max: maxToken.value, steps };
  }

  private parseExpression(): ExpressionAst {
    const token = this.peek();

    if (token.type === "string") {
      this.advance();
      return { kind: "StringLiteral", value: token.value };
    }

    if (token.type === "number") {
      this.advance();
      return { kind: "NumberLiteral", value: token.value };
    }

    if (token.type === "boolean") {
      this.advance();
      return { kind: "BooleanLiteral", value: token.value };
    }

    if (token.type === "identifier") {
      const name = this.advance().value as string;
      if (name === "ctx" && this.matchSymbol(".")) {
        const path: string[] = [];
        while (this.matchSymbol(".")) {
          this.advance();
          path.push(this.consume("identifier", "Expected ctx property after .").value);
        }

        if (path.length === 0) {
          this.failHere("Expected ctx property reference");
        }

        return { kind: "CtxRef", path };
      }

      return { kind: "VarRef", name };
    }

    if (this.matchSymbol("[")) {
      this.advance();
      const items: ExpressionAst[] = [];

      while (!this.isEof() && !this.matchSymbol("]")) {
        items.push(this.parseExpression());
        if (this.matchSymbol(",")) {
          this.advance();
          continue;
        }

        if (!this.matchSymbol("]")) {
          this.failHere("Invalid array literal: expected comma or closing bracket");
        }
      }

      this.consumeSymbol("]", "Invalid array literal: missing closing bracket");
      return { kind: "ArrayLiteral", items };
    }

    if (this.matchSymbol("{")) {
      this.advance();
      const entries = this.parseObjectEntries("}");
      this.consumeSymbol("}", "Invalid object literal: missing closing brace");
      return { kind: "ObjectLiteral", entries };
    }

    this.failHere(`Invalid argument: expected expression, got ${this.describe(token)}`);
  }

  private parseObjectEntries(endSymbol: "}") {
    const entries: ObjectEntryAst[] = [];

    while (!this.isEof() && !this.matchSymbol(endSymbol)) {
      const keyToken = this.peek();
      if (keyToken.type !== "identifier" && keyToken.type !== "string") {
        this.failHere(`Invalid object key: expected identifier or string, got ${this.describe(keyToken)}`);
      }

      const key = this.advance().value as string;
      this.consumeSymbol(":", `Expected : after object key "${key}"`);
      entries.push({ key, value: this.parseExpression() });

      if (this.matchSymbol(",")) {
        this.advance();
      }
    }

    return entries;
  }

  private matchIdentifier(value: string): boolean {
    const token = this.peek();
    return token.type === "identifier" && token.value === value;
  }

  private consumeIdentifier(value: string, message?: string): void {
    const token = this.peek();
    if (token.type !== "identifier" || token.value !== value) {
      this.failHere(message ?? `Expected ${value}`);
    }
    this.advance();
  }

  private matchSymbol(value: SymbolTokenValue): boolean {
    const token = this.peek();
    return token.type === "symbol" && token.value === value;
  }

  private consumeSymbol(value: SymbolTokenValue, message: string): void {
    if (!this.matchSymbol(value)) {
      this.failHere(message);
    }
    this.advance();
  }

  private matchArrow(): boolean {
    return this.peek().type === "arrow";
  }

  private consumeArrow(): void {
    if (!this.matchArrow()) {
      this.failHere("Expected ->");
    }
    this.advance();
  }

  private consume<T extends Token["type"]>(type: T, message: string): Extract<Token, { type: T }> {
    const token = this.peek();
    if (token.type !== type) {
      this.failHere(message);
    }
    return this.advance() as Extract<Token, { type: T }>;
  }

  private advance(): Token {
    return this.tokens[this.index++] ?? this.tokens[this.tokens.length - 1];
  }

  private peek(offset = 0): Token {
    return this.tokens[this.index + offset] ?? this.tokens[this.tokens.length - 1];
  }

  private isEof(): boolean {
    return this.peek().type === "eof";
  }

  private failHere(message: string): never {
    const token = this.peek();
    throw new GlyphSyntaxError(message, token.line, token.column);
  }

  private failAtLast(message: string): never {
    const token = this.tokens[Math.max(0, this.tokens.length - 2)] ?? this.peek();
    throw new GlyphSyntaxError(message, token.line, token.column);
  }

  private describe(token: Token): string {
    if (token.type === "eof") {
      return "end of file";
    }

    return `${token.type} "${String(token.value)}"`;
  }
}
