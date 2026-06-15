use crate::proto::manch::v1::{
    GetVersionRequest, GetVersionResponse, ManchService,
};
use connectrpc::{RequestContext, Response, ServiceRequest, ServiceResult};

/// Stub implementation of [`ManchService`].
///
/// `get_version` reports the crate version compiled in via `CARGO_PKG_VERSION`.
/// All other RPCs will be added in later milestones.
pub struct ManchServiceImpl;

impl ManchService for ManchServiceImpl {
    async fn get_version<'a>(
        &'a self,
        _ctx: RequestContext,
        _request: ServiceRequest<'_, GetVersionRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<GetVersionResponse> + Send + use<'a>>
    {
        Response::ok(GetVersionResponse {
            version: env!("CARGO_PKG_VERSION").to_string(),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The test verifies the contract that matters: `get_version` returns a
    /// response whose `version` field equals the crate's `CARGO_PKG_VERSION`.
    ///
    /// We test this at the level of the extracted `version_string()` helper
    /// (which contains the logic) plus a smoke-check that the impl wires it
    /// through, encoded via the binary proto codec, to ensure the full
    /// round-trip works.
    #[tokio::test]
    async fn get_version_returns_cargo_pkg_version() {
        use buffa::{Message as _, MessageView as _, bytes::Bytes};
        use connectrpc::{CodecFormat, Encodable as _, RequestContext, ServiceRequest};

        let svc = ManchServiceImpl;

        // ── build request ────────────────────────────────────────────────────
        // GetVersionRequest has no fields, so an empty buffer is a valid
        // encoding. We keep the view and bytes in the same scope so the
        // borrowed `ServiceRequest` is valid for the call.
        let body = Bytes::new();
        let view =
            crate::proto::manch::v1::__buffa::view::GetVersionRequestView::decode_view(&body)
                .expect("empty buffer is a valid GetVersionRequest");
        let req = ServiceRequest::<GetVersionRequest>::from_parts(&view, &body);

        let ctx = RequestContext::new(axum::http::HeaderMap::new());

        // ── call the service ─────────────────────────────────────────────────
        let resp = svc
            .get_version(ctx, req)
            .await
            .expect("get_version must not return a ConnectError");

        // ── verify via proto round-trip ──────────────────────────────────────
        // Encode the opaque response body to binary protobuf, then decode it
        // back to the owned struct so we can inspect the `version` field.
        let encoded = resp
            .body
            .encode(CodecFormat::Proto)
            .expect("encoding must not fail")
            .to_vec();

        let decoded = GetVersionResponse::decode(&mut encoded.as_slice())
            .expect("decoding must not fail");

        assert_eq!(
            decoded.version,
            env!("CARGO_PKG_VERSION"),
            "returned version must equal CARGO_PKG_VERSION"
        );
    }
}
