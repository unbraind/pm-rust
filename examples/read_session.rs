//! Pull certified native context pages without loading the SDK into Node.
use pm_rust::{ReadSessionLimits, SessionCancellation, Workspace};
use std::{error::Error, path::Path};

/// Opens a caller-selected tracker and discards each page after processing it.
fn main() -> Result<(), Box<dyn Error>> {
    let path = std::env::args().nth(1).ok_or("pass a tracker directory")?;
    let workspace = Workspace::discover(Path::new(&path))?;
    let mut session = workspace.read_session(
        &ReadSessionLimits::default(),
        SessionCancellation::default(),
        &pm_rust::current_timestamp(),
    )?;
    while let Some(page) = session.next_page()? {
        print!("{}", page.output.stringify_read_json());
        eprintln!("{}", serde_json::to_string(&page.receipt)?);
    }
    Ok(())
}
