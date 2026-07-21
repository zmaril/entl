#!/usr/bin/env bash
# Regenerate every artifact derived from the fluessig catalog (schema_gen.rs,
# schema_docs.json, entl.models, tables.gen.ts, schema.gen.ts, and the three
# binding surfaces). The schema tool lives in its own repo now
# (github.com/zmaril/fluessig); this drives it against entl's OWN schema crate.
#
# entl's schema is authored as Rust derives in crates/entl-schema (the fluessig
# Rust-derive front end) — there is no TypeSpec or Node in this chain anymore.
#
# fluessig is located via $FLUESSIG_DIR:
#   - locally: a sibling checkout (defaults to ../fluessig next to this repo).
#   - in CI:   a pinned clone (see .github/workflows/ci.yml), exported as FLUESSIG_DIR.
#
# The chain: crates/entl-schema --(cargo run emit bins)--> schema/{catalog,api}.json
#            schema/catalog.json --(fluessig-gen)--> the committed generated files.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FLUESSIG_DIR="${FLUESSIG_DIR:-$(cd "$REPO/../fluessig" 2>/dev/null && pwd || true)}"

if [ -z "${FLUESSIG_DIR:-}" ] || [ ! -f "$FLUESSIG_DIR/Cargo.toml" ]; then
  echo "error: fluessig not found. Set FLUESSIG_DIR to a fluessig checkout" >&2
  echo "       (git clone https://github.com/zmaril/fluessig), or place one at ../fluessig." >&2
  exit 1
fi

# 1. crates/entl-schema (the derive front end) -> catalog.json + api.json.
#    The schema crate is its own single-crate workspace; it reaches the fluessig
#    derive crates through a gitignored `fluessig/` symlink → $FLUESSIG_DIR (the
#    same $FLUESSIG_DIR the downstream fluessig-gen stage uses). Refresh it here
#    so a fresh checkout / a moved FLUESSIG_DIR resolves cleanly.
SCHEMA="$REPO/crates/entl-schema"
ln -sfn "$FLUESSIG_DIR" "$SCHEMA/fluessig"
cargo run -q --manifest-path "$SCHEMA/Cargo.toml" --bin fluessig-emit \
  > "$REPO/schema/catalog.json"
cargo run -q --manifest-path "$SCHEMA/Cargo.toml" --bin fluessig-emit-api \
  > "$REPO/schema/api.json"

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
