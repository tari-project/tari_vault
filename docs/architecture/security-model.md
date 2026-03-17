# Security Model

## Threat Model

| Threat | Mitigation |
|--------|-----------|
| Passive observer reads `Claim_ID` in transit | TLS required for non-loopback binds; server hard-errors at startup otherwise. `--insecure-no-tls` bypasses this for external-proxy deployments — operator responsibility to keep the vault port off the public network |
| Intermediary reads proof payload in transit | `Claim_ID` only — never plaintext on wire after `storeProof` |
| Attacker reads vault file on disk | Only ciphertext + nonce stored; AES-256-GCM key never written |
| Attacker guesses `Claim_ID` | 256-bit key space (32-byte random key); brute force infeasible |
| Replay: retrieve proof twice | Single-use: record deleted on first successful retrieval |
| Stale proofs accumulate | TTL + periodic cleanup sweep |
| Timing oracle on token comparison | `subtle::ConstantTimeEq` for bearer token check |
| Unauthenticated RPC calls | Optional Bearer token enforced at HTTP layer before RPC parsing |
| Memory leak of key material | `ZeroizeOnDrop` on all sensitive types |
| Log scraping for secrets | Only `record_id` (non-sensitive) appears in log output |
| Concurrent file corruption | `tokio::sync::Mutex` (intra-process) + `fs2` exclusive lock (inter-process) + atomic rename |

---

## Key-in-the-ID Pattern

This is the central security mechanism. The `Claim_ID` encodes everything needed to retrieve a proof:

```
Claim_ID (64-char base64url, no padding)
    = base64url_nopad( record_id[16] || encryption_key[32] )
                       ──────────────   ───────────────────
                       UUIDv4 bytes     AES-256-GCM key
                       (non-sensitive)  (NEVER reaches disk)
```

### What the vault stores on disk

```json
{
  "3f2504e0-4f89-11d3-9a0c-0305e82c3301": {
    "nonce": "base64-encoded-12-bytes",
    "ciphertext": "base64-encoded-ciphertext-with-gcm-tag",
    "stored_at": "2024-01-15T10:30:00Z",
    "expires_at": "2024-01-15T11:30:00Z"
  }
}
```

**Absent from disk:** the encryption key, the plaintext, any part of the `Claim_ID` that would allow decryption.

### What an attacker with disk access gains

- A list of `record_id` UUIDs (lookup keys only).
- Ciphertexts that are indistinguishable from random bytes without the key.
- Timestamps (stored_at, expires_at).

They cannot derive the encryption key from this data; it exists only in the `Claim_ID` held by the legitimate caller.

---

## Encryption Scheme

| Property | Value |
|----------|-------|
| Algorithm | AES-256-GCM (AEAD) |
| Key size | 256 bits (32 bytes) |
| Nonce size | 96 bits (12 bytes) — GCM standard |
| Key source | `OsRng` via `Aes256Gcm::generate_key` |
| Nonce source | `OsRng` via `Aes256Gcm::generate_nonce` |
| Key reuse | None — a fresh key and nonce are generated per `storeProof` call |
| Authentication | GCM tag provides integrity + authenticity (wrong key → decryption error) |

AES-GCM with a unique key per ciphertext eliminates nonce-reuse risk entirely: even if a nonce were reused across two records, they would use different keys so there is no vulnerability.

---

## Memory Safety

### Types with `ZeroizeOnDrop`

| Type | What gets wiped | Why |
|------|-----------------|-----|
| `PlaintextProof` | `data: Vec<u8>` (proof bytes) | Proof material never lingers in heap after use |
| `ClaimId` | `encryption_key: [u8; 32]` and `record_id: [u8; 16]` | Key material wiped when struct leaves scope |
| `Zeroizing<[u8; 48]>` | Intermediate encode buffer | Wipes the combined bytes after base64 encoding |
| `Zeroizing<Vec<u8>>` | Intermediate decode buffer | Wipes decoded bytes after splitting into fields |

