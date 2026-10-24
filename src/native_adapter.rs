//! Versioned, transport-neutral byte ABI for authorized native adapters.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Wire protocol identity; incompatible versions refuse before dispatch.
pub const NATIVE_ADAPTER_ABI: &str = "pm-native-adapter/1";
/// Hard admission bound for each JSON frame, independent of adapter claims.
pub const NATIVE_ADAPTER_MAX_BYTES: usize = 1_048_576;

/// Narrow SDK capabilities; an adapter grants each operation explicitly.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterCapability {
    /// Read one canonical item.
    Get,
    /// Read a bounded canonical list, retaining omission receipts.
    List,
    /// Claim through the backend's authoritative lock-scoped SDK operation.
    Claim,
    /// Release only the caller's claim through the authoritative SDK operation.
    Release,
}

/// Adapter declaration supplied by the embedding application, never auto-discovered.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterManifest {
    /// Exact supported wire ABI.
    pub abi: String,
    /// Operations authorized for this adapter instance.
    pub capabilities: Vec<AdapterCapability>,
    /// Maximum encoded request or response frame accepted by the adapter.
    pub max_frame_bytes: usize,
}

/// Narrow typed operations; tracker identity and credentials belong to the adapter.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum AdapterOperation {
    /// Read a canonical item with the backend SDK's get operation.
    Get {
        /// Exact item ID.
        id: String,
    },
    /// Read a bounded list with canonical omission and continuation receipts.
    List {
        /// Requested row ceiling, from one through one hundred.
        limit: u16,
        /// Opaque canonical producer continuation from the preceding SDK page.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<String>,
        /// Opaque canonical output continuation, when `continuation_kind` requires it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_cursor: Option<String>,
    },
    /// Claim using the authoritative SDK, without force or read-then-write emulation.
    Claim {
        /// Exact item ID.
        id: String,
        /// Authorized caller identity; the backend must verify this identity.
        author: String,
    },
    /// Release only a claim belonging to this authorized caller.
    Release {
        /// Exact item ID.
        id: String,
        /// Authorized caller identity; foreign claims must be skipped with a reason.
        author: String,
    },
}

impl AdapterOperation {
    /// Returns the required capability without inspecting backend data.
    #[must_use]
    pub const fn capability(&self) -> AdapterCapability {
        match self {
            Self::Get { .. } => AdapterCapability::Get,
            Self::List { .. } => AdapterCapability::List,
            Self::Claim { .. } => AdapterCapability::Claim,
            Self::Release { .. } => AdapterCapability::Release,
        }
    }
}

/// One independently correlated call; identifiers do not imply replay safety.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterRequest {
    /// Exact wire ABI.
    pub abi: String,
    /// Caller-owned correlation ID, never a retry or transaction key.
    pub request_id: String,
    /// Typed SDK operation.
    pub operation: AdapterOperation,
}

/// Successful backend output or a typed backend refusal, without projection loss.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum AdapterOutcome {
    /// Authoritative SDK output, including every mutation and omission receipt.
    Success {
        /// Unmodified SDK result. Success does not imply changed, complete or owned.
        output: Value,
    },
    /// Authoritative refusal retained with backend guidance and original details.
    Refused {
        /// Stable backend code.
        code: String,
        /// Backend explanation.
        message: String,
        /// Backend recovery guidance.
        remedy: String,
        /// Original structured refusal, including partial-operation receipts.
        details: Value,
    },
}

/// Exactly one correlated response frame; adapters never stream unbounded data.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterReply {
    /// Exact wire ABI.
    pub abi: String,
    /// Echo of the caller's request ID.
    pub request_id: String,
    /// Backend outcome, retained even when it is a refusal or peer success.
    pub outcome: AdapterOutcome,
}

/// Stable local refusal classifications, distinct from backend domain refusals.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterRefusalCode {
    /// Manifest, request or reply uses another ABI.
    IncompatibleAbi,
    /// The instance does not authorize this operation.
    UnsupportedCapability,
    /// A frame is malformed or cannot be correlated.
    InvalidFrame,
    /// Encoded input or output exceeds its admission ceiling.
    FrameLimit,
    /// Transport failed; a mutation's outcome may be unknown.
    OutcomeUnknown,
}

/// Actionable ABI refusal; a transport failure never proves a write did not occur.
#[derive(Clone, Debug, Deserialize, Serialize, thiserror::Error)]
#[error("{message}; {remedy}")]
pub struct AdapterRefusal {
    /// Stable local classification.
    pub code: AdapterRefusalCode,
    /// Whether exchange started; true means a mutation may already have committed.
    pub dispatched: bool,
    /// Concise explanation.
    pub message: String,
    /// Explicit caller recovery instruction.
    pub remedy: String,
}

