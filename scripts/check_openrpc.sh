#!/usr/bin/env bash
# Validate the committed openrpc.json against structural requirements.
#
# Checks performed (in order):
#   1. JSON syntax validity (via Python — no extra deps required).
#   2. Required OpenRPC top-level fields: openrpc, info.title, info.version, methods.
#   3. All expected vault + discovery methods are present.
#   4. All custom error codes are documented in components.errors.
#   5. (Optional) Deep schema validation via @open-rpc/schema-utils-js if npx
#      is available.
#
# Usage:
#   ./scripts/check_openrpc.sh [path/to/openrpc.json]
#
# Exit codes:
#   0 — all checks passed
#   1 — one or more checks failed
#
# Intended for local development and CI:
#   make check-openrpc
#   # or in a CI step:
#   ./scripts/check_openrpc.sh

set -euo pipefail

SPEC="${1:-openrpc.json}"
PASS=0
FAIL=1

# ── Colours (disabled when not a terminal) ────────────────────────────────────
if [ -t 1 ]; then
  GREEN='\033[0;32m'; RED='\033[0;31m'; YELLOW='\033[1;33m'; RESET='\033[0m'
else
  GREEN=''; RED=''; YELLOW=''; RESET=''
fi

ok()   { echo -e "${GREEN}  ✓${RESET} $*"; }
fail() { echo -e "${RED}  ✗${RESET} $*"; OVERALL_STATUS=$FAIL; }
info() { echo -e "${YELLOW}  ·${RESET} $*"; }

OVERALL_STATUS=$PASS

echo
echo "Validating: ${SPEC}"
echo "────────────────────────────────────────"

# ── 1. JSON syntax ────────────────────────────────────────────────────────────
echo
echo "1. JSON syntax"

if python3 -c "
import json, sys
try:
    with open(sys.argv[1]) as f:
        json.load(f)
    sys.exit(0)
except json.JSONDecodeError as e:
    print(f'     {e}')
    sys.exit(1)
" "$SPEC"; then
  ok "Valid JSON"
else
  fail "Invalid JSON — fix syntax errors above before continuing"
  # Fatal: remaining checks would all fail on bad JSON
  exit $FAIL
fi

# ── 2. Required OpenRPC fields ────────────────────────────────────────────────
echo
echo "2. Required OpenRPC fields"

python3 - "$SPEC" <<'PYEOF'
import json, sys

with open(sys.argv[1]) as f:
    spec = json.load(f)

errors = []

def check(condition, message):
    if not condition:
        errors.append(message)
        print(f"  \033[0;31m  ✗\033[0m {message}")
    else:
        print(f"  \033[0;32m  ✓\033[0m {message}")

check(isinstance(spec.get("openrpc"), str),    "spec.openrpc is a string")
check(isinstance(spec.get("info"), dict),       "spec.info is an object")
check(isinstance(spec.get("info", {}).get("title"), str),   "spec.info.title is a string")
check(isinstance(spec.get("info", {}).get("version"), str), "spec.info.version is a string")
check(isinstance(spec.get("methods"), list),   "spec.methods is an array")

if errors:
    sys.exit(1)
PYEOF

if [ $? -ne 0 ]; then
  fail "Required field checks failed"
  OVERALL_STATUS=$FAIL
fi

# ── 3. Expected methods ───────────────────────────────────────────────────────
echo
echo "3. Expected methods"

python3 - "$SPEC" <<'PYEOF'
import json, sys

with open(sys.argv[1]) as f:
    spec = json.load(f)

documented = {m["name"] for m in spec.get("methods", [])}

expected = [
    "vault_storeProof",
    "vault_retrieveProof",
    "vault_deleteProof",
    "rpc.discover",
]

all_ok = True
for method in expected:
    if method in documented:
        print(f"  \033[0;32m  ✓\033[0m {method}")
    else:
        print(f"  \033[0;31m  ✗\033[0m {method}  ← missing")
        all_ok = False

if not all_ok:
    sys.exit(1)
PYEOF

if [ $? -ne 0 ]; then
  fail "Method coverage check failed"
  OVERALL_STATUS=$FAIL
fi

# ── 4. Custom error codes ─────────────────────────────────────────────────────
echo
echo "4. Custom error codes in components.errors"

python3 - "$SPEC" <<'PYEOF'
import json, sys

with open(sys.argv[1]) as f:
    spec = json.load(f)

errors_section = spec.get("components", {}).get("errors", {})
documented_codes = {v["code"] for v in errors_section.values() if "code" in v}

expected_codes = {
    -32001: "ProofNotFound",
    -32002: "ProofExpired",
    -32003: "InvalidClaimId",
    -32004: "DecryptionFailed",
    -32005: "InternalError",
}

all_ok = True
for code, name in sorted(expected_codes.items()):
    if code in documented_codes:
        print(f"  \033[0;32m  ✓\033[0m {code}  ({name})")
    else:
        print(f"  \033[0;31m  ✗\033[0m {code}  ({name})  ← missing")
        all_ok = False

if not all_ok:
    sys.exit(1)
PYEOF

if [ $? -ne 0 ]; then
  fail "Error code coverage check failed"
  OVERALL_STATUS=$FAIL
fi

# ── 5. Deep schema validation (optional) ─────────────────────────────────────
echo
echo "5. Deep OpenRPC schema validation (optional)"

if command -v npx &>/dev/null; then
  info "npx found — running @open-rpc/schema-utils-js validator..."
  if npx --yes @open-rpc/schema-utils-js validate "$SPEC" 2>/dev/null; then
    ok "Deep schema validation passed"
  else
    info "Deep validation failed or validator unavailable (non-fatal)"
  fi
else
  info "npx not found — skipping deep validation (install Node.js to enable)"
fi

# ── Summary ───────────────────────────────────────────────────────────────────
echo
echo "────────────────────────────────────────"
if [ $OVERALL_STATUS -eq $PASS ]; then
  echo -e "${GREEN}All checks passed.${RESET}"
else
  echo -e "${RED}One or more checks failed.${RESET}"
fi
echo

exit $OVERALL_STATUS
