import { describe, expect, it } from "vitest";
import { parseGlyph } from "../src/language/parser.js";

describe("parser", () => {
  it("parses a simple flow", () => {
    const ast = parseGlyph(`
      goal "Say hello"
      flow main {
        SPEC(message="hello") -> spec
        EXPORT(spec)
      }
    `);

    expect(ast.goal).toBe("Say hello");
    expect(ast.flows[0]?.name).toBe("main");
    expect(ast.flows[0]?.steps).toHaveLength(2);
  });

  it("parses ctx declarations", () => {
    const ast = parseGlyph(`
      ctx {
        stack: "nextjs"
        enabled: true
      }

      flow main {
        GEN(stack=ctx.stack, enabled=ctx.enabled) -> files
      }
    `);

    expect(ast.context?.entries.map((entry) => entry.key)).toEqual(["stack", "enabled"]);
  });

  it("parses arrays and object arguments", () => {
    const ast = parseGlyph(`
      flow main {
        SPEC(entities=["project", "task"], rules={ auth: true, max: 3 }) -> spec
      }
    `);

    const step = ast.flows[0]?.steps[0];
    expect(step?.kind).toBe("ToolCall");
    if (step?.kind === "ToolCall") {
      expect(step.args).toHaveLength(2);
      expect(step.args[1]?.value.kind).toBe("ObjectLiteral");
    }
  });
});
