# Configuration Reference

Tari Vault loads configuration in priority order (later sources override earlier ones):

```
1. Built-in defaults
2. Config file  (vault_config.toml / .yaml in working directory, or --config <path>)
3. Environment variables  (VAULT__<SECTION>__<KEY>)
4. CLI flags  (highest priority)
```

---

## Complete Configuration File

```toml
# vault_config.toml

[server]
# TCP address the JSON-RPC HTTP server binds to.
# Default: "127.0.0.1:9000"
bind_address = "127.0.0.1:9000"

# Optional Bearer token for HTTP authentication.
# When set, every JSON-RPC request must include:
#   Authorization: Bearer <auth_token>
# Leave empty or omit to disable authentication.
# Recommendation: supply via VAULT__SERVER__AUTH_TOKEN env var instead
#   of committing to this file.
# Default: null (disabled)
auth_token = ""

# Path to a PEM TLS certificate file.
# Required (together with tls_key_path) when bind_address is not a loopback
# address (127.x.x.x or ::1).  Omit for loopback-only deployments.
# Default: null (plain HTTP, loopback only)
# tls_cert_path = "/etc/tari_vault/cert.pem"

# Path to the matching PEM private key file.
# Default: null
# tls_key_path = "/etc/tari_vault/key.pem"

# Allow plain HTTP on a non-loopback address.
# Only enable when TLS is terminated by an external proxy (nginx, Envoy, k8s
# Ingress) and the vault port is not reachable outside the trusted network.
# Default: false
# insecure_no_tls = false

# Maximum allowed serialised size of proof_json in bytes.
# Requests whose proof_json exceeds this limit are rejected with an
# InvalidParameter error (-32006) before encryption or storage occurs.
# The HTTP transport enforces the same cap on the full request body.
# Default: 1048576 (1 MiB)
# max_proof_size_bytes = 1048576

[storage]
# Which storage backend to use: "file" (default) or "sqlite".
# "file"   — JSON file, zero extra dependencies, suitable for low-volume deployments.
# "sqlite" — SQLite WAL mode, O(1) per operation, atomic fetch+delete, recommended
#            for higher throughput or when atomic retrieve is required.
# Default: "file"
# backend = "file"

# Path to the JSON vault file (used when backend = "file").
# The directory is created automatically if it does not exist.
# Default: platform data dir + "tari_vault/vault.json"
#   macOS:   ~/Library/Application Support/tari_vault/vault.json
#   Linux:   ~/.local/share/tari_vault/vault.json
#   Windows: %APPDATA%\tari_vault\vault.json
vault_file = "/var/lib/tari_vault/vault.json"

# Path to the SQLite database file (used when backend = "sqlite").
# The directory is created automatically if it does not exist.
# Default: same directory as vault_file, named "vault.db"
# sqlite_path = "/var/lib/tari_vault/vault.db"

# How often the background cleanup task sweeps for expired proofs, in seconds.
# Set to 0 to disable the automatic background sweep.
# You can still call cleanup() manually at startup or on demand.
# Default: 300 (5 minutes)
cleanup_interval_secs = 300

[logging]
# Log level used when RUST_LOG is not set.
# One of: error | warn | info | debug | trace
# Default: "info"
level = "info"
```

YAML format is also supported (`vault_config.yaml`):

```yaml
server:
  bind_address: "127.0.0.1:9000"
  auth_token: ""          # or omit for null
  # tls_cert_path: "/etc/tari_vault/cert.pem"
  # tls_key_path:  "/etc/tari_vault/key.pem"
  # insecure_no_tls: false
  # max_proof_size_bytes: 1048576

storage:
  # backend: "file"                          # or "sqlite"
  vault_file: "/var/lib/tari_vault/vault.json"
  # sqlite_path: "/var/lib/tari_vault/vault.db"
  cleanup_interval_secs: 300

logging:
  level: info
```

---

## Environment Variables

All variables use the prefix `VAULT__` with `__` as the section separator.

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `VAULT__SERVER__BIND_ADDRESS` | string | `127.0.0.1:9000` | Server bind address |
| `VAULT__SERVER__AUTH_TOKEN` | string | *(none)* | Bearer token; empty string = disabled |
| `VAULT__SERVER__TLS_CERT_PATH` | path | *(none)* | PEM TLS certificate file |
| `VAULT__SERVER__TLS_KEY_PATH` | path | *(none)* | PEM TLS private key file |
| `VAULT__SERVER__INSECURE_NO_TLS` | bool | `false` | Allow plain HTTP on non-loopback (proxy termination only) |
| `VAULT__SERVER__MAX_PROOF_SIZE_BYTES` | integer | `1048576` | Maximum `proof_json` size in bytes; requests over this limit are rejected |
| `VAULT__STORAGE__BACKEND` | string | `file` | Storage backend: `file` or `sqlite` |
| `VAULT__STORAGE__VAULT_FILE` | path | *(platform data dir)* | Path to vault JSON file |
| `VAULT__STORAGE__SQLITE_PATH` | path | *(same dir as vault_file, `vault.db`)* | Path to SQLite database |
| `VAULT__STORAGE__CLEANUP_INTERVAL_SECS` | integer | `300` | Cleanup sweep interval; `0` = disabled |
| `VAULT__LOGGING__LEVEL` | string | `info` | Log level (fallback when `RUST_LOG` not set) |
| `RUST_LOG` | string | *(none)* | Standard `tracing-subscriber` filter (takes priority over `VAULT__LOGGING__LEVEL`) |

