set -e
MWA_ASVO_E2E_TARGET=https://test-asvo.mwatelescope.org cargo test --test live -- --ignored --test-threads=1 --nocapture
