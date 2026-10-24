//! Minimal authorized read adapter; replace only exchange with an existing remote transport.
use pm_rust::native_adapter::{
    AdapterCapability, AdapterManifest, AdapterOperation, AdapterOutcome, AdapterReply,
    AdapterRequest, NATIVE_ADAPTER_ABI, NATIVE_ADAPTER_MAX_BYTES, NativeAdapter,
    NativeAdapterClient,
};
use pm_rust::{ItemFilter, ListOptions, Workspace};
use std::{
    error::Error,
    io::{self, Read},
    path::Path,
};

/// Explicitly selected read-only workspace; no discovery from request fields.
struct AuthorizedReadAdapter(Workspace);

impl NativeAdapter for AuthorizedReadAdapter {
    fn manifest(&self) -> AdapterManifest {
        AdapterManifest {
            abi: NATIVE_ADAPTER_ABI.into(),
            capabilities: vec![AdapterCapability::Get, AdapterCapability::List],
            max_frame_bytes: NATIVE_ADAPTER_MAX_BYTES,
        }
    }

    fn exchange(&mut self, bytes: &[u8], response_limit: usize) -> io::Result<Vec<u8>> {
        let request: AdapterRequest = serde_json::from_slice(bytes)?;
        let result = match request.operation {
            AdapterOperation::Get { id } => self.0.get(&id).map(|item| serde_json::json!(item)),
            AdapterOperation::List {
                limit,
                after,
                output_cursor,
            } => self
                .0
                .list_page(
                    &ItemFilter::default(),
                    &ListOptions {
                        all: true,
                        brief: true,
                        limit: Some(limit.to_string()),
                        after,
                        output_cursor,
                        output_budget: Some("4000".into()),
                        ..ListOptions::default()
                    },
                    &pm_rust::current_timestamp(),
                )
                .map(|output| serde_json::json!(output)),
            // Admission in NativeAdapterClient prevents these operations reaching this peer.
            _ => return Err(io::Error::other("read-only adapter")),
        };
        let outcome = match result {
            Ok(output) => AdapterOutcome::Success { output },
            Err(error) => AdapterOutcome::Refused {
                code: "native_read_refused".into(),
                message: error.to_string(),
                remedy: "check the requested identity and tracker integrity".into(),
                details: serde_json::Value::Null,
            },
        };
        let reply = AdapterReply {
            abi: NATIVE_ADAPTER_ABI.into(),
            request_id: request.request_id,
            outcome,
        };
        let encoded = pm_rust::stringify_json(&reply, false).into_bytes();
        if encoded.len() > response_limit {
            return Err(io::Error::other("response limit"));
        }
        Ok(encoded)
    }
}

/// Exchanges one typed request on stdin and emits one canonical reply on stdout.
fn main() -> Result<(), Box<dyn Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("pass an explicitly authorized tracker directory")?;
    let workspace = Workspace::discover(Path::new(&path))?;
    let mut client = NativeAdapterClient::new(AuthorizedReadAdapter(workspace))?;
    let mut bytes = Vec::new();
    io::stdin()
        .lock()
        .take((NATIVE_ADAPTER_MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > NATIVE_ADAPTER_MAX_BYTES {
        return Err("request limit exceeded".into());
    }
    let request: AdapterRequest = serde_json::from_slice(&bytes)?;
    let reply = client.call(&request)?;
    println!("{}", pm_rust::stringify_json(&reply, false));
    Ok(())
}
