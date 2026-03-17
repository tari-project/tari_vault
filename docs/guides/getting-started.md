# Getting Started

This guide covers building the vault binary, running it, and making your first JSON-RPC calls.

---

## Prerequisites

| Requirement | Version | Notes |
|-------------|---------|-------|
| Rust toolchain | 1.75+ | Required for native async traits (RPITIT) |
| cargo | bundled with Rust | |

No external runtime dependencies. The binary is fully self-contained.

---

## Build

```bash
# Debug build (fast compile, slower runtime)
cargo build

# Release build (optimised, recommended for production)
cargo build --release
```

The binary is at `target/debug/tari_vault` or `target/release/tari_vault`.

Using the Makefile:

```bash
make build          # debug
make build-release  # release
```

---

## Run with Defaults

```bash
cargo run
# or
./target/debug/tari_vault
```

Default settings:

| Setting | Default |
|---------|---------|
| Bind address | `127.0.0.1:9000` |
| Vault file | Platform data dir + `tari_vault/vault.json` |
| Auth | Disabled |
| Cleanup interval | 300 seconds (5 minutes) |
| Log level | `info` |

---

## CLI Flags

```
USAGE:
    tari_vault [OPTIONS]

OPTIONS:
    -c, --config <FILE>              Path to TOML or YAML config file
        --vault-file <FILE>          Override the vault storage file path
        --bind <ADDR>                Override the server bind address [e.g. 127.0.0.1:9000]
        --cleanup-interval <SECS>    Cleanup interval in seconds (0 = disabled)
        --auth-token <TOKEN>         Bearer token for HTTP authentication
        --tls-cert <FILE>            Path to TLS certificate (PEM); required for non-loopback
        --tls-key <FILE>             Path to TLS private key (PEM); required for non-loopback
        --insecure-no-tls            Allow plain HTTP on non-loopback (external-proxy deployments only)
        --log-level <LEVEL>          Log level: error|warn|info|debug|trace [default: info]
    -h, --help                       Print help
    -V, --version                    Print version
```

Example — run on a custom port with TLS and auth enabled:

```bash
./tari_vault \
  --bind 0.0.0.0:9443 \
  --tls-cert /etc/tari_vault/cert.pem \
  --tls-key  /etc/tari_vault/key.pem \
  --auth-token "$(openssl rand -base64 32)"
```

> **Note:** Binding to a non-loopback address without `--tls-cert` / `--tls-key` is a hard error.
> The `Claim_ID` embeds the AES-256-GCM key in the response body; plain HTTP on a reachable
> address would expose every key to passive network observers.

### Behind a reverse proxy (k8s, docker-compose, nginx)

When TLS is terminated externally, pass `--insecure-no-tls` to allow plain HTTP
on a non-loopback address. The vault will log a warning at startup as a reminder.

```bash
# docker-compose / k8s — nginx/Envoy handles TLS, vault is cluster-internal
./tari_vault \
  --bind 0.0.0.0:9000 \
  --insecure-no-tls \
  --auth-token "$VAULT_AUTH_TOKEN"
```

Or via environment variable:

```bash
VAULT__SERVER__BIND_ADDRESS=0.0.0.0:9000
VAULT__SERVER__INSECURE_NO_TLS=true
VAULT__SERVER__AUTH_TOKEN=<token>
```

> **Warning:** Only use `--insecure-no-tls` when the vault port is not reachable
> outside the trusted network. Always combine it with `--auth-token`.

---

## Quick Example with curl

Start the server (in one terminal):

```bash
cargo run
```

In another terminal:

### Store a proof

```bash
curl -s -X POST http://127.0.0.1:9000 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "vault_storeProof",
    "params": [{
      "proof_json": {"root": "a1b2c3", "path": [1, 2, 3], "leaf": "deadbeef"},
      "expires_in_secs": 3600
    }],
    "id": 1
  }'
```

Response:

```json
{
  "jsonrpc": "2.0",
  "result": "Lz8ZpE-I3JHalM_WcFRBBJBH3o5bqsUXIjkNFhVjP9qOmxKMCpw6VYzS9lCEfT5A",
  "id": 1
}
```

Save the `result` value — that is your `Claim_ID`.

### Retrieve the proof

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

Response:

```json
{
  "jsonrpc": "2.0",
  "result": {
    "proof_json": {"root": "a1b2c3", "path": [1, 2, 3], "leaf": "deadbeef"}
  },
  "id": 2
}
```

### Abort (delete without retrieving)

```bash
curl -s -X POST http://127.0.0.1:9000 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "vault_deleteProof",
    "params": ["<claim_id>"],
    "id": 3
  }'
```

### Discover the API spec

```bash
curl -s -X POST http://127.0.0.1:9000 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"rpc.discover","params":[],"id":1}'
```

---

## With Bearer Token Authentication

Start the server:

```bash
export VAULT_AUTH_TOKEN="my-secret-token"
./tari_vault --auth-token "$VAULT_AUTH_TOKEN"
```

All requests must include the header:

```bash
curl -s -X POST http://127.0.0.1:9000 \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer my-secret-token" \
  -d '{"jsonrpc":"2.0","method":"vault_storeProof","params":[{"proof_json":"hello"}],"id":1}'
```

A request without the header returns:

```
HTTP/1.1 401 Unauthorized
WWW-Authenticate: Bearer realm="tari_vault"
```

---

## Configuration File

Create `vault_config.toml` in the working directory (auto-discovered):

```toml
[server]
bind_address = "127.0.0.1:9000"
auth_token = ""          # leave empty to disable auth

[storage]
vault_file = "/var/lib/tari_vault/vault.json"
cleanup_interval_secs = 300

[logging]
level = "info"   # fallback when RUST_LOG is not set
```

Or YAML (`vault_config.yaml`):

```yaml
server:
  bind_address: "127.0.0.1:9000"
  auth_token: ""

storage:
  vault_file: "/var/lib/tari_vault/vault.json"
  cleanup_interval_secs: 300

logging:
  level: info
```

For full configuration reference see [configuration.md](configuration.md).

---

## Verify Everything Works

```bash
make ci
```

This runs formatting check, Clippy linting, all tests, and OpenRPC spec validation.
