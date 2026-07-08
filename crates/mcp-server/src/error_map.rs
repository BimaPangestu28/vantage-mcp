use rmcp::ErrorData;
use vantage_core::CaptureError;

/// Map a domain error to an MCP error. Permission-denied variants use
/// `invalid_request` with an actionable fix message and never collapse into
/// `internal_error`.
// Not yet called from `main`/`handler` — wired in once tool methods exist
// (Task 4+) to convert `CaptureError` results into `rmcp::ErrorData`.
#[allow(dead_code)]
pub fn to_mcp_error(err: CaptureError) -> ErrorData {
    match err {
        CaptureError::ScreenRecordingPermissionDenied => ErrorData::invalid_request(
            "Screen Recording permission not granted to this process. Grant it in System \
             Settings > Privacy & Security > Screen Recording, then restart the agent.",
            None,
        ),
        CaptureError::AccessibilityPermissionDenied => ErrorData::invalid_request(
            "Accessibility permission not granted to this process. Grant it in System \
             Settings > Privacy & Security > Accessibility, then restart the agent.",
            None,
        ),
        CaptureError::WindowNotFound(id) => {
            ErrorData::invalid_params(format!("window {id} not found"), None)
        }
        CaptureError::InvalidBounds(b) => ErrorData::invalid_params(
            format!("region {b:?} is outside all display bounds"),
            None,
        ),
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

    #[test]
    fn permission_denied_is_not_internal_and_is_actionable() {
        let mapped = to_mcp_error(CaptureError::ScreenRecordingPermissionDenied);
        assert_eq!(mapped.code, ErrorCode::INVALID_REQUEST);
        assert_ne!(mapped.code, ErrorCode::INTERNAL_ERROR);
        assert!(mapped.message.contains("Screen Recording"));
        assert!(mapped.message.to_lowercase().contains("grant"));
    }

    #[test]
    fn not_found_maps_to_invalid_params() {
        let mapped = to_mcp_error(CaptureError::WindowNotFound(42));
        assert_eq!(mapped.code, ErrorCode::INVALID_PARAMS);
        assert!(mapped.message.contains("42"));
    }
}
