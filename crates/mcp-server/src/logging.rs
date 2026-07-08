use tracing_subscriber::{fmt, EnvFilter};

/// Initialize tracing to **stderr only**. stdout is reserved for the JSON-RPC
/// stream; nothing else may write to it.
pub fn init() {
    let filter = EnvFilter::try_from_env("VANTAGE_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(filter)
        .with_ansi(false)
        .init();
}
