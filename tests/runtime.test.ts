import { readFile } from "node:fs/promises";
import { describe, expect, it } from "vitest";
import { createMockToolRegistry } from "../src/harness/mockTools.js";
import { GlyphVM } from "../src/runtime/glyphVM.js";

function createVM() {
  return new GlyphVM(createMockToolRegistry());
}

describe("GlyphVM", () => {
  it("executes a simple flow", async () => {
    const result = await createVM().runSource(`
      flow main {
        SPEC(message="hello") -> spec
        SUM(spec) -> summary
        EXPORT(summary)
      }
    `);

    expect(result.outputs).toHaveLength(1);
    expect(result.trace.map((event) => event.operation)).toEqual(["SPEC", "SUM", "EXPORT"]);
  });

  it("resolves variables", async () => {
    const result = await createVM().runSource(`
      flow main {
        SPEC(message="hello") -> spec
        PLAN(spec) -> plan
        EXPORT(plan)
      }
    `);

    expect(result.variables.plan).toMatchObject({ kind: "plan" });
  });

  it("rejects unknown variables", async () => {
    await expect(
      createVM().runSource(`
        flow main {
          PLAN(missing) -> plan
        }
      `)
    ).rejects.toThrow(/Unknown variable "missing"/);
  });

  it("rejects unknown tools", async () => {
    await expect(
      createVM().runSource(`
        flow main {
          NOPE() -> result
        }
      `)
    ).rejects.toThrow(/Unknown tool "NOPE"/);
  });

  it("executes repair blocks with max iterations", async () => {
    const source = await readFile("src/examples/repair_failing_tests.glyph", "utf8");
    const result = await createVM().runSource(source);

    expect(result.variables.report).toMatchObject({ status: "pass" });
    expect(result.trace.filter((event) => event.operation === "FIX")).toHaveLength(1);
    expect(result.trace.find((event) => event.operation === "REPAIR")).toMatchObject({ status: "pass" });
  });

  it("generates a trace", async () => {
    const result = await createVM().runSource(`
      flow main {
        SPEC(message="trace") -> spec
      }
    `);

    expect(result.trace[0]).toMatchObject({
      stepId: "step_1",
      operation: "SPEC",
      status: "pass"
    });
    expect(typeof result.trace[0]?.durationMs).toBe("number");
  });
});
