#!/usr/bin/env bash
# Regenerate every artifact derived from the fluessig catalog (schema_gen.rs,
# schema_docs.json, entl.models, tables.gen.ts, schema.gen.ts, and the three
# binding surfaces). The schema tool lives in its own repo now
# (github.com/zmaril/fluessig); this drives it against entl's schema/entl.tsp.
#
# fluessig is located via $FLUESSIG_DIR:
#   - locally: a sibling checkout (defaults to ../fluessig next to this repo).
#   - in CI:   a pinned clone (see .github/workflows/ci.yml), exported as FLUESSIG_DIR.
#
# The chain: schema/entl.tsp --(emitter)--> schema/{catalog,api}.json
#            schema/catalog.json --(fluessig-gen)--> the committed generated files.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FLUESSIG_DIR="${FLUESSIG_DIR:-$(cd "$REPO/../fluessig" 2>/dev/null && pwd || true)}"

if [ -z "${FLUESSIG_DIR:-}" ] || [ ! -f "$FLUESSIG_DIR/Cargo.toml" ]; then
  echo "error: fluessig not found. Set FLUESSIG_DIR to a fluessig checkout" >&2
  echo "       (git clone https://github.com/zmaril/fluessig), or place one at ../fluessig." >&2
  exit 1
fi

# 1. entl.tsp -> catalog.json + api.json (the emitter needs its node deps)
if [ ! -d "$FLUESSIG_DIR/emitter/node_modules" ]; then
  (cd "$FLUESSIG_DIR/emitter" && npm install)
fi
# entl.tsp imports the fluessig decorator library by the relative path it has
# when it sits beside the tool (`./typespec/lib.tsp`). Post-split it lives in
# entl/schema/, so compile a staging copy with the lib symlinked next to it —
# this keeps entl.tsp untouched (and byte-identical to fluessig's own fixture).
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
cp "$REPO/schema/entl.tsp" "$STAGE/entl.tsp"
ln -s "$FLUESSIG_DIR/typespec" "$STAGE/typespec"
(cd "$FLUESSIG_DIR/emitter" && node emit.mjs "$STAGE/entl.tsp" --out "$REPO/schema")

# 2. catalog.json + api.json -> the committed generated files
cargo run -q --manifest-path "$FLUESSIG_DIR/Cargo.toml" --bin fluessig-gen -- \
  "$REPO/schema/catalog.json" "$REPO/crates/entl-core/src/schema_gen.rs" \
  --docs "$REPO/schema/schema_docs.json" \
  --py-models "$REPO/crates/entl-python/python/entl/models.py" \
  --ts-tables "$REPO/crates/entl-node/tables.gen.ts" \
  --ts-drizzle "$REPO/crates/entl-node/schema.gen.ts" \
  --api "$REPO/schema/api.json" \
  --node "$REPO/crates/entl-node/src/generated.rs" \
  --python "$REPO/crates/entl-python/src/generated.rs" \
  --ruby "$REPO/crates/entl-ruby/src/generated.rs" \
  --banner-note 'straitjacket-allow-file:duplication — generated code repeats by design.'