Environment variables are loaded **after** the config file, so they override file values.

A `.env` file in the working directory is loaded automatically (using the `dotenv` crate). This is useful during development:

```bash
# .env  (do not commit to version control)
VAULT__SERVER__AUTH_TOKEN=dev-token-abc123
VAULT__STORAGE__VAULT_FILE=/tmp/vault-dev.json
VAULT__LOGGING__LEVEL=debug
```

---

## CLI Flags

CLI flags override everything else (highest priority).

| Flag | Type | Env equivalent | Description |
|------|------|----------------|-------------|
| `-c, --config <FILE>` | path | — | Explicit config file path (TOML or YAML) |
| `--vault-file <FILE>` | path | `VAULT__STORAGE__VAULT_FILE` | Vault file path (file backend) |
| `--sqlite-path <FILE>` | path | `VAULT__STORAGE__SQLITE_PATH` | SQLite database path (sqlite backend) |
| `--bind <ADDR>` | string | `VAULT__SERVER__BIND_ADDRESS` | Bind address (e.g. `0.0.0.0:9443`) |
| `--cleanup-interval <SECS>` | integer | `VAULT__STORAGE__CLEANUP_INTERVAL_SECS` | Cleanup interval; `0` = disabled |
| `--auth-token <TOKEN>` | string | `VAULT__SERVER__AUTH_TOKEN` | Bearer token |
| `--tls-cert <FILE>` | path | `VAULT__SERVER__TLS_CERT_PATH` | PEM TLS certificate (required for non-loopback) |
| `--tls-key <FILE>` | path | `VAULT__SERVER__TLS_KEY_PATH` | PEM TLS private key (required for non-loopback) |
| `--insecure-no-tls` | flag | `VAULT__SERVER__INSECURE_NO_TLS` | Allow plain HTTP on non-loopback (proxy termination only) |
| `--log-level <LEVEL>` | string | `VAULT__LOGGING__LEVEL` | Log level (fallback when `RUST_LOG` not set) |

---

## Defaults

| Setting | Default value | Notes |
|---------|--------------|-------|
| `server.bind_address` | `127.0.0.1:9000` | Loopback only — must set explicitly for external access |
| `server.auth_token` | *(none / null)* | Auth disabled by default |
| `server.tls_cert_path` | *(none / null)* | Required when binding to a non-loopback address |
| `server.tls_key_path` | *(none / null)* | Required when binding to a non-loopback address |
| `server.insecure_no_tls` | `false` | Bypass TLS requirement; only for external-proxy deployments |
| `server.max_proof_size_bytes` | `1048576` | Maximum `proof_json` size in bytes (1 MiB) |
| `storage.backend` | `file` | `file` or `sqlite` |
| `storage.vault_file` | `<platform_data_dir>/tari_vault/vault.json` | Created on first run (file backend) |
| `storage.sqlite_path` | same dir as `vault_file`, named `vault.db` | Created on first run (sqlite backend) |
| `storage.cleanup_interval_secs` | `300` | 5 minutes |
| `logging.level` | `info` | Fallback when `RUST_LOG` is not set |

---

## Log Level Configuration

Logging uses `tracing-subscriber` with `EnvFilter`. `RUST_LOG` takes priority over `--log-level` / `VAULT__LOGGING__LEVEL`.

```bash
# Enable debug logging for all tari_vault targets
RUST_LOG=tari_vault=debug ./tari_vault

# Fine-grained per-target control
RUST_LOG=tari_vault::vault=debug,tari_vault=info ./tari_vault

# Inspect jsonrpsee internal spans
RUST_LOG=tari_vault=info,jsonrpsee=debug ./tari_vault

# Fallback level via config (used when RUST_LOG is absent)
./tari_vault --log-level debug
```

---

## Security Recommendations

**Never commit `auth_token` to source control.** Supply it via environment variable:

```bash
# Generate a high-entropy token
export VAULT__SERVER__AUTH_TOKEN="$(openssl rand -base64 32)"
./tari_vault
```

**Restrict vault file permissions.** On Unix, the vault file is automatically created with `0600` permissions (owner read/write only). Ensure the directory is also restricted:

```bash
install -d -m 700 /var/lib/tari_vault
```

**Bind to loopback for local-only deployments.** The default `127.0.0.1:9000` is intentionally not world-accessible. If you need external access, enable TLS with `--tls-cert` and `--tls-key` — the vault will refuse to start on a non-loopback address without TLS. Alternatively, terminate TLS at a reverse proxy (nginx, Caddy) and keep the vault bound to loopback.
