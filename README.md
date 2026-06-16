# Glyph

Glyph is a compact executable control language that lets a small model operate high-level harnesses through typed commands, validation, tracing, and repair loops.

## Why Glyph Exists

Large models often spend many tokens producing plans, code, revisions, and explanations. Glyph compresses common workflows into a tiny executable control language. A small model can learn to emit Glyph programs, while GlyphVM and domain harnesses do the heavy lifting.

The core idea is that a small controller model should emit:

```glyph
flow auth_app {
  SPEC(app="auth portal", features=["login", "signup", "reset"]) -> spec
  PLAN(spec) -> plan
  GEN(plan, stack="nextjs") -> files
  CHECK(files, using=["types", "tests"]) -> report
  FIX(files, report, max=3) -> final
  EXPORT(final)
}
```

Instead of generating an entire application, long plan, or large response directly.

## What Glyph Is Not

Glyph is not a replacement for Python, TypeScript, or normal programming languages.
Glyph is not a chatbot.
Glyph is a model-friendly control layer.

## Architecture

```text
User request
  ->
Small controller model
  ->
Glyph program
  ->
GlyphIR
  ->
GlyphVM
  ->
Harness primitives
  ->
Trace, checks, repair, final output
```

## Example

```glyph
goal "Build a CRUD app for projects and tasks"

ctx {
  stack: "nextjs"
  db: "postgres"
  auth: "email"
}

flow main {
  SPEC(app="project tracker", entities=["project", "task"], auth=ctx.auth) -> spec
  PLAN(spec) -> plan
  GEN(plan, stack=ctx.stack, db=ctx.db) -> files
  CHECK(files, using=["types", "tests", "lint"]) -> report
  FIX(files, report, max=3) -> final
  EXPORT(final, format="file_bundle")
}
```

## How To Run

```bash
npm install
npm test
npm run build
npm run glyph -- run src/examples/build_crud_app.glyph
```

The CLI also resolves `examples/build_crud_app.glyph` to `src/examples/build_crud_app.glyph`, so this works:

```bash
npm run glyph -- run examples/build_crud_app.glyph
```

Available commands:

```bash
npm run glyph -- parse examples/build_crud_app.glyph
npm run glyph -- run examples/build_crud_app.glyph
npm run glyph -- format examples/build_crud_app.glyph
npm run glyph -- check examples/build_crud_app.glyph
npm run glyph -- compress examples/build_crud_app.glyph
```

## Language Surface

Glyph supports:

- `goal "..."` top-level declarations
- `ctx { ... }` literal context declarations
- `flow name { ... }` blocks
- primitive calls such as `SPEC(...) -> spec`
- variable references such as `PLAN(spec)`
- context references such as `GEN(plan, stack=ctx.stack)`
- strings, numbers, booleans, arrays, and object literals
- comments with `#`
- bounded repair loops:

```glyph
repair files with report max 3 {
  FIX(files, report) -> files
  CHECK(files, using=["tests"]) -> report
}
```

## MVP Primitives

The mock harness includes:

- `SPEC`
- `PLAN`
- `GEN`
- `CHECK`
- `FIX`
- `PATCH`
- `SUM` and `SUMMARIZE`
- `ASK`
- `EXPORT`
- `RUN`
- `READ`
- `WRITE`

`RUN` is mocked and does not execute a real shell command in the MVP.

## How To Add A New Primitive

Tools are registered through `ToolRegistry`. GlyphVM does not hard-code tool behavior.

```ts
import { ToolRegistry } from "./src/harness/toolRegistry.js";
import type { ToolResult } from "./src/harness/types.js";

const registry = new ToolRegistry();

registry.register("CLASSIFY", async (args, ctx): Promise<ToolResult> => {
  return {
    status: "pass",
    value: {
      label: "example",
      input: args.input
    },
    summary: "Input classified"
  };
});
```

Any Glyph program can then call:

```glyph
CLASSIFY(text="hello") -> result
```

## How To Add A New Domain Harness

Domains such as code generation, documentation, support, and data cleanup should add their own tool registrations. The runtime stays the same:

1. Parse Glyph source.
2. Compile to GlyphIR.
3. Validate with Zod.
4. Resolve variables and context.
5. Call registered harness primitives.
6. Emit a trace and final outputs.

## Future Work

- train a 1B controller to emit Glyph
- generate synthetic Glyph traces from a larger teacher model
- add domain harnesses
- add codegen harness
- add validators
- add repair loops
- add model fallback routing
- benchmark Glyph controller vs direct generation model
- create a dataset of natural language request -> Glyph program -> harness trace -> final output
