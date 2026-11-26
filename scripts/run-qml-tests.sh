#!/bin/bash
# Behaviour tests of the QML logic that needs neither the window nor the daemon, headless. Needs qmltestrunner
# (Void: qt6-quick-test).
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RUNNER="$(command -v qmltestrunner || ls /usr/lib/qt6/bin/qmltestrunner /usr/lib64/qt6/bin/qmltestrunner 2>/dev/null | head -1)"
[ -n "$RUNNER" ] || { echo "not ok - qmltestrunner not found (package qt6-quick-test)" >&2; exit 1; }
OUT="$(mktemp)"; trap 'rm -f "$OUT"' EXIT
QT_QPA_PLATFORM=offscreen "$RUNNER" -input "$ROOT/tests/qml" 2>&1 | tee "$OUT"
grep -q "^Totals: .* 0 failed" "$OUT"
