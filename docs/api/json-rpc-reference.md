# JSON-RPC API Reference

Tari Vault exposes a [JSON-RPC 2.0](https://www.jsonrpc.org/specification) API over HTTP POST.

A machine-readable [OpenRPC](https://spec.open-rpc.org/) spec is available:
- **File:** [`openrpc.json`](../../openrpc.json) at the project root.
- **At runtime:** call `rpc.discover` (see [below](#rpcdiscover)).
- **Interactive:** paste `openrpc.json` into the [OpenRPC Playground](https://playground.open-rpc.org/).

---

## Transport

| Property | Value |
|----------|-------|
| Protocol | HTTP/1.1 |
| Method | `POST` |
| Path | `/` |
| Content-Type | `application/json` |
| Default address | `http://127.0.0.1:9000` |

All requests use the JSON-RPC 2.0 envelope:

```json
{
  "jsonrpc": "2.0",
  "method": "<method_name>",
  "params": [<positional_args>],
  "id": <integer_or_string>
}
```

Responses:

```json
{ "jsonrpc": "2.0", "result": <value>, "id": <id> }
{ "jsonrpc": "2.0", "error": { "code": <int>, "message": "<str>" }, "id": <id> }
```

---

## Authentication

When bearer-token authentication is enabled, include the header on every request:

```
Authorization: Bearer <token>
```

Missing or wrong token → `HTTP 401 Unauthorized` (before JSON-RPC parsing):

```
HTTP/1.1 401 Unauthorized
WWW-Authenticate: Bearer realm="tari_vault"
```

Authentication is **disabled by default**. See [configuration.md](../guides/configuration.md) to enable it.

---

## Methods

### `vault_storeProof`

Encrypt and store a proof. Returns a single-use `Claim_ID` token.

**Params (positional):**

```json
[
  {
    "proof_json": <any JSON value>,
    "expires_in_secs": <integer | null>
  }
]
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `proof_json` | any JSON | yes | The proof. Any JSON type: object, string, number, array, boolean. |
| `expires_in_secs` | integer ≥ 0 | no | TTL in seconds from now. `null` or omit = never expires. |

**Result:** `string` — 64-character base64url `Claim_ID` token.

**Errors:** `-32005` (internal storage error)

**Example:**

```bash
curl -s -X POST http://127.0.0.1:9000 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "vault_storeProof",
    "params": [{
      "proof_json": {
        "root": "a1b2c3d4e5f67890",
        "path": [
          {"hash": "aabb", "direction": "left"},
          {"hash": "ccdd", "direction": "right"}
        ],
        "leaf": "deadbeef"
      },
      "expires_in_secs": 3600
    }],
    "id": 1
  }'
```

```json
{
  "jsonrpc": "2.0",
  "result": "Lz8ZpE-I3JHalM_WcFRBBJBH3o5bqsUXIjkNFhVjP9qOmxKMCpw6VYzS9lCEfT5A",
  "id": 1
}
```

---

### `vault_retrieveProof`

Retrieve and **consume** a stored proof. Single-use: the record is deleted on success.

**Params (positional):**

```json
["<claim_id>"]
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `claim_id` | string (64 chars) | yes | The `Claim_ID` returned by `vault_storeProof`. |

**Result:**

```json
{
  "proof_json": <original JSON value>
}
```

**Errors:**

| Code | Name | Condition |
|------|------|-----------|
| `-32001` | `ProofNotFound` | Token not found (consumed, deleted, or never stored). |
| `-32002` | `ProofExpired` | TTL elapsed; proof was deleted. |
| `-32003` | `InvalidClaimId` | Token is not valid base64url or wrong length. |
| `-32004` | `DecryptionFailed` | AES-GCM authentication tag mismatch (wrong key or corrupted ciphertext). |
| `-32005` | `InternalError` | Storage or serialisation error. |

**Example:**

```bash
curl -s -X POST http://127.0.0.1:9000 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "vault_retrieveProof",
    "params": ["Lz8ZpE-I3JHalM_WcFRBBJBH3o5bqsUXIjkNFhVjP9qOmxKMCpw6VYzS9lCEfT5A"],
    "id": 2
  }'
```

```json
{
  "jsonrpc": "2.0",
  "result": {
    "proof_json": {
      "root": "a1b2c3d4e5f67890",
      "path": [
        {"hash": "aabb", "direction": "left"},
        {"hash": "ccdd", "direction": "right"}
      ],
      "leaf": "deadbeef"
    }
  },
  "id": 2
}
```

**Calling again with the same `Claim_ID`:**

```json
{
  "jsonrpc": "2.0",
  "error": { "code": -32001, "message": "Proof not found" },
  "id": 3
}
```

---

### `vault_deleteProof`

**Abort / cancel** a stored proof without decrypting it.

Use this when a bridge operation is abandoned before the receiver has claimed the proof (e.g. the L1 burn was rolled back, the AI agent encountered an error, or the user cancelled). The full `Claim_ID` is required — the storage key alone is insufficient.

**Params (positional):**

```json
["<claim_id>"]
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `claim_id` | string (64 chars) | yes | The `Claim_ID` returned by `vault_storeProof`. |

**Result:** `null`

**Errors:**

| Code | Name | Condition |
|------|------|-----------|
| `-32001` | `ProofNotFound` | Token not found (already consumed, deleted, or never stored). |
| `-32003` | `InvalidClaimId` | Malformed token. |
| `-32005` | `InternalError` | Storage error. |

**Example:**

```bash
curl -s -X POST http://127.0.0.1:9000 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "vault_deleteProof",
    "params": ["Lz8ZpE-I3JHalM_WcFRBBJBH3o5bqsUXIjkNFhVjP9qOmxKMCpw6VYzS9lCEfT5A"],
    "id": 4
  }'
```

```json
{
  "jsonrpc": "2.0",
  "result": null,
  "id": 4
}
```

---

### `rpc.discover`

Return the full [OpenRPC](https://spec.open-rpc.org/) document for this service.

This is the standard OpenRPC service-discovery method. Requires the bearer token if auth is enabled.

**Params:** `[]` (none)

**Result:** OpenRPC document object (the contents of `openrpc.json`).

**Example:**

```bash
curl -s -X POST http://127.0.0.1:9000 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"rpc.discover","params":[],"id":1}' \
  | python3 -m json.tool
```

---

## Error Code Reference

### Custom error codes

| Code | Name | Description |
|------|------|-------------|
| `-32001` | `ProofNotFound` | Token does not exist in storage. |
| `-32002` | `ProofExpired` | TTL elapsed; record has been purged. |
| `-32003` | `InvalidClaimId` | Malformed base64url token or wrong length (expected 64 chars). |
| `-32004` | `DecryptionFailed` | AES-GCM decryption failed. Generic message — no oracle information. |
| `-32005` | `InternalError` | Storage I/O error or JSON serialisation error. |

### Standard JSON-RPC error codes

| Code | Name | Description |
|------|------|-------------|
| `-32700` | `ParseError` | Invalid JSON in the request body. |
| `-32600` | `InvalidRequest` | Not a valid JSON-RPC 2.0 request. |
| `-32601` | `MethodNotFound` | Unknown method name. |
| `-32602` | `InvalidParams` | Params do not match the method signature. |
| `-32603` | `InternalError` | Unexpected server error. |

---

## Claim_ID Format

The `Claim_ID` is a 64-character [base64url](https://datatracker.ietf.org/doc/html/rfc4648#section-5) string (no padding):

```
pattern: ^[A-Za-z0-9_-]{64}$
```

It encodes 48 raw bytes:
- bytes `[0, 16)` — UUIDv4 storage key (non-sensitive)
- bytes `[16, 48)` — AES-256-GCM decryption key (**treat as a secret**)

**Security:** anyone who obtains the `Claim_ID` can retrieve the proof exactly once. Treat it like a password — transmit only over encrypted channels if confidentiality of the proof matters end-to-end. Note that the vault itself provides encryption-at-rest; the `Claim_ID` is intentionally designed to be passable through untrusted channels, with the single-use property providing forward security.

---

## Proof JSON Format

The `proof_json` field accepts **any JSON value** — object, string, number, array, or boolean. The vault does not inspect, validate, or transform the value; it is serialised to bytes, encrypted, and returned verbatim on retrieval.

Valid examples:

```json
{"root": "abc", "path": [], "leaf": "def"}   // Merkle proof object
"raw-string-proof"                            // plain string
42                                            // number
["a", "b", "c"]                               // array
```
