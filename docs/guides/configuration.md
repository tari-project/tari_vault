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

[storage]
# Path to the JSON vault file.
# The directory is created automatically if it does not exist.
# Default: platform data dir + "tari_vault/vault.json"
#   macOS:   ~/Library/Application Support/tari_vault/vault.json
#   Linux:   ~/.local/share/tari_vault/vault.json
#   Windows: %APPDATA%\tari_vault\vault.json
vault_file = "/var/lib/tari_vault/vault.json"

# How often the background cleanup task sweeps for expired proofs, in seconds.
# Set to 0 to disable the automatic background sweep.
# You can still call cleanup() manually at startup or on demand.
# Default: 300 (5 minutes)
cleanup_interval_secs = 300

[logging]
# Fallback log level when config_file is absent.
# One of: error | warn | info | debug | trace
# Default: "info"
level = "info"

# Optional path to a log4rs YAML configuration file.
# When provided, the `level` field above is ignored.
# Default: null (use built-in console appender)
# config_file = "/etc/tari_vault/log4rs.yaml"
```

YAML format is also supported (`vault_config.yaml`):

```yaml
server:
  bind_address: "127.0.0.1:9000"
  auth_token: ""          # or omit for null

storage:
  vault_file: "/var/lib/tari_vault/vault.json"
  cleanup_interval_secs: 300

logging:
  level: info
  # config_file: "/etc/tari_vault/log4rs.yaml"
```

---

## Environment Variables

All variables use the prefix `VAULT__` with `__` as the section separator.

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `VAULT__SERVER__BIND_ADDRESS` | string | `127.0.0.1:9000` | Server bind address |
| `VAULT__SERVER__AUTH_TOKEN` | string | *(none)* | Bearer token; empty string = disabled |
| `VAULT__STORAGE__VAULT_FILE` | path | *(platform data dir)* | Path to vault JSON file |
| `VAULT__STORAGE__CLEANUP_INTERVAL_SECS` | integer | `300` | Cleanup sweep interval; `0` = disabled |
| `VAULT__LOGGING__LEVEL` | string | `info` | Log level |
| `VAULT__LOGGING__CONFIG_FILE` | path | *(none)* | log4rs YAML config path |

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
| `--vault-file <FILE>` | path | `VAULT__STORAGE__VAULT_FILE` | Vault file path |
| `--bind <ADDR>` | string | `VAULT__SERVER__BIND_ADDRESS` | Bind address (e.g. `0.0.0.0:9001`) |
| `--cleanup-interval <SECS>` | integer | `VAULT__STORAGE__CLEANUP_INTERVAL_SECS` | Cleanup interval; `0` = disabled |
| `--auth-token <TOKEN>` | string | `VAULT__SERVER__AUTH_TOKEN` | Bearer token |
| `--log-config <FILE>` | path | `VAULT__LOGGING__CONFIG_FILE` | log4rs YAML config |
| `--log-level <LEVEL>` | string | `VAULT__LOGGING__LEVEL` | Log level |

---

## Defaults

| Setting | Default value | Notes |
|---------|--------------|-------|
| `server.bind_address` | `127.0.0.1:9000` | Loopback only — must set explicitly for external access |
| `server.auth_token` | *(none / null)* | Auth disabled by default |
| `storage.vault_file` | `<platform_data_dir>/tari_vault/vault.json` | Created on first run |
| `storage.cleanup_interval_secs` | `300` | 5 minutes |
| `logging.level` | `info` | |
| `logging.config_file` | *(none)* | Built-in console appender used |

---

## Log4rs Custom Configuration

Supply a log4rs YAML file for file rotation, structured JSON output, multiple appenders, etc.

```yaml
# log4rs.yaml
appenders:
  console:
    kind: console
    encoder:
      pattern: "{d(%Y-%m-%dT%H:%M:%S%.3fZ)(utc)} {h({l:<5})} {t} — {m}{n}"
  file:
    kind: rolling_file
    path: "/var/log/tari_vault/vault.log"
    policy:
      kind: compound
      trigger:
        kind: size
        limit: 50 mb
      roller:
        kind: fixed_window
        pattern: "/var/log/tari_vault/vault.{}.log.gz"
        base: 1
        count: 5
    encoder:
      pattern: "{d(%Y-%m-%dT%H:%M:%S%.3fZ)(utc)} {l:<5} {t} — {m}{n}"

root:
  level: info
  appenders:
    - console
    - file

loggers:
  tari_vault:
    level: debug
    appenders:
      - file
    additive: false
```

Reference with:

```bash
./tari_vault --log-config log4rs.yaml
# or
VAULT__LOGGING__CONFIG_FILE=log4rs.yaml ./tari_vault
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

**Bind to loopback for local-only deployments.** The default `127.0.0.1:9000` is intentionally not world-accessible. If you need external access, use a reverse proxy (nginx, Caddy) with TLS rather than binding directly to `0.0.0.0`.
