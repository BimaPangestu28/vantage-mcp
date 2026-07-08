//! macOS `WindowInspector` backend.
//!
//! Window enumeration is done through Quartz `CGWindowListCopyWindowInfo`, and
//! per-window text is read from the Accessibility (AX) tree via `AXUIElement`.
//!
//! Two distinct permission surfaces are involved and are reported distinctly:
//! - Window *titles* (`kCGWindowName`) require Screen Recording permission; when
//!   it is absent the title is simply empty (not an error).
//! - Reading the AX tree requires Accessibility permission; its absence is
//!   surfaced as [`CaptureError::AccessibilityPermissionDenied`].

use accessibility::{AXAttribute, AXUIElement, Error as AxError};
use accessibility_sys::{
    kAXErrorAPIDisabled, kAXErrorNotImplemented, pid_t, AXIsProcessTrusted,
};
use core_foundation::base::{CFType, TCFType};
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::number::CFNumber;
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::geometry::CGRect;
use core_graphics::window::{
    copy_window_info, kCGNullWindowID, kCGWindowBounds, kCGWindowLayer, kCGWindowListOptionAll,
    kCGWindowListOptionOnScreenOnly, kCGWindowName, kCGWindowNumber, kCGWindowOwnerName,
    kCGWindowOwnerPID,
};
use objc2_app_kit::NSWorkspace;

use vantage_core::{
    Bounds, CaptureError, WindowFilter, WindowId, WindowInfo, WindowInspector, WindowText,
};

/// Maximum number of AX nodes visited by a single `read_window_text` call.
///
/// Bounds the cost of pathological trees; hitting it sets `truncated = true`.
const AX_NODE_BUDGET: usize = 2000;

/// macOS implementation of [`WindowInspector`].
///
/// Stateless; all OS handles are created per-call, so the type is trivially
/// `Send + Sync`.
pub struct MacWindowInspector;

impl MacWindowInspector {
    /// Creates a new inspector.
    pub fn new() -> Self {
        Self
    }
}

impl Default for MacWindowInspector {
    fn default() -> Self {
        Self::new()
    }
}

/// A single window as read from the Quartz window list.
struct WindowRecord {
    window_id: WindowId,
    app: String,
    title: String,
    bounds: Bounds,
    owner_pid: pid_t,
    layer: i64,
}

impl WindowInspector for MacWindowInspector {
    fn list_windows(&self, filter: WindowFilter) -> Result<Vec<WindowInfo>, CaptureError> {
        let records = read_window_records(filter.on_screen_only)?;
        let frontmost_pid = frontmost_application_pid();

        let windows = records
            .into_iter()
            // Layer 0 is the normal application-window layer; non-zero layers are
            // system chrome (menu bar, Dock, wallpaper, shadows) and are skipped.
            .filter(|record| record.layer == 0)
            .filter(|record| match &filter.app_filter {
                Some(wanted_app) => &record.app == wanted_app,
                None => true,
            })
            .map(|record| WindowInfo {
                window_id: record.window_id,
                focused: frontmost_pid == Some(record.owner_pid),
                app: record.app,
                title: record.title,
                bounds: record.bounds,
            })
            .collect();

        Ok(windows)
    }

    fn read_window_text(&self, window_id: WindowId, depth: u32) -> Result<WindowText, CaptureError> {
        // A missing/false trust flag is the authoritative signal that
        // Accessibility permission has not been granted to this process.
        if !unsafe { AXIsProcessTrusted() } {
            return Err(CaptureError::AccessibilityPermissionDenied);
        }

        let target = resolve_window(window_id)?;
        let application = AXUIElement::application(target.owner_pid);
        let window_element = locate_window_element(&application, &target, window_id)?;

        let mut walk = TextWalk::new();
        walk.visit(&window_element, depth);

        Ok(WindowText { text: walk.text, truncated: walk.truncated })
    }
}

