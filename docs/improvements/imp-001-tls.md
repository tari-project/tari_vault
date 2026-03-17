# IMP-001: Add TLS Support

**Status:** `[ ]` Planned
**Tier:** 1 — Security
**Priority:** Critical

## Problem

The server binds as a plain HTTP listener. The `Claim_ID` returned to callers embeds the full 32-byte AES-256-GCM encryption key encoded in base64url. This key travels over the network in plaintext on every `storeProof` and `retrieveProof` call. A passive network observer capturing a single request can decrypt any proof stored in the vault file.

The current design assumes co-location (localhost or private network), but this constraint is neither enforced at runtime nor documented as a deployment requirement.

## Goal

Encrypt all JSON-RPC traffic in transit. The `Claim_ID` must never appear on the wire in plaintext.

## Proposed Approach

- Add optional TLS configuration to `ServerConfig`: `tls_cert_path` and `tls_key_path` (PEM files).
- Use `rustls` via `tokio-rustls` or the `axum-server` TLS integration.
- If TLS is disabled and `bind_address` is not a loopback address, emit a startup warning (or hard error in release builds).
- Document deployment requirement: TLS is mandatory for any non-localhost binding.

## Affected Files

- `src/config.rs` — new TLS config fields
- `src/rpc/server.rs` — conditional TLS acceptor
- `Cargo.toml` — `rustls`, `tokio-rustls` dependencies
- `docs/guides/getting-started.md` — deployment notes

## Notes

- `rustls` is preferred over `openssl` for memory safety and no C dependency.
- Self-signed certificates are acceptable for localhost deployments; the configuration guide should cover both self-signed and CA-issued certificate setup.
