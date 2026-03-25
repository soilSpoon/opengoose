#!/bin/bash
# Signs test binary with Hypervisor.framework entitlement before running.
# Used as [target.aarch64-apple-darwin].runner in .cargo/config.toml
set -euo pipefail
BINARY="$1"
shift
ENTITLEMENTS_PLIST="$(dirname "$0")/hvf-entitlements.plist"
if ! codesign --sign - --entitlements "$ENTITLEMENTS_PLIST" --force "$BINARY" 2>/dev/null; then
    echo "warning: codesign failed for $BINARY — HVF tests may fail" >&2
fi
exec "$BINARY" "$@"
