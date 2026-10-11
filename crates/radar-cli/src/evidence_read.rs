// SPDX-License-Identifier: Apache-2.0
//! Operator collection of the two read packets consumed by the offline issuer.
//! This does not fill unknown valuation, risk state or costs in an issuer snapshot.

use std::io::Write as _;
use std::path::Path;
use std::time::Duration;

use radar_onchain::{Budget, RpcClient};
use radar_types::Address;
use serde_json::json;

pub fn run(args: &[String]) -> Result<(), String> {
    let wallet: Address = crate::flag(args, "--wallet")
        .and_then(|value| value.parse().ok())
        .ok_or("evidence-read needs --wallet <address>")?;
    let path = crate::flag(args, "--transaction")
        .ok_or("evidence-read needs --transaction <binary-file>")?;
    let minimum = crate::flag(args, "--min-slot")
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or("evidence-read needs --min-slot <u64>")?;
    let endpoint = crate::flag(args, "--rpc")
        .filter(|value| !value.trim().is_empty())
        .ok_or("evidence-read needs an explicit --rpc <URL>")?;
    let output = crate::flag(args, "--output");
    if args.iter().any(|arg| arg == "--output") && output.is_none() {
        return Err("evidence-read needs --output <private-file>".into());
    }
    let bytes = crate::transaction_read::bytes(&path)?;
    let rpc = RpcClient::new(endpoint);
    let mut budget = Budget::new(6, 0, Duration::from_secs(20));
    let started = crate::wallet_read::now()?;
    let mut wallet_evidence = crate::wallet_read::read(&rpc, wallet, &mut budget)?;
    wallet_evidence["read_started_at_unix_secs"] = json!(started);
    wallet_evidence["read_completed_at_unix_secs"] = json!(crate::wallet_read::now()?);
    let transaction_started = crate::wallet_read::now()?;
    let mut transaction_evidence = rpc.transaction_preflight(&bytes, minimum, &mut budget)?;
    transaction_evidence["read_started_at_unix_secs"] = json!(transaction_started);
    transaction_evidence["read_completed_at_unix_secs"] = json!(crate::wallet_read::now()?);
    // A later failure must not leave a partial packet looking ready for issuance.
    let packet =
        json!({"wallet_evidence":wallet_evidence,"transaction_evidence":transaction_evidence});
    if let Some(path) = output {
        publish(Path::new(&path), &packet)?;
        println!(
            "{}",
            json!({"evidence_written":true,"authority":"read_only"})
        );
    } else {
        println!("{packet}");
    }
    Ok(())
}

// A failed collection never reaches this function. Publication replaces the
// whole packet, so an issuer cannot observe one new half and one old half.
fn publish(path: &Path, packet: &serde_json::Value) -> Result<(), String> {
    let bytes = packet.to_string().into_bytes();
    if bytes.len() > 1_048_576 {
        return Err("evidence packet exceeds issuer read limit".into());
    }
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or("evidence output needs an explicit private directory")?;
    let metadata =
        std::fs::symlink_metadata(parent).map_err(|_| "evidence output directory unavailable")?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("evidence output directory must be a regular directory".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err("evidence output directory must be private".into());
        }
    }
    let mut temporary_name = path
        .file_name()
        .ok_or("evidence output filename missing")?
        .to_os_string();
    temporary_name.push(format!(".{}.pending", std::process::id()));
    let temporary = parent.join(temporary_name);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|_| "evidence temporary file unavailable")?;
    let result = (|| {
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|_| "evidence output write failed")?;
        drop(file);
        std::fs::rename(&temporary, path).map_err(|_| "evidence output publication failed")
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_publication_replaces_whole_packets_and_preserves_previous_on_failure() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("observations.json");
        let first = json!({"wallet_evidence":{"version":1},"transaction_evidence":{"version":1}});
        let next = json!({"wallet_evidence":{"version":2},"transaction_evidence":{"version":2}});
        publish(&path, &first).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        publish(&path, &next).unwrap();
        assert!(publish(&path, &json!({"oversized":"x".repeat(1_048_576)})).is_err());
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&std::fs::read(&path).unwrap()).unwrap(),
            next
        );
        let temporary = directory
            .path()
            .join(format!("observations.json.{}.pending", std::process::id()));
        std::fs::write(&temporary, b"existing writer").unwrap();
        assert!(publish(&path, &first).is_err());
        assert_eq!(std::fs::read(&temporary).unwrap(), b"existing writer");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&std::fs::read(&path).unwrap()).unwrap(),
            next
        );
        let destination = directory.path().join("directory");
        std::fs::create_dir(&destination).unwrap();
        assert!(publish(&destination, &first).is_err());
        assert!(
            !directory
                .path()
                .join(format!("directory.{}.pending", std::process::id()))
                .exists()
        );
        assert!(publish(Path::new("no-explicit-directory.json"), &first).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn publication_refuses_shared_or_symlinked_directories() {
        use std::os::unix::fs::{PermissionsExt as _, symlink};
        let directory = tempfile::tempdir().unwrap();
        let shared = directory.path().join("shared");
        std::fs::create_dir(&shared).unwrap();
        std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(publish(&shared.join("packet"), &json!({})).is_err());
        assert!(!shared.join("packet").exists());
        let linked = directory.path().join("linked");
        symlink(directory.path(), &linked).unwrap();
        assert!(publish(&linked.join("packet"), &json!({})).is_err());
        assert!(!directory.path().join("packet").exists());
    }

    #[test]
    fn missing_or_invalid_collection_scope_refuses_before_reading_files_or_network() {
        let valid = [
            "--wallet",
            "11111111111111111111111111111111",
            "--transaction",
            "absent",
            "--min-slot",
            "0",
            "--rpc",
            "http://127.0.0.1:1",
        ]
        .map(str::to_owned);
        let mut missing_output = valid.to_vec();
        missing_output.push("--output".into());
        assert_eq!(
            run(&missing_output),
            Err("evidence-read needs --output <private-file>".into())
        );
        for (index, value, expected) in [
            (1, "invalid", "evidence-read needs --wallet <address>"),
            (5, "bad", "evidence-read needs --min-slot <u64>"),
            (7, " ", "evidence-read needs an explicit --rpc <URL>"),
        ] {
            let mut args = valid.clone();
            args[index] = value.into();
            assert_eq!(run(&args), Err(expected.into()));
        }
        for (length, expected) in [
            (0, "evidence-read needs --wallet <address>"),
            (2, "evidence-read needs --transaction <binary-file>"),
            (4, "evidence-read needs --min-slot <u64>"),
            (6, "evidence-read needs an explicit --rpc <URL>"),
            (8, "transaction file unavailable"),
        ] {
            assert_eq!(run(&valid[..length]), Err(expected.into()));
        }
    }
}
