#!/usr/bin/env bash

# Fail the script on any error
set -euo pipefail

# Check how "dirty" the dir is. If dirty, stop, so that the diff after this script is only the regeneration.
changed_file_count=$(git status --porcelain=v1 -uall | wc -l)

if (( changed_file_count > 0 )); then
    echo "Error: ${changed_file_count} uncommitted file(s) found. Aborting" >&2
    git status --short
    exit 1
fi

#
# Downloads the MWA ASVO v2 OpenAPI schema and converts it into the format
# typify expects, writing the result to src/asvo/apiv2/openapi-schema.json.
#
# This is a separate, manual, network-touching step, deliberately kept out
# of the normal build. Turning that schema into Rust code
# (src/asvo/apiv2/openapi.rs) happens in build.rs, gated behind the
# `regen-openapi` feature so a normal build doesn't need typify at all.
# This script also does the regeneration. To regenerate from the schema that is
# already committed, without downloading, run:
#
#     cargo build --features regen-openapi
#     git diff src/asvo/apiv2/openapi.rs
#
# Commit openapi.rs as build.rs writes it. Do not run `cargo fmt` or
# `cargo clippy --fix` on it: the openapi-drift-check CI job regenerates the
# file and fails on any difference, and rustfmt lays the file out differently.
#
# It assumes:
# 1. You run this from inside the "tools" directory
# 2. You have `curl` and `python3` available
#
# The OpenAPI URL can be overridden for testing, e.g.:
#   OPENAPI_URL=http://localhost:8000/openapi.json ./generate_openapi.sh
#

OPENAPI_URL="${OPENAPI_URL:-https://test-asvo.mwatelescope.org/openapi.json}"
OUTPUT_FILE="src/asvo/apiv2/openapi-schema.json"

# --- Preflight checks (before we go anywhere, so there's nothing to unwind) -

if ! command -v curl &> /dev/null; then
    echo "Error: curl is required but was not found on PATH." >&2
    exit 1
fi

if ! command -v python3 &> /dev/null; then
    echo "Error: python3 is required but was not found on PATH." >&2
    exit 1
fi

# Switch to the root giant-squid dir
pushd .. > /dev/null

RAW_OPENAPI="$(mktemp --suffix=.json)"
cleanup() {
    rm -f "$RAW_OPENAPI"
    popd > /dev/null
}
trap cleanup EXIT

# --- Download ----------------------------------------------------------

echo "Downloading OpenAPI schema from ${OPENAPI_URL}..."
curl -fsS "$OPENAPI_URL" -o "$RAW_OPENAPI"

if ! python3 -c "import json; json.load(open(\"$RAW_OPENAPI\"))" 2>/dev/null; then
    echo "Error: downloaded content from ${OPENAPI_URL} is not valid JSON." >&2
    exit 1
fi

# --- Convert for typify --------------------------------------------------
#
# typify reads plain JSON Schema documents (a "definitions" map of named
# schemas), not full OpenAPI documents. Extract components.schemas into its
# own document and rewrite $ref pointers to match.

echo "Converting schema for typify..."
python3 - "$RAW_OPENAPI" "$OUTPUT_FILE" <<'PYEOF'
import json
import sys

in_path, out_path = sys.argv[1], sys.argv[2]

with open(in_path) as f:
    doc = json.load(f)

schemas = doc.get("components", {}).get("schemas")
if not schemas:
    sys.exit("Error: no components.schemas found in the downloaded OpenAPI document.")


def rewrite_refs(node):
    if isinstance(node, dict):
        for k, v in node.items():
            if k == "$ref" and isinstance(v, str) and v.startswith("#/components/schemas/"):
                node[k] = v.replace("#/components/schemas/", "#/definitions/")
            else:
                rewrite_refs(v)
    elif isinstance(node, list):
        for v in node:
            rewrite_refs(v)


rewrite_refs(schemas)

out = {
    "$schema": "http://json-schema.org/draft-07/schema#",
    "definitions": schemas,
}

with open(out_path, "w") as f:
    json.dump(out, f, indent=2)
PYEOF

echo "Wrote ${OUTPUT_FILE}."
cargo build --features regen-openapi

# The regenerated file is committed as build.rs wrote it (prettyplease). It is
# not formatted with `cargo fmt` or changed with `cargo clippy --fix`, because
# the openapi-drift-check CI job compares it with a fresh regeneration.
cargo check

git status
echo "Done"