//! CLI subcommand implementations. One module per subcommand,
//! orchestrated by `main::Commands`. Shared helpers in `util`.

pub mod benchmark;
pub mod cite;
pub mod inspect;
pub mod search;
pub mod search_ann;
pub mod search_graph;
pub mod search_text;
pub mod stats;
pub mod util;
pub mod validate;
