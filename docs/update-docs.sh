#!/bin/bash
set -euo pipefail

cargo doc --target x86_64-pc-windows-msvc --target x86_64-unknown-linux-gnu --target-dir docs/ --document-private-items --all-features --output-format json -Z unstable-options
docs/update-indexes.sh
