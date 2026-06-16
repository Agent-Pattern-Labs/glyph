import { describe, expect, it } from "vitest";
import { validateIR } from "../src/ir/validateIR.js";

describe("IR validators", () => {
  it("rejects invalid IR versions", () => {
    expect(() =>
      validateIR({
        version: "9.9",
        context: {},
        flows: [{ name: "main", steps: [] }]
      })
    ).toThrow(/Invalid literal value/);
  });

  it("rejects malformed operations", () => {
    expect(() =>
      validateIR({
        version: "0.1",
        context: {},
        flows: [
          {
            name: "main",
            steps: [{ kind: "tool", id: "step_1", op: "bad-op", args: {} }]
          }
        ]
      })
    ).toThrow();
  });
});
