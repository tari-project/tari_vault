use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use anyhow::Context;
use jsonrpsee::{
    server::{Methods, Server, ServerConfig, ServerHandle},
    types::ErrorObjectOwned,
};
use tokio::net::{TcpListener, TcpStream};

use zeroize::Zeroizing;

use crate::{
    auth::BearerAuthLayer,
    domain::PlaintextProof,
    error::VaultError,
    rpc::{
        api::{ProofObject, StoreProofParams, VaultRpcServer},
        discovery::discovery_module,
    },
    vault::ProofVault,
};

/// TLS certificate and private key paths for HTTPS.
pub struct TlsConfig {
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
}

/// Concrete JSON-RPC handler that delegates to a `ProofVault` implementation.
pub struct VaultRpcImpl<V> {
    vault: V,
    max_proof_size_bytes: usize,
}

impl<V: ProofVault> VaultRpcImpl<V> {
    pub fn new(vault: V, max_proof_size_bytes: usize) -> Self {
        Self {
            vault,
            max_proof_size_bytes,
        }
    }
}

#[async_trait::async_trait]
impl<V: ProofVault + 'static> VaultRpcServer for VaultRpcImpl<V> {
    async fn store_proof(&self, params: StoreProofParams) -> Result<String, ErrorObjectOwned> {
        let proof_size = params.proof_json.to_string().len();
        if proof_size > self.max_proof_size_bytes {
            return Err(vault_to_rpc_err(VaultError::InvalidParameter(format!(
                "proof_json exceeds maximum allowed size of {} bytes",
                self.max_proof_size_bytes
            ))));
        }

        let proof = PlaintextProof::from_json(&params.proof_json).map_err(vault_to_rpc_err)?;

        self.vault
            .store_proof(proof, params.expires_in_secs)
            .await
            .map_err(vault_to_rpc_err)
    }

    async fn retrieve_proof(&self, claim_id: String) -> Result<ProofObject, ErrorObjectOwned> {
        let proof = self
            .vault
            .retrieve_proof(Zeroizing::new(claim_id))
            .await
            .map_err(vault_to_rpc_err)?;

        let proof_json = proof.into_json().map_err(vault_to_rpc_err)?;
        Ok(ProofObject { proof_json })
    }

    async fn delete_proof(&self, claim_id: String) -> Result<(), ErrorObjectOwned> {
        self.vault
            .delete_proof(Zeroizing::new(claim_id))
            .await
            .map_err(vault_to_rpc_err)
    }
}

/// Start the JSON-RPC server and return a handle for graceful shutdown.
///
/// When `auth_token` is `Some(token)`, every request must carry an
/// `Authorization: Bearer <token>` header; requests without it receive an
/// HTTP 401 before any RPC processing occurs.  Pass `None` to disable auth.
///
/// When `tls` is `Some(cfg)`, the server listens over HTTPS using the
/// supplied PEM certificate and key.  When `tls` is `None`, the server uses
/// plain HTTP — which is only permitted for loopback addresses (`127.x.x.x`
/// or `::1`).  Attempting plain HTTP on a non-loopback address is a hard
/// error.
pub async fn start_server(
    bind_addr: &str,
    vault: impl ProofVault + 'static,
    auth_token: Option<String>,
    tls: Option<TlsConfig>,
    insecure_no_tls: bool,
    max_proof_size_bytes: usize,
) -> anyhow::Result<(SocketAddr, ServerHandle)> {
    check_tls_for_non_loopback(bind_addr, &tls, insecure_no_tls)?;

    let auth = BearerAuthLayer::from_config(auth_token);
    let middleware = tower::ServiceBuilder::new().layer(auth);

    let mut rpc_module = VaultRpcImpl::new(vault, max_proof_size_bytes).into_rpc();
    rpc_module.merge(discovery_module())?;

    match tls {
        None => start_plain(bind_addr, middleware, rpc_module, max_proof_size_bytes).await,
        Some(tls_cfg) => {
            start_tls(
                bind_addr,
                middleware,
                rpc_module,
                tls_cfg,
                max_proof_size_bytes,
            )
            .await
        }
    }
}

// ── Plain HTTP path ───────────────────────────────────────────────────────────

async fn start_plain(
    bind_addr: &str,
    middleware: tower::ServiceBuilder<
        tower::layer::util::Stack<BearerAuthLayer, tower::layer::util::Identity>,
    >,
    rpc_module: impl Into<Methods>,
    max_proof_size_bytes: usize,
) -> anyhow::Result<(SocketAddr, ServerHandle)> {
    // Add 4 KiB headroom for the JSON-RPC envelope so the transport limit
    // mirrors the semantic limit without rejecting valid requests.
    let http_body_limit: u32 = (max_proof_size_bytes.saturating_add(4096))
        .try_into()
        .unwrap_or(u32::MAX);
    let server_cfg = ServerConfig::builder()
        .max_request_body_size(http_body_limit)
        .build();
    let server = Server::builder()
        .set_config(server_cfg)
        .set_http_middleware(middleware)
        .build(bind_addr)
        .await?;
    let addr = server.local_addr()?;
    let handle = server.start(rpc_module);
    tracing::info!(target: "tari_vault::rpc", "Vault RPC server listening on http://{addr}");
    Ok((addr, handle))
}

// ── TLS path ─────────────────────────────────────────────────────────────────

