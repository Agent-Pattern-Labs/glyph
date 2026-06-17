#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-${TMPDIR:-/tmp}/glyph-static-proof}"

rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

cd "$ROOT_DIR"

echo "== Rust checks =="
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test

echo "== Static controller proof artifacts =="
cargo run --quiet -- fingerprint-controller >"$OUT_DIR/fingerprint.json"
cargo run --quiet -- check-controller-fingerprint-lock >"$OUT_DIR/fingerprint-lock-check.json"
cargo run --quiet -- check-conformance --output "$OUT_DIR/conformance.json" >"$OUT_DIR/conformance-summary.json"
cargo run --quiet -- check-controller-dataset >"$OUT_DIR/dataset-quality.json"
cargo run --quiet -- check-controller-curriculum >"$OUT_DIR/curriculum-quality.json"
cargo run --quiet -- check-controller-robustness >"$OUT_DIR/robustness.json"
cargo run --quiet -- status-controller-claim --output "$OUT_DIR/claim-status.json" >"$OUT_DIR/claim-status-summary.json"

echo "== Constrained decoding prompt bundle =="
cargo run --quiet -- eval-controller \
  --prompt-mode all \
  --grammar-payload gbnf \
  --emit-prompts "$OUT_DIR/prompt-bundle" \
  --case-limit 1 \
  >"$OUT_DIR/prompt-bundle-eval-summary.json"
cargo run --quiet -- verify-controller-prompt-bundle \
  "$OUT_DIR/prompt-bundle" \
  >"$OUT_DIR/prompt-bundle-verification.json"
cargo run --quiet -- export-controller-offline-queue \
  --prompt-bundle "$OUT_DIR/prompt-bundle" \
  --responses "$OUT_DIR/offline-responses" \
  --output "$OUT_DIR/offline-queue.jsonl" \
  --manifest "$OUT_DIR/offline-queue.manifest.json" \
  >"$OUT_DIR/offline-queue-summary.json"

echo "== Offline response scoring smoke =="
for mode in constrained schema-only plain; do
  response_dir="$OUT_DIR/offline-responses/cases/$mode"
  mkdir -p "$response_dir"
  cp "spec/fixtures/hello.glyph" "$response_dir/hello_summary_normal_short.glyph.txt"
  printf '%s\n' \
    'Capture hello world, summarize it, and export the summary. This prose is intentionally not executable Glyph.' \
    >"$response_dir/hello_summary_normal_short.direct-prose.txt"
  cat >"$response_dir/hello_summary_normal_short.json-tool-plan.txt" <<'JSON'
{"goal":"Say hello through the harness","context":{},"steps":[{"op":"SPEC","args":{"message":"hello world"},"assignTo":"spec"},{"op":"SUM","args":{"target":{"var":"spec"}},"assignTo":"summary"},{"op":"EXPORT","args":{"target":{"var":"summary"}}}]}
JSON
done
cargo run --quiet -- check-controller-offline-responses \
  --prompt-bundle "$OUT_DIR/prompt-bundle" \
  --responses "$OUT_DIR/offline-responses" \
  >"$OUT_DIR/offline-responses-check.json"
cargo run --quiet -- score-controller-responses \
  --prompt-bundle "$OUT_DIR/prompt-bundle" \
  --responses "$OUT_DIR/offline-responses" \
  --model-id static-proof-offline-1b \
  --bucket 1b \
  --jsonl "$OUT_DIR/offline-responses.jsonl" \
  --manifest "$OUT_DIR/offline-responses.manifest.json" \
  >"$OUT_DIR/offline-responses-summary.json"
cargo run --quiet -- verify-controller-run \
  "$OUT_DIR/offline-responses.jsonl" \
  "$OUT_DIR/offline-responses.manifest.json" \
  >"$OUT_DIR/offline-responses-verification.json"
cat >"$OUT_DIR/offline-responses-plan.json" <<JSON
{
  "version": "glyph-controller-offline-plan/0.1",
  "totalExpectedRows": 3,
  "shards": [
    {
      "id": "bucket-1b",
      "bucket": "1b",
      "jsonlPath": "$OUT_DIR/offline-responses.jsonl",
      "manifestPath": "$OUT_DIR/offline-responses.manifest.json",
      "expectedRows": 3
    }
  ]
}
JSON
cargo run --quiet -- verify-controller-shards \
  --plan "$OUT_DIR/offline-responses-plan.json" \
  >"$OUT_DIR/offline-responses-shard-verification.json"
cargo run --quiet -- plan-controller-offline-run \
  --artifact-dir "$OUT_DIR/offline-plan" \
  --output "$OUT_DIR/offline-plan/offline-plan.json" \
  >"$OUT_DIR/offline-plan-summary.json"

echo "== Manifest-backed training exports =="
cargo run --quiet -- export-controller-dataset \
  --output "$OUT_DIR/controller-dataset.jsonl" \
  --manifest "$OUT_DIR/controller-dataset.manifest.json" \
  >"$OUT_DIR/controller-dataset-export-summary.json"
cargo run --quiet -- verify-controller-training-export \
  "$OUT_DIR/controller-dataset.manifest.json" \
  >"$OUT_DIR/controller-dataset-verification.json"

cargo run --quiet -- export-controller-curriculum \
  --output "$OUT_DIR/controller-curriculum.jsonl" \
  --manifest "$OUT_DIR/controller-curriculum.manifest.json" \
  >"$OUT_DIR/controller-curriculum-export-summary.json"
cargo run --quiet -- verify-controller-training-export \
  "$OUT_DIR/controller-curriculum.manifest.json" \
  >"$OUT_DIR/controller-curriculum-verification.json"

echo "== Evidence pack seal =="
cargo run --quiet -- export-controller-evidence-pack \
  --output "$OUT_DIR/evidence-pack" \
  >"$OUT_DIR/evidence-pack-export-summary.json"
cargo run --quiet -- verify-controller-evidence-pack \
  "$OUT_DIR/evidence-pack" \
  >"$OUT_DIR/evidence-pack-verification.json"

echo "Static proof artifacts written to $OUT_DIR"
