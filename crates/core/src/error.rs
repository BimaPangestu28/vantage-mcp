use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    ScreenRecordingPermission,
    AccessibilityPermission,
    NotFound,
    InvalidInput,
    Unsupported,
    Internal,
}

#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("Screen Recording permission not granted to this process")]
    ScreenRecordingPermissionDenied,
    #[error("Accessibility permission not granted to this process")]
    AccessibilityPermissionDenied,
    #[error("window {0} not found")]
    WindowNotFound(crate::types::WindowId),
    #[error("region {0:?} is outside all display bounds")]
    InvalidBounds(crate::types::Bounds),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl CaptureError {
    pub fn kind(&self) -> ErrorKind {
        match self {
            CaptureError::ScreenRecordingPermissionDenied => ErrorKind::ScreenRecordingPermission,
            CaptureError::AccessibilityPermissionDenied => ErrorKind::AccessibilityPermission,
            CaptureError::WindowNotFound(_) => ErrorKind::NotFound,
            CaptureError::InvalidBounds(_) => ErrorKind::InvalidInput,
            CaptureError::Unsupported(_) => ErrorKind::Unsupported,
            CaptureError::Internal(_) => ErrorKind::Internal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Bounds;

    #[test]
    fn every_variant_has_a_distinct_kind() {
        let bounds = Bounds { x: 0, y: 0, width: 1, height: 1 };
        let cases = [
            (CaptureError::ScreenRecordingPermissionDenied, ErrorKind::ScreenRecordingPermission),
            (CaptureError::AccessibilityPermissionDenied, ErrorKind::AccessibilityPermission),
            (CaptureError::WindowNotFound(7), ErrorKind::NotFound),
            (CaptureError::InvalidBounds(bounds), ErrorKind::InvalidInput),
            (CaptureError::Unsupported("x".into()), ErrorKind::Unsupported),
            (CaptureError::Internal("x".into()), ErrorKind::Internal),
        ];
        for (err, expected) in cases {
            assert_eq!(err.kind(), expected);
        }
    }
}
