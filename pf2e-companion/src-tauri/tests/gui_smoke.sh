#!/usr/bin/env bash
# GUI smoke test (CONTRACT-11 § 1). Boots the real app (`pnpm tauri dev`)
# under Xvfb and asserts it reaches a serving state without panicking.
#
# Self-gating: off unless PF2E_GUI_SMOKE=1. Also skips (successfully)
# when the required system tools are missing, so default CI never fails
# here. No input automation — a clean boot is the bar.

set -euo pipefail

if [[ "${PF2E_GUI_SMOKE:-}" != "1" ]]; then
  echo "SKIP gui smoke (PF2E_GUI_SMOKE not set)"
  exit 0
fi

for tool in xvfb-run curl pnpm timeout; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "SKIP gui smoke (missing tool: $tool)"
    exit 0
  fi
done

# The script lives in <app>/src-tauri/tests/ — run from the app root.
cd "$(dirname "$0")/../.."

TMP="$(mktemp -d)"
LOG="$TMP/gui-smoke.log"

# 1) Build the web bundle first so `tauri dev` only has to boot. The
#    cap only guards against a hung build; normal builds finish well
#    inside it.
if ! timeout -s TERM 240 pnpm build >"$TMP/build.log" 2>&1; then
  echo "FAIL gui smoke: pnpm build failed:" >&2
  tail -n 40 "$TMP/build.log" >&2 || true
  rm -rf "$TMP"
  exit 1
fi

# 2) Launch the dev app under Xvfb. `setsid` gives the whole dev tree
#    its own process group so cleanup can kill it in one shot; the
#    45s hard budget is the contract's (CONTRACT-11 § 1).
setsid timeout -s TERM 45 xvfb-run -a pnpm tauri dev >"$LOG" 2>&1 &
PID=$!

cleanup() {
  kill -TERM "$PID" 2>/dev/null || true
  pkill -TERM -P "$PID" '^vite$|^cargo$' 2>/dev/null || true
  kill -TERM -- "-$PID" 2>/dev/null || true
  wait "$PID" 2>/dev/null || true
  rm -rf "$TMP"
}
trap cleanup EXIT

# 3) Poll (1s × 40): process alive + no panic in the log + vite
#    serving HTTP 200.
for _ in $(seq 1 40); do
  if ! kill -0 "$PID" 2>/dev/null; then
    echo "FAIL gui smoke: dev process exited early:" >&2
    tail -n 40 "$LOG" >&2 || true
    exit 1
  fi
  if grep -qE 'panic|thread .*panicked|RUST_BACKTRACE' "$LOG"; then
    echo "FAIL gui smoke: panic detected in dev output:" >&2
    grep -nE 'panic|thread .*panicked|RUST_BACKTRACE' "$LOG" | head -n 5 >&2 || true
    exit 1
  fi
  if curl -sf -o /dev/null http://localhost:1420; then
    echo "OK gui smoke: app booted cleanly under Xvfb (vite serving, no panics)"
    exit 0
  fi
  sleep 1
done

echo "FAIL gui smoke: vite did not serve within the budget:" >&2
tail -n 40 "$LOG" >&2 || true
exit 1
