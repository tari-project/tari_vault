use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use http::{Request, Response, StatusCode};
use subtle::ConstantTimeEq;
use tower::{Layer, Service};

/// Tower HTTP layer that enforces Bearer token authentication.
///
/// Install this layer before the JSON-RPC handler so that unauthenticated
/// requests receive an HTTP 401 before any RPC parsing takes place.
///
/// When `token` is `None` (or empty) the layer is transparent — all requests
/// are forwarded unchanged.  This makes it safe to always install the layer;
/// auth is simply disabled in development / local setups.
///
/// # Timing safety
///
/// Token comparison uses [`subtle::ConstantTimeEq`] to prevent timing oracles.
///
/// # Example
///
/// ```rust,no_run
/// use tari_vault::auth::BearerAuthLayer;
/// use tower::ServiceBuilder;
///
/// // From the vault config (None = disabled):
/// let layer = BearerAuthLayer::from_config(Some("my-secret-token".to_string()));
/// let middleware = ServiceBuilder::new().layer(layer);
/// ```
#[derive(Clone)]
pub struct BearerAuthLayer {
    token: Option<Arc<str>>,
}

impl BearerAuthLayer {
    /// Create a layer that requires the given Bearer token on every request.
    pub fn new(token: String) -> Self {
        Self {
            token: Some(Arc::from(token.as_str())),
        }
    }

    /// Create a transparent (disabled) layer.  No `Authorization` header is
    /// checked.
    pub fn disabled() -> Self {
        Self { token: None }
    }

    /// Construct from an optional config value.
    ///
    /// `None` or an empty string → auth disabled.
    pub fn from_config(token: Option<String>) -> Self {
        match token {
            Some(t) if !t.is_empty() => Self::new(t),
            _ => Self::disabled(),
        }
    }
}

impl<S> Layer<S> for BearerAuthLayer {
    type Service = BearerAuthService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        BearerAuthService {
            inner,
            token: self.token.clone(),
        }
    }
}

/// Tower HTTP service produced by [`BearerAuthLayer`].
#[derive(Clone)]
pub struct BearerAuthService<S> {
    inner: S,
    token: Option<Arc<str>>,
}

impl<S, ReqBody, RespBody> Service<Request<ReqBody>> for BearerAuthService<S>
where
    S: Service<Request<ReqBody>, Response = Response<RespBody>> + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
    ReqBody: Send + 'static,
    RespBody: Default + Send + 'static,
{
    type Response = Response<RespBody>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        // Clone the token so we're not borrowing `self` when calling
        // `self.inner.call(req)` later.
        let token = self.token.clone();

        let Some(expected) = token else {
            // Auth disabled — forward immediately.
            return Box::pin(self.inner.call(req));
        };

        let provided = req
            .headers()
            .get(http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .map(|s| s.as_bytes().to_vec());

        let auth_ok = provided
            .as_deref()
            .map(|p| bool::from(p.ct_eq(expected.as_bytes())))
            .unwrap_or(false);

        if auth_ok {
            return Box::pin(self.inner.call(req));
        }

        tracing::warn!(target: "tari_vault::auth", "Rejected request: missing or invalid Bearer token");

        Box::pin(async {
            Ok(Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header(
                    http::header::WWW_AUTHENTICATE,
                    "Bearer realm=\"tari_vault\"",
                )
                .body(RespBody::default())
                .expect("401 response is always valid"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceExt;

    async fn echo(req: Request<String>) -> Result<Response<String>, std::convert::Infallible> {
        let _ = req;
        Ok(Response::builder()
            .status(200)
            .body("ok".to_string())
            .unwrap())
    }

    #[tokio::test]
    async fn disabled_passes_all_requests() {
        let svc = BearerAuthLayer::disabled().layer(tower::service_fn(echo));
        let req = Request::builder().uri("/").body(String::new()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 200);
    }

    #[tokio::test]
    async fn valid_token_passes() {
        let svc = BearerAuthLayer::new("secret".into()).layer(tower::service_fn(echo));
        let req = Request::builder()
            .uri("/")
            .header(http::header::AUTHORIZATION, "Bearer secret")
            .body(String::new())
            .unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 200);
    }

    #[tokio::test]
    async fn missing_token_returns_401() {
        let svc = BearerAuthLayer::new("secret".into()).layer(tower::service_fn(echo));
        let req = Request::builder().uri("/").body(String::new()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn wrong_token_returns_401() {
        let svc = BearerAuthLayer::new("secret".into()).layer(tower::service_fn(echo));
        let req = Request::builder()
            .uri("/")
            .header(http::header::AUTHORIZATION, "Bearer wrong")
            .body(String::new())
            .unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn www_authenticate_header_present_on_401() {
        let svc = BearerAuthLayer::new("secret".into()).layer(tower::service_fn(echo));
        let req = Request::builder().uri("/").body(String::new()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        assert!(resp.headers().contains_key(http::header::WWW_AUTHENTICATE));
    }

    #[tokio::test]
    async fn from_config_none_is_disabled() {
        let layer = BearerAuthLayer::from_config(None);
        assert!(layer.token.is_none());
    }

    #[tokio::test]
    async fn from_config_empty_string_is_disabled() {
        let layer = BearerAuthLayer::from_config(Some(String::new()));
        assert!(layer.token.is_none());
    }
}
