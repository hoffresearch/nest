//! Lays a verified payload tarball down under the data root. The archive is
//! unpacked into a staging dir first (entries that would escape it, `..`
//! or absolute paths, abort the install) and checked: every file of
//! `REQUIRED` present, nothing but `forge/` and plain files at the top (so
//! the managed venv beside them can never be overwritten). only then is it
//! laid down, as one restorable step: the previous payload moves aside into
//! the staging dir, the new top-level files go in, then `forge/`, then
//! `VERSION` last, so a stamp only ever names a payload that is complete. a
//! failure at any point removes what was placed and puts the previous
//! payload back.

use std::fs::File;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;

/// The files a payload lays down beside `forge/` (`stage_embedder_payload.py`
/// writes them); `--uninstall` removes exactly these.
pub const TOP_LEVEL: [&str; 3] = ["model_fingerprint.py", "embed_query.py", "VERSION"];

/// What a complete payload holds, relative to `urna/`: the three query
/// embedders, what they import from the payload, the potion table and the
/// stamp. setup refuses a payload missing one, and the scan reports an
/// installed payload missing one so setup repairs it.
/// `tests/test_embedder_payload.py` checks the staged payload against it.
pub const REQUIRED: [&str; 9] = [
    "VERSION",
    "model_fingerprint.py",
    "embed_query.py",
    "forge/__init__.py",
    "forge/embed_query_potion.py",
    "forge/embed_query_model.py",
    "forge/model_registry.py",
    "forge/model_adapters.py",
    "forge/models/potion-base-8M/model.safetensors",
];

/// The required files missing under `home` (`<root>/urna`).
pub fn missing(home: &Path) -> Vec<&'static str> {
    REQUIRED
        .into_iter()
        .filter(|rel| !home.join(rel).is_file())
        .collect()
}

/// Unpacks `tar_gz` into `root` (the parent of `urna/`), calling
/// `on_entry(n)` after each entry; returns the installed forge dir.
pub fn install(tar_gz: &Path, root: &Path, mut on_entry: impl FnMut(usize)) -> Result<PathBuf> {
    std::fs::create_dir_all(root).with_context(|| format!("create {}", root.display()))?;
    let staging = root.join(format!(".urna-setup-{}", std::process::id()));
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    std::fs::create_dir_all(&staging)?;
    let result =
        unpack_into(tar_gz, &staging, &mut on_entry).and_then(|_| swap(&staging, root, None));
    let _ = std::fs::remove_dir_all(&staging);
    result
}

fn unpack_into(tar_gz: &Path, staging: &Path, on_entry: &mut impl FnMut(usize)) -> Result<()> {
    let file = File::open(tar_gz).with_context(|| format!("open {}", tar_gz.display()))?;
    let mut archive = tar::Archive::new(GzDecoder::new(file));
    archive.set_preserve_permissions(false);
    let mut n = 0usize;
    for entry in archive.entries().context("read payload archive")? {
        let mut entry = entry.context("read payload entry")?;
        let path = entry.path()?.into_owned();
        // unpack_in refuses (returns false) anything that would land outside
        // the staging dir; an honest payload never does that, so it is fatal.
        if !entry.unpack_in(staging)? {
            bail!("payload entry escapes the install dir: {}", path.display());
        }
        n += 1;
        on_entry(n);
    }
    Ok(())
}

/// The files at the top of the staged `urna/`, after checking that nothing
/// else but `forge/` sits there.
fn top_level_files(new_home: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(new_home).context("read the staged payload")? {
        let entry = entry?;
        let name = entry.file_name();
        if name == "forge" {
            continue;
        }
        if !entry.file_type()?.is_file() {
            bail!(
                "payload carries urna/{}, which is not a file; refusing to install it",
                name.to_string_lossy()
            );
        }
        files.push(entry.path());
    }
    Ok(files)
}

/// Moves `from` to `to`, replacing a file there (windows refuses a rename
/// onto an existing file).
fn put(from: &Path, to: &Path) -> Result<()> {
    if to.is_file() {
        std::fs::remove_file(to).with_context(|| format!("remove {}", to.display()))?;
    }
    std::fs::rename(from, to)
        .with_context(|| format!("move {} to {}", from.display(), to.display()))
}

fn remove(p: &Path) {
    let _ = if p.is_dir() {
        std::fs::remove_dir_all(p)
    } else {
        std::fs::remove_file(p)
    };
}