/// Reads the raw Quartz window list, mapping each dictionary into a
/// [`WindowRecord`]. Entries missing an id, owner name, or resolvable bounds are
/// skipped rather than failing the whole call.
fn read_window_records(on_screen_only: bool) -> Result<Vec<WindowRecord>, CaptureError> {
    let list_option = if on_screen_only {
        kCGWindowListOptionOnScreenOnly
    } else {
        kCGWindowListOptionAll
    };

    let window_dicts = copy_window_info(list_option, kCGNullWindowID).ok_or_else(|| {
        CaptureError::Internal("CGWindowListCopyWindowInfo returned null".into())
    })?;

    let mut records = Vec::with_capacity(window_dicts.len() as usize);
    for index in 0..window_dicts.len() {
        let Some(entry) = window_dicts.get(index) else {
            continue;
        };
        // Each entry is a CFDictionary<CFString, CFType> owned by the array; wrap
        // it under the get rule so the retain count stays balanced.
        let dictionary = unsafe {
            CFDictionary::<CFString, CFType>::wrap_under_get_rule(*entry as CFDictionaryRef)
        };
        if let Some(record) = window_record_from_dict(&dictionary) {
            records.push(record);
        }
    }

    Ok(records)
}

/// Extracts a [`WindowRecord`] from one Quartz window dictionary.
///
/// Returns `None` when the mandatory `kCGWindowNumber`, `kCGWindowOwnerName`,
/// `kCGWindowOwnerPID`, or `kCGWindowBounds` keys are absent or malformed.
fn window_record_from_dict(dictionary: &CFDictionary<CFString, CFType>) -> Option<WindowRecord> {
    let window_id = dictionary_i64(dictionary, unsafe { kCGWindowNumber })? as WindowId;
    let app = dictionary_string(dictionary, unsafe { kCGWindowOwnerName })?;
    let owner_pid = dictionary_i64(dictionary, unsafe { kCGWindowOwnerPID })? as pid_t;
    let bounds = dictionary_bounds(dictionary, unsafe { kCGWindowBounds })?;
    // Titles need Screen Recording permission; treat absence as an empty title.
    let title = dictionary_string(dictionary, unsafe { kCGWindowName }).unwrap_or_default();
    // Absent layer is treated as the normal window layer (0).
    let layer = dictionary_i64(dictionary, unsafe { kCGWindowLayer }).unwrap_or(0);

    Some(WindowRecord { window_id, app, title, bounds, owner_pid, layer })
}

/// Reads an integer-valued key from a Quartz window dictionary.
fn dictionary_i64(dictionary: &CFDictionary<CFString, CFType>, key: CFStringRef) -> Option<i64> {
    let key = unsafe { CFString::wrap_under_get_rule(key) };
    let value = dictionary.find(&key)?;
    value.downcast::<CFNumber>()?.to_i64()
}

/// Reads a string-valued key from a Quartz window dictionary.
fn dictionary_string(
    dictionary: &CFDictionary<CFString, CFType>,
    key: CFStringRef,
) -> Option<String> {
    let key = unsafe { CFString::wrap_under_get_rule(key) };
    let value = dictionary.find(&key)?;
    Some(value.downcast::<CFString>()?.to_string())
}

/// Reads the `kCGWindowBounds` sub-dictionary and converts it to [`Bounds`].
fn dictionary_bounds(
    dictionary: &CFDictionary<CFString, CFType>,
    key: CFStringRef,
) -> Option<Bounds> {
    let key = unsafe { CFString::wrap_under_get_rule(key) };
    let value = dictionary.find(&key)?;
    // The bounds value is itself a CFDictionary in CGRect dictionary form.
    let bounds_dict =
        unsafe { CFDictionary::wrap_under_get_rule(value.as_CFTypeRef() as CFDictionaryRef) };
    let rect = CGRect::from_dict_representation(&bounds_dict)?;
    Some(cgrect_to_bounds(&rect))
}

/// Converts a `CGRect` (floating-point, top-left origin) to integer [`Bounds`],
/// rounding to the nearest device pixel and clamping negative sizes to zero.
fn cgrect_to_bounds(rect: &CGRect) -> Bounds {
    Bounds {
        x: rect.origin.x.round() as i32,
        y: rect.origin.y.round() as i32,
        width: rect.size.width.round().max(0.0) as u32,
        height: rect.size.height.round().max(0.0) as u32,
    }
}

/// Finds the [`WindowRecord`] for `window_id` across all windows (on-screen or
/// not), returning [`CaptureError::WindowNotFound`] when it is gone.
fn resolve_window(window_id: WindowId) -> Result<WindowRecord, CaptureError> {
    read_window_records(false)?
        .into_iter()
        .find(|record| record.window_id == window_id)
        .ok_or(CaptureError::WindowNotFound(window_id))
}

