//! Lays a verified payload tarball down under the data root. The archive is
//! unpacked into a staging dir first (entries that would escape it, `..`
//! or absolute paths, abort the install), checked for the embedder script,
//! and only then laid down: the files at the payload's top level
//! (`model_fingerprint.py`, `embed_query.py`, `VERSION`) replace their old
//! copies in `<root>/urna/`, then `forge/` is swapped in whole. the managed
//! venv next to them is never touched; a payload that carries one, or any
//! other directory beside `forge/`, is refused before anything moves.

use std::fs::File;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;

/// Unpacks `tar_gz` into `root` (the parent of `urna/`), calling
/// `on_entry(n)` after each entry; returns the installed forge dir.
pub fn install(tar_gz: &Path, root: &Path, mut on_entry: impl FnMut(usize)) -> Result<PathBuf> {
    std::fs::create_dir_all(root).with_context(|| format!("create {}", root.display()))?;
    let staging = root.join(format!(".urna-setup-{}", std::process::id()));
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    std::fs::create_dir_all(&staging)?;
    let result = unpack_into(tar_gz, &staging, &mut on_entry).and_then(|_| swap(&staging, root));
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

fn swap(staging: &Path, root: &Path) -> Result<PathBuf> {
    let new_home = staging.join("urna");
    let new_forge = new_home.join("forge");
    if !new_forge.join("embed_query_potion.py").is_file() {
        bail!("payload has no urna/forge/embed_query_potion.py");
    }
    let files = top_level_files(&new_home)?;
    let home = root.join("urna");
    std::fs::create_dir_all(&home)?;
    for f in files {
        let dst = home.join(f.file_name().unwrap_or_default());
        // windows refuses a rename onto an existing file.
        if dst.is_file() {
            std::fs::remove_file(&dst).with_context(|| format!("remove old {}", dst.display()))?;
        }
        std::fs::rename(&f, &dst).with_context(|| format!("move payload to {}", dst.display()))?;
    }
    let forge = home.join("forge");
    if forge.exists() {
        std::fs::remove_dir_all(&forge)
            .with_context(|| format!("remove old {}", forge.display()))?;
    }
    std::fs::rename(&new_forge, &forge)
        .with_context(|| format!("move payload to {}", forge.display()))?;
    Ok(forge)
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

    #[test]
    fn installs_a_payload_and_keeps_the_venv() {
        let d = tmp("ok");
        let root = d.join("root");
        std::fs::create_dir_all(root.join("urna/venv")).unwrap();
        let tgz = tarball(
            &d,
            &[
                ("urna/forge/embed_query_potion.py", b"print(1)"),
                ("urna/forge/models/x", b"t"),
            ],
        );
        let mut seen = 0;
        let forge = install(&tgz, &root, |n| seen = n).unwrap();
        assert!(forge.join("embed_query_potion.py").is_file());
        assert!(root.join("urna/venv").is_dir());
        assert_eq!(seen, 2);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn installs_the_top_level_modules_and_replaces_old_copies() {
        let d = tmp("top");
        let root = d.join("root");
        std::fs::create_dir_all(root.join("urna/venv/bin")).unwrap();
        std::fs::write(root.join("urna/venv/bin/python"), b"venv").unwrap();
        std::fs::write(root.join("urna/model_fingerprint.py"), b"old").unwrap();
        let tgz = tarball(
            &d,
            &[
                ("urna/model_fingerprint.py", b"new fingerprint"),
                ("urna/embed_query.py", b"new st embedder"),
                ("urna/VERSION", b"0.5.2"),
                ("urna/forge/embed_query_potion.py", b"print(1)"),
            ],
        );
        install(&tgz, &root, |_| {}).unwrap();
        let read = |p: &str| std::fs::read(root.join(p)).unwrap();
        assert_eq!(read("urna/model_fingerprint.py"), b"new fingerprint");
        assert_eq!(read("urna/embed_query.py"), b"new st embedder");
        assert_eq!(read("urna/VERSION"), b"0.5.2");
        assert_eq!(read("urna/venv/bin/python"), b"venv");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn refuses_a_payload_that_carries_a_venv_and_moves_nothing() {
        let d = tmp("venv");
        let root = d.join("root");
        std::fs::create_dir_all(root.join("urna/forge")).unwrap();
        std::fs::write(root.join("urna/forge/keep"), b"old").unwrap();
        std::fs::write(root.join("urna/model_fingerprint.py"), b"old").unwrap();
        let tgz = tarball(
            &d,
            &[
                ("urna/model_fingerprint.py", b"new"),
                ("urna/venv/bin/python", b"hostile"),
                ("urna/forge/embed_query_potion.py", b"print(1)"),
            ],
        );
        let err = install(&tgz, &root, |_| {}).unwrap_err();
        assert!(err.to_string().contains("urna/venv"), "{err}");
        assert!(root.join("urna/forge/keep").is_file());
        assert_eq!(
            std::fs::read(root.join("urna/model_fingerprint.py")).unwrap(),
            b"old"
        );
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn rejects_a_payload_without_the_embedder() {
        let d = tmp("empty");
        let tgz = tarball(&d, &[("urna/readme", b"x")]);
        let err = install(&tgz, &d.join("root"), |_| {}).unwrap_err();
        assert!(err.to_string().contains("embed_query_potion.py"));
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
