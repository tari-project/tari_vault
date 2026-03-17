# Improvement Backlog

This directory tracks planned and completed improvements to Tari Vault, organized by priority tier.

Each improvement has its own file with detailed context, design notes, and completion status.

## Status Legend

| Symbol | Meaning |
|--------|---------|
| `[ ]` | Planned |
| `[~]` | In progress |
| `[x]` | Completed |

---

## Tier 1 — Security

| ID | Title | Status |
|----|-------|--------|
| [IMP-001](imp-001-tls.md) | Add TLS support | `[ ]` |
| [IMP-002](imp-002-zeroize-claim-id-string.md) | Zeroize incoming `claim_id` string | `[ ]` |

## Tier 2 — Architecture / Storage

| ID | Title | Status |
|----|-------|--------|
| [IMP-003](imp-003-sqlite-backend.md) | SQLite storage backend | `[ ]` |
| [IMP-004](imp-004-atomic-delete-proof.md) | Collapse `delete_proof` to single storage round-trip | `[ ]` |

## Tier 3 — Reliability

| ID | Title | Status |
|----|-------|--------|
| [IMP-005](imp-005-ttl-zero-edge-case.md) | Harden TTL=0 edge case | `[ ]` |
| [IMP-006](imp-006-expiry-delete-logging.md) | Log failed expiry deletion during retrieve | `[ ]` |

## Tier 4 — Dependency Hygiene

| ID | Title | Status |
|----|-------|--------|
| [IMP-007](imp-007-dotenvy.md) | Replace `dotenv` with `dotenvy` | `[ ]` |
| [IMP-008](imp-008-fs2-replacement.md) | Replace unmaintained `fs2` file-lock crate | `[ ]` |
| [IMP-009](imp-009-tracing.md) | Migrate from `log4rs` to `tracing` | `[ ]` |

## Tier 5 — Test Coverage

| ID | Title | Status |
|----|-------|--------|
| [IMP-010](imp-010-auth-rejection-test.md) | Assert HTTP 401 in auth rejection tests | `[ ]` |
| [IMP-011](imp-011-discover-integration-test.md) | Integration test for `rpc.discover` | `[ ]` |
| [IMP-012](imp-012-delete-expired-proof-test.md) | Test `vault_deleteProof` on expired proof | `[ ]` |

## Tier 6 — Minor / Nice-to-Have

| ID | Title | Status |
|----|-------|--------|
| [IMP-013](imp-013-request-size-cap.md) | Request size cap on `proof_json` | `[ ]` |
| [IMP-014](imp-014-expires-in-secs-cast.md) | Fix lossy `u64 → i64` cast for `expires_in_secs` | `[ ]` |
| [IMP-015](imp-015-rate-limiting.md) | Per-caller rate limiting on `storeProof` | `[ ]` |
