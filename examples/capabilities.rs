//! Inspect the complete native product without opening a tracker or transport.
use pm_rust::discovery::{DiscoveryRequest, discover_capabilities};

/// Prints a complete bounded JSON inventory using the same contract as the CLI.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let request = DiscoveryRequest::default();
    let response = discover_capabilities(&request)?;
    println!("{}", response.render(true)?);
    Ok(())
}