/// Creates a stable ABI diagnostic without retaining potentially sensitive frame bytes.
fn refusal(code: AdapterRefusalCode) -> AdapterRefusal {
    let (message, remedy) = match code {
        AdapterRefusalCode::IncompatibleAbi => (
            "incompatible native adapter ABI",
            "use an adapter and caller implementing pm-native-adapter/1",
        ),
        AdapterRefusalCode::UnsupportedCapability => (
            "adapter capability unavailable; no call dispatched",
            "select an authorized adapter declaring the requested operation",
        ),
        AdapterRefusalCode::InvalidFrame => (
            "invalid native adapter frame",
            "use the documented typed fields and matching request ID; reconcile a dispatched mutation before retrying",
        ),
        AdapterRefusalCode::FrameLimit => (
            "native adapter frame exceeds admission limits",
            "use smaller canonical pages; reconcile a dispatched mutation before retrying",
        ),
        AdapterRefusalCode::OutcomeUnknown => (
            "adapter transport failed; mutation outcome may be unknown",
            "read authoritative backend receipts before deciding whether to retry",
        ),
    };
    AdapterRefusal {
        code,
        dispatched: false,
        message: message.into(),
        remedy: remedy.into(),
    }
}

/// Marks an untrustworthy dispatched result without hiding its ABI classification.
fn dispatched_refusal(mut error: AdapterRefusal) -> AdapterRefusal {
    error.dispatched = true;
    error
        .remedy
        .push_str("; reconcile authoritative mutation receipts before retrying");
    error
}

/// Caller-owned native transport, implemented with the backend's authorized SDK.
///
/// No Git, Node, C layout, dynamic loading or network transport is assumed.
/// Implementations must bound reads by `response_limit`, authorize the caller,
/// preserve canonical receipts/refusals, and use authoritative claim/release
/// locking. Do not emulate mutations with a client-side read then write.
/// Never retry here: an interrupted mutation can already have committed.
pub trait NativeAdapter {
    /// Declares the immutable capabilities of this authorized adapter instance.
    fn manifest(&self) -> AdapterManifest;
    /// Exchanges exactly one bounded UTF-8 JSON frame without automatic retries.
    ///
    /// # Errors
    /// Return transport failure when no trustworthy response is available.
    /// The client converts transport failures to an unknown-outcome refusal.
    fn exchange(
        &mut self,
        request: &[u8],
        response_limit: usize,
    ) -> Result<Vec<u8>, std::io::Error>;
}

/// Validates a manifest before any backend call.
///
/// # Errors
/// Refuses incompatible ABI versions or invalid frame admission ceilings.
pub fn check_adapter_manifest(manifest: &AdapterManifest) -> Result<(), AdapterRefusal> {
    check_abi(&manifest.abi)?;
    if manifest.max_frame_bytes == 0 || manifest.max_frame_bytes > NATIVE_ADAPTER_MAX_BYTES {
        return Err(refusal(AdapterRefusalCode::FrameLimit));
    }
    Ok(())
}

/// Checks the exact ABI identity; unknown revisions are never optimistically accepted.
fn check_abi(abi: &str) -> Result<(), AdapterRefusal> {
    if abi != NATIVE_ADAPTER_ABI {
        return Err(refusal(AdapterRefusalCode::IncompatibleAbi));
    }
    Ok(())
}

/// Checks a nonblank bounded caller string without imposing backend ID syntax.
fn valid_text(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256
}

/// Validates and encodes a request before transport access or writes.
///
/// # Errors
/// Refuses incompatible versions, unsupported operations, invalid fields or size.
pub fn prepare_adapter_request(
    manifest: &AdapterManifest,
    request: &AdapterRequest,
) -> Result<Vec<u8>, AdapterRefusal> {
    check_adapter_manifest(manifest)?;
    check_abi(&request.abi)?;
    if !manifest
        .capabilities
        .contains(&request.operation.capability())
    {
        return Err(refusal(AdapterRefusalCode::UnsupportedCapability));
    }
    let valid = match &request.operation {
        AdapterOperation::Get { id } => valid_text(id),
        AdapterOperation::List { limit, .. } => (1..=100).contains(limit),
        AdapterOperation::Claim { id, author } | AdapterOperation::Release { id, author } => {
            valid_text(id) && valid_text(author)
        }
    };
    if !valid_text(&request.request_id) || !valid {
        return Err(refusal(AdapterRefusalCode::InvalidFrame));
    }
    let encoded = crate::stringify_json(request, false).into_bytes();
    check_size(encoded.len(), manifest.max_frame_bytes)?;
    Ok(encoded)
}

