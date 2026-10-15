// SPDX-License-Identifier: Apache-2.0
//! Persistent, single-attempt Privy authorizations.
//!
//! Each nonce gets an exclusively created tombstone before the signing key is
//! used. A failed or interrupted attempt stays consumed. Never delete markers
//! to retry: resolve the attempt and obtain a new authorization instead.

use std::{
    fmt::Write as _,
    fs::{self, OpenOptions},
    io,
    path::{Path, PathBuf},
};

/// Signer-owned state, configured at startup rather than supplied by a caller.
pub struct ReplayStore {
    directory: PathBuf,
}

impl ReplayStore {
    /// Opens an existing private directory. Provision it independently; a
    /// missing directory must not silently reset replay history.
    ///
    /// # Errors
    /// Refuses missing, non-directory, symlink or (on Unix) non-private state.
    pub fn at(directory: &Path) -> io::Result<Self> {
        let metadata = fs::symlink_metadata(directory)?;
        if !metadata.is_dir() {
            return Err(io::Error::other("nonce state is not a directory"));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(io::Error::other(
                    "nonce directory must be private to the signer",
                ));
            }
        }
        Ok(Self {
            directory: directory.to_path_buf(),
        })
    }

    /// Consumes a nonce atomically across processes, before any signature is
    /// made or returned. Its hash is the filename; caller text is never a path.
    ///
    /// # Errors
    /// Empty/reused nonces and any storage failure refuse the signing attempt.
    /// An I/O failure may leave a consumed marker; safety takes precedence over
    /// transparent retry. Directory metadata is synced on Unix (deployment).
    pub fn claim(&self, nonce: &str) -> io::Result<()> {
        if nonce.trim().is_empty() {
            return Err(io::Error::other("authorization nonce is empty"));
        }
        let digest = ring::digest::digest(&ring::digest::SHA256, nonce.as_bytes());
        let mut name = String::new();
        for byte in digest.as_ref() {
            write!(name, "{byte:02x}").expect("formatting into a String");
        }
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let marker = options.open(self.directory.join(name))?;
        marker.sync_all()?;
        #[cfg(unix)]
        fs::File::open(&self.directory)?.sync_all()?;
        Ok(())
    }
}
