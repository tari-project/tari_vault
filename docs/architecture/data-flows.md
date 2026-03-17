# Data Flows

Sequence diagrams for every significant operation in Tari Vault.

---

## Store Proof (`vault_storeProof`)

```mermaid
sequenceDiagram
    autonumber
    participant S as Sender (L1 Bridge)
    participant A as BearerAuthLayer
    participant R as VaultRpcImpl
    participant V as StandardVault
    participant D as LocalFileStore (disk)

    S->>A: POST / {"method":"vault_storeProof","params":[{proof_json, expires_in_secs}]}
    A->>A: Check Authorization: Bearer header (ConstantTimeEq)
    A->>R: Forward (or reject with HTTP 401)

    R->>R: PlaintextProof::from_json(proof_json)
    R->>V: store_proof(proof, expires_in_secs)

    V->>V: OsRng → AES-256 key (32 bytes)
    V->>V: OsRng → AES-GCM nonce (12 bytes)
    V->>V: AES-256-GCM encrypt(proof.as_bytes(), key, nonce)
    V->>V: drop(proof)  ← ZeroizeOnDrop wipes plaintext
    V->>V: ClaimId::new(key) → record_id (UUIDv4) + key
    V->>V: key.zeroize()  ← wipe GenericArray (Copy type!)
    V->>V: Build StoredRecord {nonce, ciphertext, stored_at, expires_at}

    V->>D: insert(record_id, StoredRecord)
    D->>D: acquire process lock (tokio Mutex)
    D->>D: acquire file lock (fs2 exclusive)
    D->>D: read current JSON state
    D->>D: insert new record
    D->>D: write_atomic → NamedTempFile + rename
    D->>D: chmod 0600 (Unix)
    D-->>V: Ok(())

    V->>V: ClaimId::encode() → base64url_nopad(record_id || key)
    V->>V: drop(claim_id)  ← ZeroizeOnDrop wipes key from RAM
    V-->>R: Ok(claim_id_string)
    R-->>S: {"result": "64-char-claim-id-string"}

    Note over S: Sender passes Claim_ID to Receiver<br/>through any channel (AI agent, queue, etc.)
    Note over D: Disk contains: record_id → {nonce, ciphertext, timestamps}<br/>No key, no plaintext
```

---

## Retrieve Proof (`vault_retrieveProof`)

```mermaid
sequenceDiagram
    autonumber
    participant Re as Receiver (L2 Wallet)
    participant A as BearerAuthLayer
    participant R as VaultRpcImpl
    participant V as StandardVault
    participant D as LocalFileStore (disk)

    Re->>A: POST / {"method":"vault_retrieveProof","params":["<Claim_ID>"]}
    A->>A: Check Authorization: Bearer header
    A->>R: Forward

    R->>V: retrieve_proof(claim_id_str)

    V->>V: ClaimId::decode(claim_id_str)
    Note over V: Zeroizing<Vec<u8>> wipes raw bytes<br/>after split into record_id + key

    V->>D: fetch(record_id)
    D-->>V: StoredRecord {nonce, ciphertext, expires_at, …}

    V->>V: is_expired()? → if yes: delete + return ProofExpired
    V->>V: validate nonce length == 12

    V->>V: AES-256-GCM decrypt(ciphertext, key, nonce)
    Note over V: Wrong key → GCM auth tag mismatch<br/>→ DecryptionFailed (generic error)

    V->>D: delete(record_id)  ← single-use: consumed immediately
    D->>D: acquire locks → remove record → write_atomic
    D-->>V: Ok(())

    V->>V: drop(claim_id)  ← ZeroizeOnDrop wipes key
    V-->>R: Ok(PlaintextProof { plaintext_bytes })

    R->>R: proof.into_json()  ← zeroizes bytes after parse
    R-->>Re: {"result": {"proof_json": { … }}}

    Note over D: Record no longer exists on disk<br/>A second call returns ProofNotFound
```

---

## Delete Proof (`vault_deleteProof`)

Abort / cancel flow: the Claim_ID holder discards an unclaimed proof.

