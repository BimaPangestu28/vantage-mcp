use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub type WindowId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct WindowInfo {
    pub window_id: WindowId,
    pub app: String,
    pub title: String,
    pub bounds: Bounds,
    pub focused: bool,
}

#[derive(Debug, Clone)]
pub struct WindowFilter {
    pub app_filter: Option<String>,
    pub on_screen_only: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct WindowText {
    pub text: String,
    pub truncated: bool,
}

/// RGBA8, row-major, `pixels.len() == width * height * 4`.
#[derive(Debug, Clone)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardPrefer {
    Text,
    Image,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ClipboardKind {
    Text,
    Image,
    Empty,
}

#[derive(Debug, Clone)]
pub struct ClipboardContent {
    pub kind: ClipboardKind,
    pub text: Option<String>,
    pub image: Option<RgbaImage>,
}