/// Start a TLS server.
///
/// Internally this starts a plain-HTTP jsonrpsee server on a loopback address
/// and runs a TLS listener in a background task that wraps incoming streams
/// with rustls then proxies raw bytes to the plain server via
/// `tokio::io::copy_bidirectional`.  The auth token travels within the HTTP
/// framing (over loopback) so security is preserved.
async fn start_tls(
    bind_addr: &str,
    middleware: tower::ServiceBuilder<
        tower::layer::util::Stack<BearerAuthLayer, tower::layer::util::Identity>,
    >,
    rpc_module: impl Into<Methods>,
    tls_cfg: TlsConfig,
    max_proof_size_bytes: usize,
) -> anyhow::Result<(SocketAddr, ServerHandle)> {
    let tls_acceptor = build_tls_acceptor(&tls_cfg)?;

    let http_body_limit: u32 = (max_proof_size_bytes.saturating_add(4096))
        .try_into()
        .unwrap_or(u32::MAX);
    let server_cfg = ServerConfig::builder()
        .max_request_body_size(http_body_limit)
        .build();

    // The plain server always binds to loopback so it is not reachable from
    // the outside world.  The auth token check still runs for every request.
    let plain_server = Server::builder()
        .set_config(server_cfg)
        .set_http_middleware(middleware)
        .build("127.0.0.1:0")
        .await?;
    let plain_addr = plain_server.local_addr()?;
    let server_handle = plain_server.start(rpc_module);

    // Bind the public TLS listener.
    let tls_listener = TcpListener::bind(bind_addr).await?;
    let tls_addr = tls_listener.local_addr()?;
    tracing::info!(target: "tari_vault::rpc", "Vault RPC server listening on https://{tls_addr}");

    // Spawn the TLS accept loop.  Each accepted connection is handed off to
    // its own task that performs the TLS handshake and then forwards raw TCP
    // bytes to the plain server via copy_bidirectional.
    tokio::spawn(async move {
        loop {
            let (tcp, peer) = match tls_listener.accept().await {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(target: "tari_vault::rpc", "TLS accept error: {e}");
                    continue;
                }
            };

            let acceptor = tls_acceptor.clone();
            tokio::spawn(async move {
                let mut tls_stream = match acceptor.accept(tcp).await {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!(
                            target: "tari_vault::rpc",
                            "TLS handshake failed from {peer}: {e}"
                        );
                        return;
                    }
                };

                let mut plain_stream = match TcpStream::connect(plain_addr).await {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!(
                            target: "tari_vault::rpc",
                            "Failed to connect to plain server: {e}"
                        );
                        return;
                    }
                };

                if let Err(e) =
                    tokio::io::copy_bidirectional(&mut tls_stream, &mut plain_stream).await
                {
                    tracing::debug!(target: "tari_vault::rpc", "TLS proxy connection closed: {e}");
                }
            });
        }
    });

    Ok((tls_addr, server_handle))
}

// ── TLS helpers ───────────────────────────────────────────────────────────────

/// Load a [`tokio_rustls::TlsAcceptor`] from PEM certificate and key files.
fn build_tls_acceptor(cfg: &TlsConfig) -> anyhow::Result<tokio_rustls::TlsAcceptor> {
    // Ensure a crypto provider is installed (ring by default).
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();

    let cert_bytes = std::fs::read(&cfg.cert_path)
        .with_context(|| format!("failed to read TLS cert: {:?}", cfg.cert_path))?;
    let key_bytes = std::fs::read(&cfg.key_path)
        .with_context(|| format!("failed to read TLS key: {:?}", cfg.key_path))?;

    let certs: Vec<rustls::pki_types::CertificateDer<'static>> =
        rustls_pemfile::certs(&mut cert_bytes.as_slice())
            .collect::<Result<_, _>>()
            .context("failed to parse TLS certificates")?;

    let key = rustls_pemfile::private_key(&mut key_bytes.as_slice())
        .context("failed to parse TLS private key")?
        .context("no private key found in TLS key file")?;

    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .context("failed to build TLS server config")?;

    Ok(tokio_rustls::TlsAcceptor::from(Arc::new(config)))
}

/// Enforce that plain-HTTP servers are only allowed on loopback addresses.
///
/// The Claim_ID embeds the AES-256-GCM encryption key in the response body,
/// so unencrypted traffic on a reachable address would expose every key to
/// passive network observers.
fn check_tls_for_non_loopback(
    bind_addr: &str,
    tls: &Option<TlsConfig>,
    insecure_no_tls: bool,
) -> anyhow::Result<()> {
    if tls.is_some() || insecure_no_tls {
        return Ok(());
    }
    let addr: SocketAddr = bind_addr
        .parse()
        .with_context(|| format!("invalid bind address: {bind_addr}"))?;
    if !addr.ip().is_loopback() {
        anyhow::bail!(
            "TLS is disabled but the bind address ({bind_addr}) is not a loopback address. \
             Provide --tls-cert and --tls-key to enable TLS, bind to 127.0.0.1 / ::1, \
             or pass --insecure-no-tls if TLS is terminated by an external proxy."
        );
    }
    Ok(())
}

// ── Error mapping ────────────────────────────────────────────────────────────

fn vault_to_rpc_err(err: VaultError) -> ErrorObjectOwned {
    let code = err.rpc_code();
    // Log internal errors at warn level — never include sensitive detail.
    if code == -32005 {
        tracing::warn!(target: "tari_vault::rpc", "Internal vault error: {err}");
    }
    ErrorObjectOwned::owned(code, err.to_string(), None::<()>)
}
