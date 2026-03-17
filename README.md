# Tari Vault

A secure, single-use encrypted handoff service for L1 Merkle Proofs. Built in Rust with AES-256-GCM and JSON-RPC 2.0.

---

## The Problem

During an L1→L2 bridge, the L1 side produces a Merkle Proof that the L2 wallet needs to finalise the transaction. If this proof travels through untrusted channels — AI agents, orchestration pipelines, message queues — any participant in that chain can read it.

Tari Vault eliminates this risk. The proof is encrypted with a freshly generated key that **never touches disk**. Only the holder of the returned `Claim_ID` token can decrypt it, exactly once.

## How It Works

```
L1 Sender ──vault_storeProof──► Vault ──Claim_ID──► L1 Sender
                                                         │
                              (any untrusted channel) ◄──┘
                                                         │
L2 Wallet ◄──proof_json────── Vault ◄──vault_retrieveProof──
```

**Key-in-the-ID pattern** — the `Claim_ID` is a 64-character base64url token encoding both the storage lookup key and the AES-256-GCM decryption key:

```
base64url_nopad( record_id[16 bytes] || encryption_key[32 bytes] )
```

The vault stores only ciphertext. Without the `Claim_ID`, stored data is useless. Claims are single-use: the record is deleted on first retrieval.

---

## Quick Start

**Prerequisites:** Rust 1.75+ (required for native async traits).

```bash
# Build
cargo build --release

# Run with defaults (binds to 127.0.0.1:9000, no auth)
./target/release/tari_vault
```

### Store a proof

```bash
curl -s -X POST http://127.0.0.1:9000 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "vault_storeProof",
    "params": [{"proof_json": {"root": "a1b2c3", "path": [1,2,3]}, "expires_in_secs": 3600}],
    "id": 1
  }'
```

```json
{"jsonrpc": "2.0", "result": "Lz8ZpE-I3JHalM_WcFRBBJBH3o5bqsUXIjkNFhVjP9qOmxKMCpw6VYzS9lCEfT5A", "id": 1}
```

### Retrieve the proof (single-use)

```bash
curl -s -X POST http://127.0.0.1:9000 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc": "2.0", "method": "vault_retrieveProof", "params": ["<Claim_ID>"], "id": 2}'
```

See [Getting Started](docs/guides/getting-started.md) for the full walkthrough including auth and abort flows.

---

## API

All methods are JSON-RPC 2.0 over HTTP POST. The machine-readable spec is served at runtime via `rpc.discover`.

| Method | Parameters | Returns |
|--------|-----------|---------|
| `vault_storeProof` | `{proof_json, expires_in_secs?}` | `Claim_ID` string |
| `vault_retrieveProof` | `Claim_ID` | `{proof_json}` |
| `vault_deleteProof` | `Claim_ID` | `null` |
| `rpc.discover` | — | OpenRPC spec |

**Error codes:** `-32001` NotFound · `-32002` Expired · `-32003` InvalidClaimId · `-32004` DecryptionFailed · `-32005` Internal · `-32006` InvalidParameter

Full reference: [JSON-RPC Reference](docs/api/json-rpc-reference.md)

---

## Configuration

Configuration is layered, lowest to highest priority:

```
built-in defaults → vault_config.toml → environment variables → CLI flags
```

```toml
# vault_config.toml
[server]
bind_address = "127.0.0.1:9000"
auth_token = ""                         # empty = auth disabled

[storage]
backend = "file"                        # "file" (default) or "sqlite"
vault_file = "/var/lib/tari_vault/vault.json"
# sqlite_path = "/var/lib/tari_vault/vault.db"  # used when backend = "sqlite"
cleanup_interval_secs = 300             # 0 = disabled

[logging]
level = "info"
```

Environment variables use a `VAULT__` prefix: `VAULT__SERVER__BIND_ADDRESS`, `VAULT__SERVER__AUTH_TOKEN`, etc.

CLI flags: `--bind`, `--vault-file`, `--sqlite-path`, `--auth-token`, `--cleanup-interval`, `--log-level`, `--config`.

Full reference: [Configuration Guide](docs/guides/configuration.md)

---

## Security

- **AES-256-GCM** encryption with a per-proof random key and nonce
- **Key never persisted** — the decryption key exists only in RAM and in the `Claim_ID`
- **Bearer token auth** with constant-time comparison (`subtle::ConstantTimeEq`) enforced at the HTTP layer, before RPC parsing
- **ZeroizeOnDrop** on all sensitive types (`PlaintextProof`, `ClaimId`, intermediate buffers)
- **Two storage backends** — file (`LocalFileStore`: atomic writes via `NamedTempFile::persist()`, dual-lock with `fd-lock`) or SQLite (`SqliteStore`: WAL mode, O(1) ops, `secure_delete`); selected via `storage.backend` config
- **Generic crypto error messages** — all AES-GCM failures return `DecryptionFailed` with no detail

Details: [Security Model](docs/architecture/security-model.md)

---

## Library Use

Tari Vault is also a library crate. Embed it directly in a `walletd` daemon or any Tokio application:

```toml
[dependencies]
tari_vault = { path = "../tari_vault" }
```

See [Library Integration](docs/guides/library-integration.md) and the [Axum example](docs/guides/library-integration-axum.md).

---

## Development

```bash
make build          # debug build
make build-release  # release build
make test           # all tests (unit + integration + doctests)
make lint           # clippy -D warnings
make fmt            # format
make ci             # full CI: fmt-check + lint + test + openrpc validation
```

Single test: `cargo test <test_name>`

See [Contributing](docs/development/contributing.md) for guidelines.

---

## Documentation

| Document | Description |
|----------|-------------|
| [Getting Started](docs/guides/getting-started.md) | Build, run, and first API calls |
| [Configuration](docs/guides/configuration.md) | All configuration options |
| [JSON-RPC Reference](docs/api/json-rpc-reference.md) | Full API reference |
| [Architecture Overview](docs/architecture/overview.md) | System design and component map |
| [Data Flows](docs/architecture/data-flows.md) | Request lifecycle diagrams |
| [Security Model](docs/architecture/security-model.md) | Threat model and cryptographic guarantees |
| [Library Integration](docs/guides/library-integration.md) | Embedding as a library |
