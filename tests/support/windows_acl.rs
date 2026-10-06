//! Windows access-control fixture used by mutation recovery tests.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Restores a temporary directory's access even if its test panics.
pub(super) struct DeniedDirectoryAccess {
    path: PathBuf,
    active: bool,
}

impl DeniedDirectoryAccess {
    /// Removes the fixture's deny rule before checking the operation's result.
    pub(super) fn restore(&mut self) -> io::Result<()> {
        if self.active {
            change_directory_acl(&self.path, "/remove:d", "*S-1-1-0")?;
            self.active = false;
        }
        Ok(())
    }
}

impl Drop for DeniedDirectoryAccess {
    fn drop(&mut self) {
        if let Err(error) = self.restore() {
            eprintln!("failed to restore Windows directory-listing fixture: {error}");
        }
    }
}

/// Denies a directory right to Everyone using its stable, locale-free SID.
///
/// `RD` denies `FILE_LIST_DIRECTORY`; `WD` denies `FILE_ADD_FILE` without denying reads.
pub(super) fn deny_directory_access(path: &Path, right: &str) -> io::Result<DeniedDirectoryAccess> {
    change_directory_acl(path, "/deny", &format!("*S-1-1-0:({right})"))?;
    Ok(DeniedDirectoryAccess {
        path: path.to_path_buf(),
        active: true,
    })
}

/// Applies one ACL operation only to the disposable fixture directory.
fn change_directory_acl(path: &Path, operation: &str, rule: &str) -> io::Result<()> {
    let output = Command::new("icacls")
        .arg(path)
        .args([operation, rule])
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "icacls could not update the fixture ACL ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )))
    }
}
