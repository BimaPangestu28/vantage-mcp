use std::sync::Arc;

use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use rmcp::{tool_handler, tool_router, ServerHandler};

use vantage_core::{ClipboardAccess, ScreenCapturer, TextRecognizer, WindowInspector};

/// The MCP server handler. Holds injected, platform-agnostic backends.
/// Tool methods are added in later tasks; this establishes the wiring.
// The backend fields are unused until tool methods are added in later tasks
// (Task 4+); they are wired in now so the constructor and injection points
// are stable.
#[allow(dead_code)]
#[derive(Clone)]
pub struct Vantage {
    pub(crate) windows: Arc<dyn WindowInspector>,
    pub(crate) capturer: Arc<dyn ScreenCapturer>,
    pub(crate) ocr: Arc<dyn TextRecognizer>,
    pub(crate) clipboard: Arc<dyn ClipboardAccess>,
}

// `#[tool_router]` generates `Self::tool_router()`, an associated function
// returning a `ToolRouter<Self>` built from every method in this impl block
// tagged `#[tool]`. There are none yet, so it returns an empty-but-valid
// router. `#[tool_handler]` below calls `Self::tool_router()` automatically,
// so no field is needed on the struct to hold it.
#[tool_router]
impl Vantage {
    pub fn new(
        windows: Arc<dyn WindowInspector>,
        capturer: Arc<dyn ScreenCapturer>,
        ocr: Arc<dyn TextRecognizer>,
        clipboard: Arc<dyn ClipboardAccess>,
    ) -> Self {
        Self {
            windows,
            capturer,
            ocr,
            clipboard,
        }
    }
}

#[tool_handler]
impl ServerHandler for Vantage {
    fn get_info(&self) -> ServerInfo {
        let capabilities = ServerCapabilities::builder().enable_tools().build();
        ServerInfo::new(capabilities)
            .with_server_info(Implementation::new("vantage-mcp", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Desktop capture for macOS. Prefer read_window_text over screenshots; \
                 capture_region defaults to OCR text (no image) to keep token cost low.",
            )
    }
}
