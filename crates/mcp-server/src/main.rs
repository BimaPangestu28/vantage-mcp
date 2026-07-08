mod error_map;
mod handler;
mod image_out;
mod logging;

use anyhow::Result;
use rmcp::{transport::stdio, ServiceExt};

use handler::Vantage;

#[cfg(target_os = "macos")]
use vantage_platform_macos as backend;
#[cfg(target_os = "linux")]
use vantage_platform_linux as backend;
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
compile_error!("vantage-mcp supports macOS and Linux only");

#[tokio::main]
async fn main() -> Result<()> {
    logging::init();
    tracing::info!("vantage-mcp starting (stdio); logging on stderr only");

    let (windows, capturer, ocr, clipboard) = backend::backends();

    let service = Vantage::new(windows, capturer, ocr, clipboard)
        .serve(stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}
