//! `nest search-filtered <file> <query-json> <field> <value> -k K` — exact
//! cosine restricted to the chunks whose `field` == `value` (0x17 meta_index).
//! Query is a JSON array vector, like `search`. Score is real cosine; recall is
//! 1.0 within the filter. An empty result means the file has no meta_index or
//! the (field, value) pair is absent — never a silent whole-corpus fallback.

use anyhow::Result;
use std::path::PathBuf;

use super::util::print_result;

pub fn run(file: PathBuf, query: String, field: String, value: String, k: i32) -> Result<()> {
    let runtime = nest_runtime::MmapNestFile::open(&file)?;
    let qvec: Vec<f32> =
        serde_json::from_str(&query).map_err(|e| anyhow::anyhow!("invalid query JSON: {}", e))?;
    let result = runtime.search_filtered(&qvec, &field, &value, k)?;
    print_result(&result);
    Ok(())
}
