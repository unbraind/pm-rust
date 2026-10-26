//! Inspect bounded native scan work before requesting list context.
use pm_rust::{QueryCostRequest, QueryProjection, Workspace};
use std::{error::Error, path::Path};

/// Prints brief and full plans; neither call executes a query or writes state.
fn main() -> Result<(), Box<dyn Error>> {
    let path = std::env::args().nth(1).ok_or("pass a tracker directory")?;
    let workspace = Workspace::discover(Path::new(&path))?;
    for projection in [QueryProjection::Brief, QueryProjection::Full] {
        let plan = workspace.explain_list(&QueryCostRequest {
            projection,
            ..QueryCostRequest::default()
        })?;
        println!("{}", serde_json::to_string(&plan)?);
    }
    Ok(())
}
