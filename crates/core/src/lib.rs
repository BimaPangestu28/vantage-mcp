//! Platform-agnostic capability traits, value types, and errors for vantage-mcp.
pub mod error;
pub mod traits;
pub mod types;

pub use error::{CaptureError, ErrorKind};
pub use traits::{ClipboardAccess, ScreenCapturer, TextRecognizer, WindowInspector};
pub use types::*;
