export interface CompressionExample {
  name: string;
  file: string;
  naturalLanguage: string;
}

export const compressionExamples: CompressionExample[] = [
  {
    name: "hello",
    file: "hello.glyph",
    naturalLanguage:
      "Create a very small harness workflow that takes the phrase hello world as the requirement, records it as a normalized specification, asks the harness to produce a concise summary of that specification, and then exports the summary object as the final local artifact so the caller can inspect the trace and output."
  },
  {
    name: "build_crud_app",
    file: "build_crud_app.glyph",
    naturalLanguage:
      "Build a CRUD application for tracking projects and tasks. The application should use Next.js as the application stack, Postgres as the database, and email-based authentication. First transform the user request into a normalized product and technical specification that identifies the project and task entities and includes the authentication requirement. Next create a structured implementation plan from that specification. Then generate a mock file bundle using the selected stack and database context. After generation, run local harness checks for TypeScript types, tests, and lint rules. If the report identifies problems, apply a bounded mock repair step with a maximum of three repair attempts. Finally export the repaired file bundle as the final artifact, preserving a trace of every harness primitive that ran."
  },
  {
    name: "repair_failing_tests",
    file: "repair_failing_tests.glyph",
    naturalLanguage:
      "Load an existing application bundle from the local mock filesystem, treat it as the current file target, and run the test checker against it. Capture the resulting report in a variable. If the tests are failing, enter a bounded repair loop that applies a fix using the current files and the latest report, replaces the files with the fixed version, and then reruns the test check to update the report. Stop as soon as the report passes or after three iterations, then export the latest files."
  },
  {
    name: "summarize_docs",
    file: "summarize_docs.glyph",
    naturalLanguage:
      "Read a local documentation target from the mock filesystem, pass the documents into a summarization primitive, and create a compact summary that can be inspected by the caller. Run checks against the summary for coverage and clarity. If the report indicates the summary needs work, apply one bounded repair step using the summary and report. Export the repaired summary as a markdown artifact."
  },
  {
    name: "generate_landing_page",
    file: "generate_landing_page.glyph",
    naturalLanguage:
      "Create a responsive landing page workflow for a developer tooling product. Specify the product name, target audience, and required page sections including hero, features, pricing, and frequently asked questions. Create a plan from the specification, generate a mock landing page bundle using the configured stack, check the result for accessibility, responsiveness, and copy quality, apply up to two mock repairs based on the report, and export the final file bundle."
  },
  {
    name: "data_cleanup_pipeline",
    file: "data_cleanup_pipeline.glyph",
    naturalLanguage:
      "Define a local data cleanup workflow for customer records. Capture the dataset name, important fields, deduplication rule, and email normalization rule as a normalized specification. Convert that specification into a plan for validation and cleanup. Generate a mock data cleanup pipeline, run a mocked validation command against the generated pipeline, perform schema and deduplication checks, patch the pipeline using the check report as instructions, and export the final pipeline artifact."
  },
  {
    name: "customer_support_reply_stub",
    file: "customer_support_reply_stub.glyph",
    naturalLanguage:
      "Create a stub workflow for a future customer support harness. Specify the customer issue, the desired calm direct tone, and the escalation policy for refund-related cases. Turn those requirements into a plan, generate a mock support reply draft, check the draft for empathy, factual accuracy, and risk, apply one bounded fix using the report, and export the final support response artifact."
  },
  {
    name: "meeting_notes_to_tasks",
    file: "meeting_notes_to_tasks.glyph",
    naturalLanguage:
      "Read meeting notes from the local mock filesystem, summarize the important decisions and action items, and generate a structured task list from that summary. Require each task to include an owner, a due date, and the related decision where applicable. Check the task list for missing owners and dates, repair missing fields once using the report, and export the final task artifact."
  },
  {
    name: "security_review",
    file: "security_review.glyph",
    naturalLanguage:
      "Run a lightweight local security review workflow. Read a file bundle from the mock filesystem, create a specification that scopes the review to secrets, dependency risks, and authorization risks, and create a structured review plan. Check the file bundle against those security dimensions, summarize the resulting report into an inspectable finding summary, and export the final security review report artifact."
  }
];

export function findCompressionExample(fileOrName: string): CompressionExample | undefined {
  const normalized = fileOrName.replace(/\\/g, "/").split("/").pop() ?? fileOrName;
  return compressionExamples.find((example) => example.file === normalized || example.name === normalized.replace(/\.glyph$/, ""));
}