/// Locates the `AXWindow` element matching `target` within its owning
/// application.
///
/// Matching prefers an exact title match; otherwise it falls back to the
/// application's first window so best-effort text extraction still works when
/// the title is unavailable.
fn locate_window_element(
    application: &AXUIElement,
    target: &WindowRecord,
    window_id: WindowId,
) -> Result<AXUIElement, CaptureError> {
    let windows = application
        .attribute(&AXAttribute::windows())
        .map_err(|error| map_ax_error(error, window_id))?;

    if windows.is_empty() {
        return Err(CaptureError::WindowNotFound(window_id));
    }

    if !target.title.is_empty() {
        for window in windows.iter() {
            if let Ok(title) = window.attribute(&AXAttribute::title()) {
                if title == target.title {
                    return Ok(window.clone());
                }
            }
        }
    }

    // Fall back to the first window (title unavailable or unmatched).
    windows
        .get(0)
        .map(|window| window.clone())
        .ok_or(CaptureError::WindowNotFound(window_id))
}

/// Maps an accessibility-crate error to a [`CaptureError`].
///
/// `kAXErrorAPIDisabled`/`kAXErrorNotImplemented` mean the AX API is
/// unavailable to this process (permission not granted); anything else is
/// treated as the window no longer being resolvable.
fn map_ax_error(error: AxError, window_id: WindowId) -> CaptureError {
    match error {
        AxError::Ax(code) if code == kAXErrorAPIDisabled || code == kAXErrorNotImplemented => {
            CaptureError::AccessibilityPermissionDenied
        }
        _ => CaptureError::WindowNotFound(window_id),
    }
}

/// Returns the process id of the frontmost application, if any.
fn frontmost_application_pid() -> Option<pid_t> {
    let workspace = NSWorkspace::sharedWorkspace();
    workspace
        .frontmostApplication()
        .map(|application| application.processIdentifier())
}

/// Depth-first accumulator for AX text attributes, bounded by depth and a node
/// budget.
struct TextWalk {
    text: String,
    remaining_nodes: usize,
    truncated: bool,
}

impl TextWalk {
    fn new() -> Self {
        Self { text: String::new(), remaining_nodes: AX_NODE_BUDGET, truncated: false }
    }

    /// Visits `element` and, subject to `depth_remaining` and the node budget,
    /// its descendants, collecting `AXValue`/`AXTitle`/`AXDescription` strings.
    fn visit(&mut self, element: &AXUIElement, depth_remaining: u32) {
        if self.remaining_nodes == 0 {
            self.truncated = true;
            return;
        }
        self.remaining_nodes -= 1;

        self.collect_strings(element);

        let children = match element.attribute(&AXAttribute::children()) {
            Ok(children) => children,
            // Missing/unsupported children attribute is normal for leaf nodes.
            Err(_) => return,
        };

        if children.is_empty() {
            return;
        }
        if depth_remaining == 0 {
            // There is more tree below, but the depth bound stops us.
            self.truncated = true;
            return;
        }

        for child in children.iter() {
            if self.remaining_nodes == 0 {
                self.truncated = true;
                break;
            }
            self.visit(&child, depth_remaining - 1);
        }
    }

    /// Appends the string-valued text attributes of a single node.
    fn collect_strings(&mut self, element: &AXUIElement) {
        // AXValue is a generic CFType; only string values contribute text.
        if let Ok(value) = element.attribute(&AXAttribute::value()) {
            if let Some(text) = value.downcast::<CFString>() {
                self.push(&text.to_string());
            }
        }
        if let Ok(title) = element.attribute(&AXAttribute::title()) {
            self.push(&title.to_string());
        }
        if let Ok(description) = element.attribute(&AXAttribute::description()) {
            self.push(&description.to_string());
        }
    }

    /// Appends a non-empty fragment, newline-separated.
    fn push(&mut self, fragment: &str) {
        let fragment = fragment.trim();
        if fragment.is_empty() {
            return;
        }
        if !self.text.is_empty() {
            self.text.push('\n');
        }
        self.text.push_str(fragment);
    }
}
