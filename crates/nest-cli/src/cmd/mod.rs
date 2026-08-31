//! CLI subcommand implementations. One module per subcommand,
//! orchestrated by `main::Commands`. Shared helpers in `util`.

pub mod ask;
pub mod benchmark;
pub mod build;
pub mod cite;
pub mod doctor;
pub mod embed_gate;
pub mod inspect;
pub mod pyenv;
pub mod retrieve;
pub mod search;
pub mod search_ann;
pub mod search_graph;
pub mod search_space;
pub mod search_text;
pub mod stats;
pub mod util;
pub mod validate;