```mermaid
sequenceDiagram
    autonumber
    participant C as Caller (aborting party)
    participant A as BearerAuthLayer
    participant R as VaultRpcImpl
    participant V as StandardVault
    participant D as LocalFileStore (disk)

    C->>A: POST / {"method":"vault_deleteProof","params":["<Claim_ID>"]}
    A->>A: Check Authorization: Bearer header
    A->>R: Forward

    R->>V: delete_proof(claim_id_str)
    V->>V: ClaimId::decode(claim_id_str)
    Note over V: Requires full Claim_ID (includes key)<br/>record_id alone is insufficient

    V->>D: fetch(record_id)  ← verify existence
    alt proof not found
        D-->>V: Err(NotFound)
        V-->>R: Err(ProofNotFound)
        R-->>C: {"error":{"code":-32001,"message":"Proof not found"}}
    else proof exists
        D-->>V: Ok(StoredRecord)
        V->>D: delete(record_id)
        D-->>V: Ok(())
        V->>V: drop(claim_id)  ← ZeroizeOnDrop
        V-->>R: Ok(())
        R-->>C: {"result": null}
    end
```

---

## Periodic Cleanup Sweep

Background task that purges expired proofs.

```mermaid
sequenceDiagram
    autonumber
    participant M as main()
    participant CT as CleanupTask
    participant V as StandardVault
    participant D as LocalFileStore (disk)

    M->>CT: spawn_cleanup_task(vault, interval, shutdown_token)
    Note over CT: Child CancellationToken created<br/>stopping the task does NOT cancel parent

    loop Every `interval` seconds
        CT->>CT: ticker.tick() [MissedTickBehavior::Skip]
        CT->>V: cleanup()
        V->>D: delete_expired()
        D->>D: acquire locks
        D->>D: read state
        D->>D: retain only non-expired records
        D->>D: write_atomic (only if any removed)
        D-->>V: Ok(n_removed)
        V-->>CT: Ok(n_removed)
        CT->>CT: log if n_removed > 0
    end

    Note over M: On shutdown (Ctrl-C)
    M->>CT: shutdown_token.cancel()
    CT->>CT: biased select: cancelled() wins
    CT-->>M: task exits
```

---

## Server Startup and Graceful Shutdown

```mermaid
sequenceDiagram
    autonumber
    participant OS as OS / User
    participant M as main()
    participant S as RPC Server
    participant CT as CleanupTask

    OS->>M: ./tari_vault [--config …]
    M->>M: load_config()  layered: defaults → file → env → CLI
    M->>M: init_logging() log4rs
    M->>M: LocalFileStore::new(vault_file)
    M->>M: Arc::new(StandardVault::new(storage))
    M->>M: vault.cleanup()  startup sweep: clear leftover expired proofs
    M->>CT: spawn_cleanup_task(vault, interval, shutdown_token)
    M->>S: start_server(bind_addr, vault, auth_token)
    Note over S: Applies BearerAuthLayer → jsonrpsee server<br/>Merges vault_* + rpc.discover modules
    S-->>M: (SocketAddr, ServerHandle)
    M->>OS: log "Vault RPC server listening on …"

    OS->>M: Ctrl-C (SIGINT)
    M->>S: server_handle.stop()
    S-->>M: server_handle.stopped().await
    M->>M: log "RPC server stopped"
    M->>CT: shutdown_token.cancel()
    CT-->>M: task.stopped().await
    M->>M: log "Shutdown complete"
    M-->>OS: exit 0
```

---

## ClaimId Encoding

How 48 bytes become a 64-character token.

```
Input:
  record_id       = [0x3f, 0x25, 0x04, 0xe0, … 16 bytes total] (UUIDv4)
  encryption_key  = [0x7a, 0x3b, 0xc1, 0xd4, … 32 bytes total] (AES-256 key)

Step 1 – Concatenate (in Zeroizing<[u8; 48]> buffer):
  bytes = record_id[0..16] || encryption_key[0..32]
        = 48 bytes

Step 2 – Base64url encode (RFC 4648 §5, no padding):
  Claim_ID = base64url_nopad(bytes)
           = exactly 64 characters
           ┌────────────────────────┬────────────────────────────────────────┐
           │ chars  0 – 21  (22 ch) │ chars 22 – 63  (42 ch)                 │
           │ encodes record_id[16]  │ encodes encryption_key[32]              │
           └────────────────────────┴────────────────────────────────────────┘

Step 3 – Zeroizing buffer is dropped, wiping the 48-byte concatenation.

Decode:
  base64url_nopad decode → Zeroizing<Vec<u8>> (48 bytes)
  split: bytes[0..16] → record_id, bytes[16..48] → encryption_key
  Zeroizing<Vec<u8>> dropped → raw bytes wiped
```
