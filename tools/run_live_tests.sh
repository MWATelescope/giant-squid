#!/usr/bin/env bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# Run the live tests (tests/live.rs) against the MWA ASVO test server. They
# submit and cancel real jobs there, so they are `#[ignore]`d and never run in
# CI. Run this by hand, before a commit that touches the API client.
set -euo pipefail

cd "$(dirname "$0")/.."

MWA_ASVO_E2E_TARGET=https://test-asvo.mwatelescope.org \
    cargo test --test live -- --ignored --test-threads=1 --nocapture