### GenericArray zeroization

`Aes256Gcm::generate_key` returns a `GenericArray<u8, U32>`. This type is `Copy`, so `drop()` is a no-op. The code explicitly calls `.zeroize()` on a mutable binding:

```rust
let mut gk = generated_key;
gk.zeroize();
```

This is a subtle but important correctness point that plain `drop(generated_key)` would miss.

### `PlaintextProof::into_json`

After deserialising the byte buffer to a JSON `Value`, the method explicitly calls `self.data.zeroize()` before returning — the bytes are wiped even though `drop` would also trigger `ZeroizeOnDrop`.

---

## What Never Appears in Logs

The logging policy is enforced at the code level, not just by convention:

| Sensitive item | Logged as |
|---------------|-----------|
| `ClaimId` (full token) | Never logged |
| `encryption_key` | `<redacted>` in `Debug` output |
| `PlaintextProof` | `PlaintextProof(<redacted>)` in `Debug` output |
| `auth_token` | `<redacted>` in `ServerConfig` `Debug` output |
| RPC request params | Not logged by the vault layer |

Only `record_id` (the non-sensitive storage key, formatted as a lowercase hex UUID) appears in info/warn log lines:

```
INFO tari_vault::vault — Proof stored; record_id=3f2504e04f8911d39a0c0305e82c3301
WARN tari_vault::vault — Expired proof access attempt; record_id=3f2504e04f8911d39a0c0305e82c3301
INFO tari_vault::vault — Proof retrieved and consumed; record_id=3f2504e04f8911d39a0c0305e82c3301
```

---

## Authentication

Authentication is **optional** and operates at the HTTP transport layer via a Tower middleware (`BearerAuthLayer`), applied before JSON-RPC parsing.

### Enforcement flow

```
HTTP request arrives
        │
        ▼
BearerAuthLayer checks
Authorization: Bearer <token>
        │
   ┌────┴─────────┐
   │ token valid  │  token missing/wrong
   │ (or disabled)│
   ▼              ▼
JSON-RPC        HTTP 401 Unauthorized
handler         WWW-Authenticate: Bearer realm="tari_vault"
```

### Token comparison

Comparison uses `subtle::ConstantTimeEq`:

```rust
bool::from(provided_bytes.ct_eq(expected_bytes))
```

This prevents timing oracles: the comparison takes the same wall-clock time regardless of how many bytes match, so an attacker cannot determine the token length or prefix by measuring response latency.

### Recommendation

- Enable auth whenever the vault is reachable from any process other than a single trusted caller.
- Use a high-entropy token (≥ 32 random bytes, base64-encoded): `openssl rand -base64 32`.
- Pass the token via `VAULT__SERVER__AUTH_TOKEN` environment variable or the `--auth-token` CLI flag rather than a config file (reduces risk of committing it to source control).

---

## Storage Concurrency Safety

The vault file is protected by two independent locks:

```
                 ┌───────────────────────────────────────┐
                 │           LocalFileStore               │
                 │                                       │
  async task 1  ─┤ tokio::sync::Mutex<()>                │
  async task 2  ─┤  (serialises within one process)      │
  async task 3  ─┤                                       │
                 │   └─ spawn_blocking (one at a time)   │
                 │       │                               │
                 │       ▼                               │
                 │   fs2 exclusive file lock             │
                 │    on vault.lock sidecar              │
                 │   (serialises across processes)       │
                 └───────────────────────────────────────┘
```

This two-level approach means:
- **Within a process:** The `tokio::sync::Mutex` prevents concurrent `spawn_blocking` calls from racing. It is a `tokio` mutex so it yields the executor rather than blocking a thread.
- **Across processes:** The `fs2` exclusive lock prevents two vault instances (e.g. a running server and a CLI invocation) from corrupting each other's data.
- **Crash safety:** Even if a process is killed mid-write, the vault file is replaced atomically via `NamedTempFile::persist` (a rename syscall). The old data remains intact until the rename succeeds.
