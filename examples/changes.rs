//! Read one native change page; persist its cursor after consuming the whole page.
use pm_rust::{
    SessionCancellation, Workspace,
    changes::{ChangeReader, ChangeRequest, LocalChangeAdapter},
};
use std::path::Path;

/// Selects the local adapter explicitly, with no Node or Git dependency.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: changes <project> [cursor]")?;
    let cancellation = SessionCancellation::default();
    let adapter =
        LocalChangeAdapter::new(Workspace::discover(Path::new(&path))?, cancellation.clone());
    let request = ChangeRequest {
        cursor: std::env::args().nth(2),
        ..ChangeRequest::default()
    };
    let mut reader = ChangeReader::new(adapter, request, cancellation)?;
    println!("{}", reader.pull()?.stringify_toon());
    Ok(())
}
