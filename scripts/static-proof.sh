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
