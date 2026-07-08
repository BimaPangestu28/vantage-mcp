mod error_map;
mod handler;
mod image_out;
mod logging;

use std::sync::Arc;

use anyhow::Result;
use rmcp::{transport::stdio, ServiceExt};

use handler::Vantage;
use vantage_platform_macos as backend;

#[tokio::main]
async fn main() -> Result<()> {
    logging::init();
    tracing::info!("vantage-mcp starting (stdio); logging on stderr only");

    let windows = Arc::new(backend::MacWindowInspector::new());
    let capturer = Arc::new(backend::MacScreenCapturer::new());
    let ocr = Arc::new(backend::MacTextRecognizer::new());
    let clipboard = Arc::new(backend::MacClipboard::new());

    let service = Vantage::new(windows, capturer, ocr, clipboard)
        .serve(stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}
