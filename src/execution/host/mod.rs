//! Wasmtime host glue shared between the CLI runtime ([`super::wasm_runner`]) and the E2E test
//! harness (`tests/e2e_tests.rs`). Both link against the same `env`/`Dream` imports; only the
//! output sink differs (real stdout vs. a captured buffer).
//!
//! The pieces are split by concern so each capability lives next to the stdlib module it backs:
//!   * [`memory`] - shared string/`char[]` marshaling across the WASM boundary.
//!   * [`file`]   - `src/stdlib/file.dream` (synchronous `std::fs`).
//!   * [`regex`]  - `src/stdlib/regex.dream` (the `regex` crate).
//!   * [`http`]   - `src/stdlib/http.dream` (blocking `reqwest` + the async future bridge).
//!   * [`math`]   - the `Math.*` `env` builtins.

mod file;
mod http;
mod math;
mod memory;
mod regex;

pub use file::link_file_functions;
pub use http::link_http_functions;
pub use math::link_math_functions;
pub use memory::{read_string_from_memory, write_bytes_to_memory, write_string_to_memory};
pub use regex::link_regex_functions;
