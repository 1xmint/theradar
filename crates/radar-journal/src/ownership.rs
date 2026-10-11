// SPDX-License-Identifier: Apache-2.0
//! Shared cooperative ownership; domain replay stays in each journal caller.

use std::path::{Path, PathBuf};

use crate::JournalError;

pub(crate) struct Owner {
    pub(crate) file: std::fs::File,
    pub(crate) process: u32,
}

impl Owner {
    pub(crate) fn acquire(path: PathBuf) -> Result<(Self, PathBuf), JournalError> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let directory = parent.canonicalize().map_err(|source| JournalError::Io {
            path: parent.display().to_string(),
            source,
        })?;
        let name = path.file_name().ok_or_else(|| JournalError::Io {
            path: path.display().to_string(),
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "an operation journal needs a filename",
            ),
        })?;
        let path = directory.join(name);
        let mut lock_path = path.as_os_str().to_os_string();
        lock_path.push(".lock");
        let lock_path = PathBuf::from(lock_path);
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let file = options
            .open(&lock_path)
            .map_err(|source| JournalError::Io {
                path: lock_path.display().to_string(),
                source,
            })?;
        file.try_lock().map_err(|source| JournalError::Io {
            path: lock_path.display().to_string(),
            source: source.into(),
        })?;
        Ok((
            Self {
                file,
                process: std::process::id(),
            },
            path,
        ))
    }

    pub(crate) fn release(&self) {
        // An inherited descriptor must not retain a dropped owner's lock, and
        // a forked child's destructor must not unlock its parent's ownership.
        if self.process == std::process::id() {
            let _ = self.file.unlock();
        }
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        self.release();
    }
}
