#!/usr/bin/env node
import { Command } from "commander";
import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { compareCompression } from "../eval/compression.js";
import { findCompressionExample } from "../eval/examples.js";
import { createMockToolRegistry } from "../harness/mockTools.js";
import { parseGlyphToIR } from "../ir/glyphIR.js";
import { validateIR } from "../ir/validateIR.js";
import { formatGlyph } from "../language/formatter.js";
import { parseGlyph } from "../language/parser.js";
import { GlyphVM } from "../runtime/glyphVM.js";

const program = new Command();

program.name("glyph").description("GlyphVM CLI").version("0.1.0");

program
  .command("parse")
  .description("Parse a .glyph file and print AST and/or IR")
  .argument("<file>", "Glyph source file")
  .option("--ast", "print AST")
  .option("--ir", "print IR")
  .action(async (file, options: { ast?: boolean; ir?: boolean }) => {
    const { source } = await readGlyphFile(file);
    const ast = parseGlyph(source);
    const ir = validateIR(parseGlyphToIR(source));

    if (options.ast && options.ir) {
      printJson({ ast, ir });
      return;
    }

    printJson(options.ast ? ast : ir);
  });

program
  .command("run")
  .description("Execute a .glyph program with mock harness tools")
  .argument("<file>", "Glyph source file")
  .action(async (file) => {
    const { source } = await readGlyphFile(file);
    const vm = new GlyphVM(createMockToolRegistry());
    const result = await vm.runSource(source);
    printJson({
      trace: result.trace,
      outputs: result.outputs,
      variables: result.variables
    });
  });

program
  .command("format")
  .description("Format Glyph source")
  .argument("<file>", "Glyph source file")
  .option("-w, --write", "write formatted output back to the file")
  .action(async (file, options: { write?: boolean }) => {
    const { source, resolvedPath } = await readGlyphFile(file);
    const formatted = formatGlyph(source);

    if (options.write) {
      await writeFile(resolvedPath, formatted, "utf8");
      console.log(`Formatted ${displayPath(resolvedPath)}`);
      return;
    }

    process.stdout.write(formatted);
  });

program
  .command("check")
  .description("Parse and validate a .glyph file without running it")
  .argument("<file>", "Glyph source file")
  .action(async (file) => {
    const { source, resolvedPath } = await readGlyphFile(file);
    validateIR(parseGlyphToIR(source));
    console.log(`OK ${displayPath(resolvedPath)}`);
  });

program
  .command("compress")
  .description("Compare Glyph source length against a verbose natural-language equivalent")
  .argument("<file>", "Glyph source file")
  .action(async (file) => {
    const { source } = await readGlyphFile(file);
    const example = findCompressionExample(file);

    if (!example) {
      throw new Error(`No compression eval example registered for ${file}`);
    }

    printJson({
      example: example.name,
      ...compareCompression(source, example)
    });
  });

async function readGlyphFile(input: string): Promise<{ source: string; resolvedPath: string }> {
  const candidates = [
    path.resolve(input),
    path.resolve("src", input),
    path.resolve("src/examples", path.basename(input))
  ];

  for (const candidate of [...new Set(candidates)]) {
    try {
      return {
        source: await readFile(candidate, "utf8"),
        resolvedPath: candidate
      };
    } catch (error) {
      if (!isNotFound(error)) {
        throw error;
      }
    }
  }

  throw new Error(`Glyph file not found: ${input}`);
}

function printJson(value: unknown): void {
  console.log(JSON.stringify(value, null, 2));
}

function displayPath(filePath: string): string {
  return path.relative(process.cwd(), filePath) || ".";
}

function isNotFound(error: unknown): boolean {
  return error !== null && typeof error === "object" && "code" in error && (error as { code?: unknown }).code === "ENOENT";
}

program.parseAsync(process.argv).catch((error: unknown) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
});
