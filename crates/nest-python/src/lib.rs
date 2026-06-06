//! PyO3 bindings for `nest_runtime`. Exposes `NestFile.open(path)`,
//! search variants, plus `build()` for emitting `.nest` files from
//! pre-embedded chunks. See `python/nest.py` for the Python wrapper.

use pyo3::prelude::*;

mod build_fn;
mod build_inputs;
mod chunk_id_fn;
mod nest_file;

use build_fn::build;
use chunk_id_fn::chunk_id;
use nest_file::{NestFile, SearchHitPy};

#[pymodule]
fn _nest(m: &Bound<PyModule>) -> PyResult<()> {
    m.add_class::<NestFile>()?;
    m.add_class::<SearchHitPy>()?;
    m.add_function(wrap_pyfunction!(build, m)?)?;
    m.add_function(wrap_pyfunction!(chunk_id, m)?)?;
    Ok(())
}
