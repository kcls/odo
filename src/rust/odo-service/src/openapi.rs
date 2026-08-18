//! Shared OpenAPI spec-dump support.
//!
//! Each service builds its own `#[derive(OpenApi)] ApiDoc` (the paths and
//! components are inherently per-service), but the *dump* mechanism is
//! shared: run the binary with `--dump-openapi <path>` to write the spec to
//! disk as pretty JSON and exit, without connecting to a database or any
//! other service. The UI build generates TypeScript types from the
//! committed spec files, so this is the source of truth for the client
//! types — it never talks to a running service.

use utoipa::openapi::OpenApi;

/// CLI flag that triggers a spec dump.
pub const DUMP_FLAG: &str = "--dump-openapi";

/// If argv contains `--dump-openapi <path>`, write `doc` to that path as
/// pretty JSON and return `Some(exit_code)` — the caller should
/// `std::process::exit` with it immediately, before any DB/network setup.
/// Returns `None` when the flag is absent, so normal startup proceeds.
///
/// Usage at the top of a service `main`:
/// ```ignore
/// if let Some(code) = odo::openapi::maybe_dump(build_doc()) {
///     std::process::exit(code);
/// }
/// ```
pub fn maybe_dump(doc: OpenApi) -> Option<i32> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == DUMP_FLAG {
            let path = match args.next() {
                Some(p) => p,
                None => {
                    eprintln!("{DUMP_FLAG} requires a file path argument");
                    return Some(1);
                }
            };
            return Some(write_spec(&doc, &path));
        }
    }
    None
}

fn write_spec(doc: &OpenApi, path: &str) -> i32 {
    let json = match doc.to_pretty_json() {
        Ok(j) => j,
        Err(e) => {
            eprintln!("failed to serialize OpenAPI spec: {e}");
            return 1;
        }
    };
    match std::fs::write(path, format!("{json}\n")) {
        Ok(()) => {
            eprintln!("wrote OpenAPI spec to {path}");
            0
        }
        Err(e) => {
            eprintln!("failed to write {path}: {e}");
            1
        }
    }
}
