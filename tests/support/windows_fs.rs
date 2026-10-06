//! Windows fixture policy shared by filesystem fault tests.

use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::path::Path;

/// Refuses directory enumeration through a real exclusive Windows handle.
///
/// List-directory access activates sharing checks; share mode zero refuses a
/// second enumerator. Metadata probes request no data access and remain valid.
/// Backup semantics is required to open a directory handle.
pub(super) fn exclusive_directory_handle(path: &Path) -> io::Result<File> {
    const FILE_LIST_DIRECTORY: u32 = 1;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    OpenOptions::new()
        .access_mode(FILE_LIST_DIRECTORY)
        .share_mode(0)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
}

/// Requires symlinks on CI; permits only a missing local privilege to skip.
pub(super) fn symlink_created(result: io::Result<()>) -> io::Result<bool> {
    match result {
        Ok(()) => Ok(true),
        Err(error) if error.raw_os_error() == Some(1314) && std::env::var_os("CI").is_none() => {
            eprintln!("skipping Windows symlink fixture off CI: ERROR_PRIVILEGE_NOT_HELD: {error}");
            Ok(false)
        }
        Err(error) => Err(error),
    }
}