/// Refuses excess bytes before decoding a frame.
fn check_size(length: usize, maximum: usize) -> Result<(), AdapterRefusal> {
    if length > maximum {
        return Err(refusal(AdapterRefusalCode::FrameLimit));
    }
    Ok(())
}

/// Validates one bounded response and preserves its canonical backend envelope.
///
/// # Errors
/// Refuses invalid requests, oversized replies, malformed frames or mismatched identity.
pub fn check_adapter_reply(
    manifest: &AdapterManifest,
    request: &AdapterRequest,
    bytes: &[u8],
) -> Result<AdapterReply, AdapterRefusal> {
    prepare_adapter_request(manifest, request)?;
    check_size(bytes.len(), manifest.max_frame_bytes)?;
    let reply: AdapterReply =
        serde_json::from_slice(bytes).map_err(|_| refusal(AdapterRefusalCode::InvalidFrame))?;
    check_abi(&reply.abi)?;
    if reply.request_id != request.request_id {
        return Err(refusal(AdapterRefusalCode::InvalidFrame));
    }
    Ok(reply)
}

/// Stateless native SDK client; no retries, shared mutable tracker state or local claims.
pub struct NativeAdapterClient<A> {
    /// Embedding application's transport and authorization boundary.
    adapter: A,
    /// Fixed declaration validated at construction.
    manifest: AdapterManifest,
}

impl<A: NativeAdapter> NativeAdapterClient<A> {
    /// Admits an explicit adapter without invoking its backend.
    ///
    /// # Errors
    /// Refuses incompatible ABI declarations and invalid frame ceilings.
    pub fn new(adapter: A) -> Result<Self, AdapterRefusal> {
        let manifest = adapter.manifest();
        check_adapter_manifest(&manifest)?;
        Ok(Self { adapter, manifest })
    }

    /// Dispatches once after preflight, preserving authoritative results and refusals.
    ///
    /// # Errors
    /// Refuses invalid or unsupported calls before dispatch. Any failure after
    /// exchange starts must be reconciled at the backend before mutation retry.
    pub fn call(&mut self, request: &AdapterRequest) -> Result<AdapterReply, AdapterRefusal> {
        let bytes = prepare_adapter_request(&self.manifest, request)?;
        let response = self
            .adapter
            .exchange(&bytes, self.manifest.max_frame_bytes)
            .map_err(|_| dispatched_refusal(refusal(AdapterRefusalCode::OutcomeUnknown)))?;
        check_adapter_reply(&self.manifest, request, &response).map_err(dispatched_refusal)
    }
}

/// Offline CLI transcript: validates compatibility without executing any operation.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterTranscript {
    /// Authorized adapter declaration.
    pub manifest: AdapterManifest,
    /// Prospective operation.
    pub request: AdapterRequest,
    /// Optional recorded response; never replayed against the backend.
    pub reply: Option<AdapterReply>,
}

/// Validates a bounded transcript and returns an agent-sized compatibility receipt.
///
/// # Errors
/// Refuses malformed input, unsupported capabilities, or incompatible recorded replies.
pub fn check_adapter_transcript(bytes: &[u8]) -> Result<Value, AdapterRefusal> {
    check_size(bytes.len(), NATIVE_ADAPTER_MAX_BYTES)?;
    let transcript: AdapterTranscript =
        serde_json::from_slice(bytes).map_err(|_| refusal(AdapterRefusalCode::InvalidFrame))?;
    let request_bytes = prepare_adapter_request(&transcript.manifest, &transcript.request)?;
    let outcome = match transcript.reply {
        Some(reply) => {
            let encoded = crate::stringify_json(&reply, false).into_bytes();
            let checked = check_adapter_reply(&transcript.manifest, &transcript.request, &encoded)?;
            match checked.outcome {
                AdapterOutcome::Success { .. } => "success",
                AdapterOutcome::Refused { .. } => "refused",
            }
        }
        None => "not_supplied",
    };
    Ok(serde_json::json!({
        "ok": true, "abi": NATIVE_ADAPTER_ABI, "dispatched": false,
        "request_id": transcript.request.request_id,
        "capability": transcript.request.operation.capability(),
        "request_bytes": request_bytes.len(), "reply_status": outcome,
        "receipt_scope": "abi_compatibility_only"
    }))
}
