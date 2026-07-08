#!/usr/bin/env bash
# Stand up the entl dev environment: build the Rust core + CLI. The language
# bindings (node/python/ruby) and the postgres sink have their own setup — see
# the README. Safe to run from anywhere.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "building the Rust core + CLI…"
cargo build

echo
echo "dev environment ready:"
echo "  cargo test        # run the core/cli test suite"
echo "  scripts/gen.sh    # regenerate code from the schema"
echo "  bindings (node/python/ruby) + postgres sink: see the README"
