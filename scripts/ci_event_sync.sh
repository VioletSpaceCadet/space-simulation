#!/usr/bin/env bash
# Verifies that every Event variant in sim_core/src/types.rs is either
# handled in ui_web/src/hooks/applyEvents.ts or explicitly allow-listed.
#
# Usage: ./scripts/ci_event_sync.sh
# Exit 0 = all events accounted for, Exit 1 = missing handlers.
set -euo pipefail

TYPES_RS="crates/sim_core/src/types.rs"
APPLY_EVENTS_TS="ui_web/src/hooks/applyEvents.ts"

# Events the FE intentionally does not handle (debug/informational only).
# Update this list if you add a new backend event that has no FE state mutation.
ALLOW_LIST="AlertCleared AlertRaised PowerConsumed ResearchRoll"

# --- Extract Event variants from Rust enum ---
rust_variants=$(
  sed -n '/^pub enum Event {/,/^}/p' "$TYPES_RS" \
    | grep -oE '^\s+[A-Z][A-Za-z]+\s*\{' \
    | grep -oE '[A-Z][A-Za-z]+' \
    | sort -u
)

# --- Extract handled event keys from TypeScript ---
# Matches handler map keys (e.g., AsteroidDiscovered: handleAsteroidDiscovered)
# or case 'EventName': in a switch statement
ts_handled=$(
  grep -oE "[A-Za-z]+: (handle[A-Za-z]+|noOp)" "$APPLY_EVENTS_TS" | grep -oE "^[A-Za-z]+"
  grep -oE "case '[A-Za-z]+'" "$APPLY_EVENTS_TS" | grep -oE "'[A-Za-z]+'" | tr -d "'" || true
)
ts_handled=$(echo "$ts_handled" | sort -u)

# Combine handled + allow-listed
all_handled=$(printf '%s\n%s' "$ts_handled" "$ALLOW_LIST" | tr ' ' '\n' | sort -u)

# --- Check for missing handlers ---
missing=$(comm -23 <(echo "$rust_variants") <(echo "$all_handled"))

if [[ -n "$missing" ]]; then
  echo "ERROR: The following Event variants are not handled in applyEvents.ts"
  echo "       and are not in the allow-list in scripts/ci_event_sync.sh:"
  echo ""
  echo "$missing" | sed 's/^/  - /'
  echo ""
  echo "Fix: add a case in applyEvents.ts, or add to ALLOW_LIST if intentionally skipped."
  exit 1
fi

rust_count=$(echo "$rust_variants" | wc -l | tr -d ' ')
ts_count=$(echo "$ts_handled" | wc -l | tr -d ' ')
allow_count=$(echo "$ALLOW_LIST" | wc -w | tr -d ' ')
echo "OK: all $rust_count Event variants are accounted for."
echo "  Handled in FE: $ts_count"
echo "  Allow-listed:  $allow_count"
