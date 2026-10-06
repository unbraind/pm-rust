//! Windows fixture policy shared by filesystem fault tests.

use std::io;

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
