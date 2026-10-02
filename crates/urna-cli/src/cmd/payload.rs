//! The embedder payload's contract, shared by `urna setup` (which lays it
//! down) and the query gate (which reads it to name a broken install): the
//! files a complete payload holds and where an installed one lives.

use std::path::{Path, PathBuf};

/// What a complete payload holds, relative to `urna/`: every file
/// `stage_embedder_payload.py` ships, since each query path imports or reads
/// one of them (the potion route reads the table's config, tokenizer and
/// weights; the registry route imports the adapters, the st and image
/// backends and model_fingerprint; search-text runs embed_query.py). setup
/// refuses a payload missing one, and the scan reports an installed payload
/// missing one so setup repairs it. `tests/test_embedder_payload.py` checks
/// that this list and the staged payload are the same set of files.
pub const REQUIRED: [&str; 20] = [
    "VERSION",
    "model_fingerprint.py",
    "embed_query.py",
    "forge/__init__.py",
    "forge/embed_default.py",
    "forge/embed_image.py",
    "forge/embed_potion.py",
    "forge/embed_query_model.py",
    "forge/embed_query_potion.py",
    "forge/embed_st.py",
    "forge/embed_st_worker.py",
    "forge/model_adapters.py",
    "forge/model_registry.py",
    "forge/models/potion-base-8M/README.md",
    "forge/models/potion-base-8M/config.json",
    "forge/models/potion-base-8M/model.safetensors",
    "forge/models/potion-base-8M/modules.json",
    "forge/models/potion-base-8M/special_tokens_map.json",
    "forge/models/potion-base-8M/tokenizer.json",
    "forge/models/potion-base-8M/tokenizer_config.json",
];

/// The required files missing under `home` (`<root>/urna`).
pub fn missing(home: &Path) -> Vec<&'static str> {
    REQUIRED
        .into_iter()
        .filter(|rel| !home.join(rel).is_file())
        .collect()
}

/// The python modules the payload itself provides: an import error naming
/// one of them is a broken payload, never a package to pip install.
pub const MODULES: [&str; 3] = ["forge", "embed_query", "model_fingerprint"];

/// The installed payload a script belongs to (`<root>/urna`), when it is
/// one: `<root>/urna/embed_query.py` or `<root>/urna/forge/<name>`. a
/// checkout's `python/` is not a payload.
pub fn home_of(script: &Path) -> Option<PathBuf> {
    let parent = script.parent()?;
    let home = if parent.file_name()? == "forge" {
        parent.parent()?
    } else {
        parent
    };
    (home.file_name()? == "urna" && home.join("forge").is_dir()).then(|| home.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_of_finds_an_installed_payload_and_not_a_checkout() {
        let d = std::env::temp_dir().join(format!("urna_payload_home_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let home = d.join("data").join("urna");
        std::fs::create_dir_all(home.join("forge")).unwrap();
        std::fs::create_dir_all(d.join("python").join("forge")).unwrap();
        assert_eq!(
            home_of(&home.join("forge/embed_query_model.py")),
            Some(home.clone())
        );
        assert_eq!(home_of(&home.join("embed_query.py")), Some(home.clone()));
        assert_eq!(home_of(&d.join("python/forge/embed_query_model.py")), None);
        assert_eq!(home_of(&d.join("python/embed_query.py")), None);
        assert_eq!(missing(&home).len(), REQUIRED.len());
        let _ = std::fs::remove_dir_all(&d);
    }
}
