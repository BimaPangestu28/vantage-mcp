mod error_map;
mod handler;
mod logging;
mod stub_backends;

use std::sync::Arc;

use anyhow::Result;
use rmcp::{transport::stdio, ServiceExt};

use handler::Vantage;

// TEMPORARY — the real macOS backends (vantage_platform_macos::Mac*) land in
// Tasks 8-11 and wire in here in Task 12. Until then these stub backends let
// the server boot and serve the (empty) tool set today. See stub_backends.rs.
use stub_backends::{StubClipboard, StubScreenCapturer, StubTextRecognizer, StubWindowInspector};

#[tokio::main]
async fn main() -> Result<()> {
    logging::init();
    tracing::info!("vantage-mcp starting (stdio); logging on stderr only");

    // TEMPORARY backends — see stub_backends.rs. Replaced by real macOS
    // backends in Task 12.
    let windows = Arc::new(StubWindowInspector);
    let capturer = Arc::new(StubScreenCapturer);
    let ocr = Arc::new(StubTextRecognizer);
    let clipboard = Arc::new(StubClipboard);

    let service = Vantage::new(windows, capturer, ocr, clipboard)
        .serve(stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}
