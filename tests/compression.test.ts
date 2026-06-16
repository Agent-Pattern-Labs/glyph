import { describe, expect, it } from "vitest";
import { compareCompression } from "../src/eval/compression.js";

describe("compression eval", () => {
  it("calculates compression ratios", () => {
    const stats = compareCompression("flow main { EXPORT(result) }", {
      name: "sample",
      file: "sample.glyph",
      naturalLanguage:
        "Export the already prepared result object as the final artifact using the local harness output mechanism so it can be inspected by the caller."
    });

    expect(stats.glyphChars).toBeGreaterThan(0);
    expect(stats.naturalLanguageApproxTokens).toBeGreaterThan(stats.glyphApproxTokens);
    expect(stats.compressionRatio).toBeGreaterThan(1);
  });
});
