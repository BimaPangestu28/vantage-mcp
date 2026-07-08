use rmcp::ErrorData;
use vantage_core::CaptureError;

/// Map a domain error to an MCP error. Permission-denied variants use
/// `invalid_request` with an actionable fix message and never collapse into
/// `internal_error`.
pub fn to_mcp_error(err: CaptureError) -> ErrorData {
    match err {
        CaptureError::ScreenRecordingPermissionDenied => {
            #[cfg(target_os = "macos")]
            let msg =
                "Screen Recording permission not granted to this process. Grant it in System \
                       Settings > Privacy & Security > Screen Recording, then restart the agent.";
            #[cfg(target_os = "linux")]
            let msg = "Screen capture was denied. On Wayland, approve the screen-capture / \
                       screenshot portal prompt (xdg-desktop-portal) for this application; on X11 \
                       ensure the session allows capture, then retry.";
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            let msg = "Screen capture permission not granted to this process.";
            ErrorData::invalid_request(msg, None)
        }
        CaptureError::AccessibilityPermissionDenied => {
            #[cfg(target_os = "macos")]
            let msg = "Accessibility permission not granted to this process. Grant it in System \
                       Settings > Privacy & Security > Accessibility, then restart the agent.";
            #[cfg(target_os = "linux")]
            let msg = "The accessibility (AT-SPI) bus is unavailable. Enable assistive \
                       technologies / the accessibility bus for this session (e.g. in your \
                       desktop's accessibility settings, or start at-spi2-registryd), then retry.";
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            let msg = "Accessibility support is not available on this platform.";
            ErrorData::invalid_request(msg, None)
        }
        CaptureError::WindowNotFound(id) => {
            ErrorData::invalid_params(format!("window {id} not found"), None)
        }
        CaptureError::InvalidBounds(b) => {
            ErrorData::invalid_params(format!("region {b:?} is outside all display bounds"), None)
        }
        CaptureError::Unsupported(msg) => {
            ErrorData::invalid_request(format!("unsupported on this platform: {msg}"), None)
        }
        CaptureError::Internal(msg) => ErrorData::internal_error(msg, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::ErrorCode;

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_permission_denied_is_not_internal_and_is_actionable() {
        let mapped = to_mcp_error(CaptureError::ScreenRecordingPermissionDenied);
        assert_eq!(mapped.code, ErrorCode::INVALID_REQUEST);
        assert_ne!(mapped.code, ErrorCode::INTERNAL_ERROR);
        assert!(mapped.message.contains("Screen Recording"));
        assert!(mapped.message.to_lowercase().contains("grant"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_permission_text_is_platform_appropriate() {
        let ax = to_mcp_error(CaptureError::AccessibilityPermissionDenied);
        assert_eq!(ax.code, ErrorCode::INVALID_REQUEST);
        assert_ne!(ax.code, ErrorCode::INTERNAL_ERROR);
        // Must not reference macOS System Settings on Linux.
        assert!(!ax.message.contains("System Settings"));
        assert!(ax.message.to_lowercase().contains("accessibility"));

        let sr = to_mcp_error(CaptureError::ScreenRecordingPermissionDenied);
        assert_eq!(sr.code, ErrorCode::INVALID_REQUEST);
        assert!(!sr.message.contains("System Settings"));
        assert!(sr.message.to_lowercase().contains("capture"));
    }

    #[test]
    fn not_found_maps_to_invalid_params() {
        let mapped = to_mcp_error(CaptureError::WindowNotFound(42));
        assert_eq!(mapped.code, ErrorCode::INVALID_PARAMS);
        assert!(mapped.message.contains("42"));
    }
}
