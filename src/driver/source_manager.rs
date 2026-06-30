//! Compatibility facade re-exporting the source-loading and prelude-merge APIs that previously
//! lived in this single god module (consumed by the external `dream-lsp` tooling crate). The
//! implementations now live in the focused [`source_loader`](crate::driver::source_loader),
//! [`prelude`](crate::driver::prelude), and [`json_derive`](crate::driver::json_derive) modules;
//! in-crate callers use those directly.

pub use crate::driver::prelude::merge_prelude;
pub use crate::driver::source_loader::{
    collect_declarations, parse_file_recursive, resolve_import_path, ProgramAccumulator,
};
