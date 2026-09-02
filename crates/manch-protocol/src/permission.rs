//! Permission vocabulary for host-registered tools. ACP's
//! `session/request_permission` round-trip, applied to the BYOK path.
//!
//! Manch does not decide permission *posture* for consumers it has not met.
//! It ships a default that always asks — never one that silently allows — and
//! it does not truncate ACP's own vocabulary to do so: [`PermissionDecision`]
//! carries `Vec<acp::PermissionOption>` and `acp::RequestPermissionOutcome`
//! verbatim, so a consumer with its own policy store (remembered
//! "always allow"/"always reject" choices, an allowlist, an approvals queue)
//! can return the fuller option set — including `AllowAlways` /
//! `RejectAlways` — and resolve a remembered decision itself without Manch
//! standing in the way.
//!
//! The built-in [`AskOncePolicy`] offers only the two "once" options. That is
//! a statement about what Manch *stores*, not a judgement about the
//! `AllowAlways` option itself: remembering a decision across invocations is
//! state, and Manch's protocol crate holds none. A consumer that does hold
//! that state (a `PermissionPolicy` backed by its own store) is free to offer
//! `acp::PermissionOptionKind::AllowAlways` / `RejectAlways` and resolve them
//! itself via [`PermissionDecision::Resolved`].

use async_trait::async_trait;

use crate::Result;
use crate::acp;
use crate::tool::{ToolContext, ToolInvocation};

/// What a [`PermissionPolicy`] decides for a single tool invocation.
///
/// This is deliberately not collapsed to a bool. `Ask` hands the caller ACP's
/// own `PermissionOption`s to present to a human (via
/// `session/request_permission`); `Resolved` lets a policy with its own store
/// short-circuit that round-trip with an outcome it already knows — without
/// Manch ever needing to model what "remembered" means.
#[derive(Debug, Clone)]
pub enum PermissionDecision {
    /// A human must be asked. Carries the options to offer, in ACP's own
    /// vocabulary.
    Ask(Vec<acp::PermissionOption>),
    /// The policy already knows the outcome (e.g. a remembered "always
    /// allow") and no round-trip is needed.
    Resolved(acp::RequestPermissionOutcome),
}

/// **Extension point.** Decides whether (and how) a human is asked before a
/// [`Tier::Draft`](crate::Tier) tool executes.
///
/// Manch does not decide permission posture for consumers it has not met — it
/// only provides the seam and a safe default. See the module docs for why the
/// default never offers `AllowAlways`.
#[async_trait]
pub trait PermissionPolicy: Send + Sync {
    /// Decide what to do about `inv` in the context `cx`.
    async fn decide(&self, cx: &ToolContext, inv: &ToolInvocation) -> Result<PermissionDecision>;
}

/// The two permission options a policy holding no memory can honestly offer:
/// allow this one call, or reject this one call. Their ids are ACP's own
/// `PermissionOptionKind` serde names (`allow_once`, `reject_once`), not
/// arbitrary tokens, so a consumer building its own option list — or matching
/// on a response — has a fixed, documented vocabulary to work against.
pub fn once_options() -> Vec<acp::PermissionOption> {
    vec![
        acp::PermissionOption::new(
            acp::PermissionOptionId::new("allow_once"),
            "Allow once",
            acp::PermissionOptionKind::AllowOnce,
        ),
        acp::PermissionOption::new(
            acp::PermissionOptionId::new("reject_once"),
            "Reject",
            acp::PermissionOptionKind::RejectOnce,
        ),
    ]
}

/// Maps a [`acp::PermissionOptionId`] back to the [`acp::PermissionOptionKind`]
/// it names, if it is one of the four ACP serde names
/// (`allow_once`/`allow_always`/`reject_once`/`reject_always`). Returns `None`
/// for anything else, rather than guessing — an id a consumer invented for its
/// own option is not silently coerced into one of ACP's kinds.
pub fn kind_of(id: &acp::PermissionOptionId) -> Option<acp::PermissionOptionKind> {
    match id.0.as_ref() {
        "allow_once" => Some(acp::PermissionOptionKind::AllowOnce),
        "allow_always" => Some(acp::PermissionOptionKind::AllowAlways),
        "reject_once" => Some(acp::PermissionOptionKind::RejectOnce),
        "reject_always" => Some(acp::PermissionOptionKind::RejectAlways),
        _ => None,
    }
}

/// The default [`PermissionPolicy`]: always asks, offering only the two
/// "once" options. See the module docs for why `AllowAlways` is never
/// offered by this policy specifically (it holds no store to remember a
/// decision by), while remaining fully expressible for a consumer that does.
#[derive(Debug, Clone, Copy, Default)]
pub struct AskOncePolicy;

#[async_trait]
impl PermissionPolicy for AskOncePolicy {
    async fn decide(&self, _cx: &ToolContext, _inv: &ToolInvocation) -> Result<PermissionDecision> {
        Ok(PermissionDecision::Ask(once_options()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Extensions;

    #[test]
    fn option_ids_are_the_kind_serde_names() {
        let opts = once_options();
        let ids: Vec<&str> = opts.iter().map(|o| o.option_id.0.as_ref()).collect();
        assert_eq!(ids, vec!["allow_once", "reject_once"]);
    }

    #[test]
    fn every_offered_option_id_maps_back_to_its_kind() {
        for o in once_options() {
            assert_eq!(kind_of(&o.option_id), Some(o.kind));
        }
    }

    #[test]
    fn an_unknown_option_id_maps_to_no_kind() {
        assert!(kind_of(&acp::PermissionOptionId::new("yes-please")).is_none());
    }

    #[tokio::test]
    async fn the_default_policy_asks_rather_than_deciding() {
        let cx = ToolContext::new("s", "c1", std::sync::Arc::new(Extensions::default()));
        let inv = ToolInvocation {
            id: "c1".into(),
            name: "draft_prescription".into(),
            arguments: serde_json::Value::Null,
        };
        match AskOncePolicy.decide(&cx, &inv).await.unwrap() {
            PermissionDecision::Ask(opts) => assert_eq!(opts.len(), 2),
            PermissionDecision::Resolved(_) => panic!("default policy must not decide silently"),
        }
    }
}
