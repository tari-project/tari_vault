use jsonrpsee::{server::RpcModule, types::ErrorObjectOwned};
use serde_json::Value;

/// The committed OpenRPC spec, embedded at compile time.
///
/// This guarantees that the running binary always serves the spec that matches
/// the code it was built from.  The file path is relative to this source file.
const OPENRPC_SPEC: &str = include_str!("../../openrpc.json");

/// Build a `RpcModule` that exposes the standard `rpc.discover` method.
///
/// `rpc.discover` is the [OpenRPC service-discovery](https://spec.open-rpc.org/#service-discovery-method)
/// method.  Callers can retrieve the full API description at runtime by sending:
///
/// ```json
/// {"jsonrpc":"2.0","method":"rpc.discover","params":[],"id":1}
/// ```
///
/// The method is intentionally registered with a dot in its name to match the
/// OpenRPC standard.  The spec embedded here is compiled-in from `openrpc.json`
/// at the project root — it therefore always matches the binary's API version.
///
/// **Authentication note**: when bearer-token auth is enabled, this method
/// requires the same `Authorization: Bearer <token>` header as all other
/// methods, because auth is enforced at the HTTP transport layer before
/// JSON-RPC routing.  The spec is also available as `openrpc.json` in the
/// source repository for unauthenticated offline access.
pub fn discovery_module() -> RpcModule<()> {
    let mut module = RpcModule::new(());

    module
        .register_method("rpc.discover", |_params, _ctx, _extensions| {
            serde_json::from_str::<Value>(OPENRPC_SPEC).map_err(|e| {
                ErrorObjectOwned::owned(-32603, "Failed to load OpenRPC spec", Some(e.to_string()))
            })
        })
        .expect("rpc.discover registration must not fail");

    module
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn openrpc_spec_is_valid_json() {
        let value: Value =
            serde_json::from_str(OPENRPC_SPEC).expect("openrpc.json must be valid JSON");
        assert!(value.is_object(), "openrpc.json root must be an object");
    }

    #[test]
    fn openrpc_spec_has_required_fields() {
        let spec: Value = serde_json::from_str(OPENRPC_SPEC).unwrap();
        assert!(
            spec["openrpc"].is_string(),
            "missing 'openrpc' version field"
        );
        assert!(spec["info"]["title"].is_string(), "missing 'info.title'");
        assert!(
            spec["info"]["version"].is_string(),
            "missing 'info.version'"
        );
        assert!(spec["methods"].is_array(), "missing 'methods' array");
    }

    #[test]
    fn all_vault_methods_are_documented() {
        let spec: Value = serde_json::from_str(OPENRPC_SPEC).unwrap();
        let method_names: Vec<&str> = spec["methods"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|m| m["name"].as_str())
            .collect();

        for expected in &[
            "vault_storeProof",
            "vault_retrieveProof",
            "vault_deleteProof",
            "rpc.discover",
        ] {
            assert!(
                method_names.contains(expected),
                "openrpc.json is missing method '{expected}'"
            );
        }
    }

    #[test]
    fn all_custom_error_codes_are_documented() {
        let spec: Value = serde_json::from_str(OPENRPC_SPEC).unwrap();
        let errors = &spec["components"]["errors"];

        let codes: Vec<i64> = errors
            .as_object()
            .unwrap()
            .values()
            .filter_map(|e| e["code"].as_i64())
            .collect();

        for expected_code in &[-32001_i64, -32002, -32003, -32004, -32005] {
            assert!(
                codes.contains(expected_code),
                "openrpc.json is missing error code {expected_code}"
            );
        }
    }
}
