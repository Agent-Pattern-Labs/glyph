import { syntaxError } from "./errors.js";

export type SymbolTokenValue = "{" | "}" | "(" | ")" | "[" | "]" | ":" | "," | "=" | ".";

export type Token =
  | {
      type: "identifier";
      value: string;
      line: number;
      column: number;
    }
  | {
      type: "string";
      value: string;
      line: number;
      column: number;
    }
  | {
      type: "number";
      value: number;
      raw: string;
      line: number;
      column: number;
    }
  | {
      type: "boolean";
      value: boolean;
      line: number;
      column: number;
    }
  | {
      type: "symbol";
      value: SymbolTokenValue;
      line: number;
      column: number;
    }
  | {
      type: "arrow";
      value: "->";
      line: number;
      column: number;
    }
  | {
      type: "eof";
      value: "";
      line: number;
      column: number;
    };

const SYMBOLS = new Set(["{", "}", "(", ")", "[", "]", ":", ",", "=", "."]);

export function tokenize(source: string): Token[] {
  const tokens: Token[] = [];
  let index = 0;
  let line = 1;
  let column = 1;

  const current = () => source[index];
  const next = () => source[index + 1];

  const advance = () => {
    const char = source[index++];
    if (char === "\n") {
      line += 1;
      column = 1;
    } else {
      column += 1;
    }
    return char;
  };

  const push = (token: Token) => tokens.push(token);

  while (index < source.length) {
    const char = current();

    if (char === " " || char === "\t" || char === "\r" || char === "\n") {
      advance();
      continue;
    }

    if (char === "#") {
      while (index < source.length && current() !== "\n") {
        advance();
      }
      continue;
    }

    const startLine = line;
    const startColumn = column;

    if (char === "-" && next() === ">") {
      advance();
      advance();
      push({ type: "arrow", value: "->", line: startLine, column: startColumn });
      continue;
    }

    if (char === '"') {
      advance();
      let value = "";

      while (index < source.length) {
        const part = advance();
        if (part === '"') {
          push({ type: "string", value, line: startLine, column: startColumn });
          break;
        }

        if (part === "\\") {
          if (index >= source.length) {
            syntaxError("Unterminated string literal", startLine, startColumn);
          }

          const escaped = advance();
          switch (escaped) {
            case '"':
              value += '"';
              break;
            case "\\":
              value += "\\";
              break;
            case "n":
              value += "\n";
              break;
            case "t":
              value += "\t";
              break;
            case "r":
              value += "\r";
              break;
            default:
              value += escaped;
          }
          continue;
        }

        value += part;
      }

      if (tokens[tokens.length - 1]?.line !== startLine || tokens[tokens.length - 1]?.column !== startColumn) {
        syntaxError("Unterminated string literal", startLine, startColumn);
      }

      continue;
    }

    if (/[0-9]/.test(char) || (char === "-" && /[0-9]/.test(next() ?? ""))) {
      let raw = "";
      if (char === "-") {
        raw += advance();
      }

      while (/[0-9]/.test(current() ?? "")) {
        raw += advance();
      }

      if (current() === ".") {
        raw += advance();
        if (!/[0-9]/.test(current() ?? "")) {
          syntaxError("Invalid number literal", startLine, startColumn);
        }

        while (/[0-9]/.test(current() ?? "")) {
          raw += advance();
        }
      }

      push({ type: "number", value: Number(raw), raw, line: startLine, column: startColumn });
      continue;
    }

    if (/[A-Za-z_]/.test(char)) {
      let value = "";
      while (/[A-Za-z0-9_]/.test(current() ?? "")) {
        value += advance();
      }

      if (value === "true" || value === "false") {
        push({ type: "boolean", value: value === "true", line: startLine, column: startColumn });
      } else {
        push({ type: "identifier", value, line: startLine, column: startColumn });
      }
      continue;
    }

    if (SYMBOLS.has(char)) {
      push({ type: "symbol", value: char as SymbolTokenValue, line: startLine, column: startColumn });
      advance();
      continue;
    }

    syntaxError(`Unknown token "${char}"`, startLine, startColumn);
  }

  tokens.push({ type: "eof", value: "", line, column });
  return tokens;
}
