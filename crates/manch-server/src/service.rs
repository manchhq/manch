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
    ) -> ServiceResult<impl connectrpc::Encodable<GetVersionResponse> + Send + use<'a>> {
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
    /// The test calls `get_version` directly, encodes the opaque response body
    /// through the binary proto codec, decodes it back to the owned struct, and
    /// asserts that `version == CARGO_PKG_VERSION`.
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
        // GetVersionRequestView is publicly re-exported at crate::proto::manch::v1
        // (via `pub use self::__buffa::view::GetVersionRequestView` in the generated
        // mod.rs), so we use that stable public path instead of the internal
        // `__buffa::view::` sub-module.
        let view =
            crate::proto::manch::v1::GetVersionRequestView::decode_view(&body)
                .expect("empty buffer is a valid GetVersionRequest");
        // ServiceRequest::from_parts is #[doc(hidden)] (generated dispatch glue uses
        // it internally); no public From/new constructor exists on ServiceRequest in
        // connectrpc 0.7.0. The only alternative would be an encode→decode round-trip
        // through GetVersionRequestOwnedView::from_owned, but that is heavier without
        // benefit. We keep from_parts and note the limitation here.
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
