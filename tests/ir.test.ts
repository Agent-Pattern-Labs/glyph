import { describe, expect, it } from "vitest";
import { compileAstToIR, parseGlyphToIR } from "../src/ir/glyphIR.js";
import { validateIR } from "../src/ir/validateIR.js";
import { parseGlyph } from "../src/language/parser.js";

describe("GlyphIR", () => {
  it("converts AST to IR", () => {
    const ast = parseGlyph(`
      flow main {
        SPEC(app="tracker") -> spec
        PLAN(spec) -> plan
      }
    `);

    const ir = compileAstToIR(ast);
    expect(ir.version).toBe("0.1");
    expect(ir.flows[0]?.steps[1]).toMatchObject({
      kind: "tool",
      op: "PLAN",
      args: { input: { var: "spec" } },
      assignTo: "plan"
    });
  });

  it("validates IR with Zod", () => {
    const ir = parseGlyphToIR(`
      goal "Build"
      flow main {
        SPEC(app="tracker") -> spec
      }
    `);

    expect(validateIR(ir)).toEqual(ir);
  });
});