/// Lays the staged payload down over `<root>/urna`, restoring the previous
/// one on any failure. `fail_before` names an entry to fail on instead of
/// placing it: the tests use it to stop an upgrade halfway; setup passes
/// `None`.
fn swap(staging: &Path, root: &Path, fail_before: Option<&str>) -> Result<PathBuf> {
    let new_home = staging.join("urna");
    let gone: Vec<&str> = missing(&new_home);
    if !gone.is_empty() {
        bail!(
            "payload is incomplete, missing urna/{}",
            gone.join(", urna/")
        );
    }
    let mut names: Vec<String> = top_level_files(&new_home)?
        .iter()
        .filter_map(|f| f.file_name().map(|n| n.to_string_lossy().into_owned()))
        .filter(|n| n != "VERSION")
        .collect();
    // forge after the modules it imports from, the stamp after everything.
    names.push("forge".into());
    names.push("VERSION".into());
    let home = root.join("urna");
    std::fs::create_dir_all(&home)?;
    let previous = staging.join("previous");
    std::fs::create_dir_all(&previous)?;
    let mut aside: Vec<String> = Vec::new();
    let mut placed: Vec<String> = Vec::new();
    let step = (|| -> Result<()> {
        for n in &names {
            if home.join(n).exists() {
                std::fs::rename(home.join(n), previous.join(n))
                    .with_context(|| format!("move the previous {n} aside"))?;
                aside.push(n.clone());
            }
        }
        for n in &names {
            if fail_before == Some(n.as_str()) {
                bail!("stopped before {n}");
            }
            put(&new_home.join(n), &home.join(n))?;
            placed.push(n.clone());
        }
        Ok(())
    })();
    if let Err(e) = step {
        for n in placed.iter().rev() {
            remove(&home.join(n));
        }
        for n in aside.iter().rev() {
            let _ = std::fs::rename(previous.join(n), home.join(n));
        }
        return Err(
            e.context("installing the payload failed; the previous payload is back in place")
        );
    }
    Ok(home.join("forge"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::GzEncoder;

    fn tarball(dir: &Path, entries: &[(&str, &[u8])]) -> PathBuf {
        let out = dir.join("p.tar.gz");
        let gz = GzEncoder::new(File::create(&out).unwrap(), Compression::fast());
        let mut b = tar::Builder::new(gz);
        for (name, data) in entries {
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(0o644);
            // set_path refuses `..`; write the raw name to model a hostile archive.
            let raw = &mut h.as_old_mut().name;
            raw[..name.len()].copy_from_slice(name.as_bytes());
            h.set_cksum();
            b.append(&h, *data).unwrap();
        }
        b.into_inner().unwrap().finish().unwrap();
        out
    }

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("urna_unpack_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A complete payload stamped `version`, every file's body naming the
    /// release (`<rel>@<version>`), plus `extra` entries.
    fn payload(version: &str, extra: &[(&str, &[u8])]) -> Vec<(String, Vec<u8>)> {
        let mut v: Vec<(String, Vec<u8>)> = REQUIRED
            .iter()
            .map(|rel| {
                let body = if *rel == "VERSION" {
                    format!("{version}\n")
                } else {
                    format!("{rel}@{version}")
                };
                (format!("urna/{rel}"), body.into_bytes())
            })
            .collect();
        v.extend(extra.iter().map(|(n, b)| (n.to_string(), b.to_vec())));
        v
    }

    fn pack(dir: &Path, entries: &[(String, Vec<u8>)]) -> PathBuf {
        let refs: Vec<(&str, &[u8])> = entries
            .iter()
            .map(|(n, b)| (n.as_str(), b.as_slice()))
            .collect();
        tarball(dir, &refs)
    }

    /// Unpacks `entries` into a staging dir under `root` and swaps it in,
    /// stopping before `fail_before`; what `install` does, with the fault.
    fn swap_staged(
        d: &Path,
        root: &Path,
        entries: &[(String, Vec<u8>)],
        fail_before: Option<&str>,
    ) -> Result<PathBuf> {
        let tgz = pack(d, entries);
        let staging = root.join(".urna-setup-test");
        let _ = std::fs::remove_dir_all(&staging);
        std::fs::create_dir_all(&staging).unwrap();
        unpack_into(&tgz, &staging, &mut |_| {}).unwrap();
        let r = swap(&staging, root, fail_before);
        let _ = std::fs::remove_dir_all(&staging);
        r
    }

    fn read(root: &Path, rel: &str) -> String {
        String::from_utf8(std::fs::read(root.join("urna").join(rel)).unwrap()).unwrap()
    }

    #[test]
    fn installs_a_payload_and_keeps_the_venv() {
        let d = tmp("ok");
        let root = d.join("root");
        std::fs::create_dir_all(root.join("urna/venv/bin")).unwrap();
        std::fs::write(root.join("urna/venv/bin/python"), b"venv").unwrap();
        let tgz = pack(&d, &payload("0.5.2", &[]));
        let mut seen = 0;
        let forge = install(&tgz, &root, |n| seen = n).unwrap();
        assert!(forge.join("embed_query_potion.py").is_file());
        assert_eq!(seen, REQUIRED.len());
        assert!(missing(&root.join("urna")).is_empty());
        assert_eq!(read(&root, "VERSION"), "0.5.2\n");
        assert_eq!(read(&root, "venv/bin/python"), "venv");
        assert!(!root.join(".urna-setup-test").exists());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn an_upgrade_replaces_every_file_of_the_previous_release() {
        let d = tmp("upgrade");
        let root = d.join("root");
        swap_staged(&d, &root, &payload("0.5.1", &[]), None).unwrap();
        swap_staged(&d, &root, &payload("0.5.2", &[]), None).unwrap();
        for rel in REQUIRED.iter().filter(|r| **r != "VERSION") {
            assert_eq!(read(&root, rel), format!("{rel}@0.5.2"));
        }
        assert_eq!(read(&root, "VERSION"), "0.5.2\n");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_failed_upgrade_puts_the_previous_payload_back() {
        // stopped after the top-level files went in, and again after forge:
        // either way every file is 0.5.1's again and the stamp says 0.5.1.
        for stop in ["forge", "VERSION"] {
            let d = tmp(&format!("restore_{}", stop.to_lowercase()));
            let root = d.join("root");
            std::fs::create_dir_all(root.join("urna/venv")).unwrap();
            std::fs::write(root.join("urna/venv/marker"), b"keep").unwrap();
            swap_staged(&d, &root, &payload("0.5.1", &[]), None).unwrap();
            let err = swap_staged(&d, &root, &payload("0.5.2", &[]), Some(stop)).unwrap_err();
            assert!(
                format!("{err:#}").contains("previous payload is back"),
                "{err:#}"
            );
            for rel in REQUIRED.iter().filter(|r| **r != "VERSION") {
                assert_eq!(read(&root, rel), format!("{rel}@0.5.1"), "{stop}: {rel}");
            }
            assert_eq!(read(&root, "VERSION"), "0.5.1\n", "{stop}");
            assert_eq!(read(&root, "venv/marker"), "keep");
            std::fs::remove_dir_all(&d).unwrap();
        }
    }

    #[test]
    fn a_failed_first_install_leaves_no_stamp_and_no_half_payload() {
        let d = tmp("first");
        let root = d.join("root");
        assert!(swap_staged(&d, &root, &payload("0.5.2", &[]), Some("VERSION")).is_err());
        assert!(!root.join("urna/VERSION").exists());
        assert!(!root.join("urna/forge").exists());
        assert!(!root.join("urna/embed_query.py").exists());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn refuses_an_incomplete_payload_and_moves_nothing() {
        let d = tmp("incomplete");
        let root = d.join("root");
        swap_staged(&d, &root, &payload("0.5.1", &[]), None).unwrap();
        let mut half = payload("0.5.2", &[]);
        half.retain(|(n, _)| n != "urna/forge/embed_query_model.py");
        let err = swap_staged(&d, &root, &half, None).unwrap_err();
        assert!(
            err.to_string().contains("urna/forge/embed_query_model.py"),
            "{err}"
        );
        assert_eq!(read(&root, "VERSION"), "0.5.1\n");
        assert_eq!(read(&root, "embed_query.py"), "embed_query.py@0.5.1");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn missing_names_what_an_installed_payload_lacks() {
        let d = tmp("missing");
        let root = d.join("root");
        swap_staged(&d, &root, &payload("0.5.2", &[]), None).unwrap();
        std::fs::remove_file(root.join("urna/embed_query.py")).unwrap();
        assert_eq!(missing(&root.join("urna")), vec!["embed_query.py"]);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn refuses_a_payload_that_carries_a_venv_and_moves_nothing() {
        let d = tmp("venv");
        let root = d.join("root");
        swap_staged(&d, &root, &payload("0.5.1", &[]), None).unwrap();
        let hostile = payload("0.5.2", &[("urna/venv/bin/python", b"hostile")]);
        let err = swap_staged(&d, &root, &hostile, None).unwrap_err();
        assert!(err.to_string().contains("urna/venv"), "{err}");
        assert_eq!(
            read(&root, "model_fingerprint.py"),
            "model_fingerprint.py@0.5.1"
        );
        assert!(!root.join("urna/venv").exists());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn rejects_path_traversal_and_leaves_the_old_payload() {
        let d = tmp("evil");
        let root = d.join("root");
        std::fs::create_dir_all(root.join("urna/forge")).unwrap();
        std::fs::write(root.join("urna/forge/keep"), b"old").unwrap();
        let tgz = tarball(&d, &[("../../escaped", b"x")]);
        assert!(install(&tgz, &root, |_| {}).is_err());
        assert!(!d.join("escaped").exists());
        assert!(root.join("urna/forge/keep").is_file());
        std::fs::remove_dir_all(&d).unwrap();
    }
}
