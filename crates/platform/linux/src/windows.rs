//! Linux `WindowInspector` backed by AT-SPI2 over D-Bus.
//!
//! The `WindowInspector` trait is synchronous and is called from the handler's
//! `spawn_blocking` pool. AT-SPI (`atspi`/`zbus`, tokio reactor) is async, so
//! this backend owns a private current-thread Tokio runtime and drives async
//! calls with `block_on`. That is safe here because the trait methods run on a
//! `spawn_blocking` thread, never on a runtime worker thread.

use std::sync::Mutex;

use atspi::connection::AccessibilityConnection;
use atspi::proxy::accessible::{AccessibleProxy, ObjectRefExt};
use atspi::proxy::proxy_ext::ProxyExt;
use atspi::zbus;
use atspi::{CoordType, Role, State};
use vantage_core::{
    Bounds, CaptureError, WindowFilter, WindowId, WindowInfo, WindowInspector, WindowText,
};

use crate::atspi_conn::window_id_hash;

/// The AT-SPI registry's desktop root accessible.
const REGISTRY_DEST: &str = "org.a11y.atspi.Registry";
const ROOT_PATH: &str = "/org/a11y/atspi/accessible/root";

const ZERO_BOUNDS: Bounds = Bounds {
    x: 0,
    y: 0,
    width: 0,
    height: 0,
};

pub struct LinuxWindowInspector {
    // Private current-thread runtime; a Mutex lets the `&self` trait methods
    // borrow it to `block_on`.
    rt: Mutex<tokio::runtime::Runtime>,
}

impl LinuxWindowInspector {
    pub fn new() -> Self {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build current-thread runtime for AT-SPI");
        Self { rt: Mutex::new(rt) }
    }
}

impl Default for LinuxWindowInspector {
    fn default() -> Self {
        Self::new()
    }
}

/// A top-level window discovered in the AT-SPI tree, with the identity needed
/// to resolve it again (bus name + object path) and its on-screen state.
pub(crate) struct Frame {
    pub info: WindowInfo,
    pub bus: String,
    pub path: String,
    pub showing: bool,
}

/// Connect to the accessibility bus. A failure here means AT-SPI is unavailable
/// (bus not running / accessibility disabled) -> the actionable permission error.
pub(crate) async fn connect() -> Result<AccessibilityConnection, CaptureError> {
    AccessibilityConnection::new()
        .await
        .map_err(|_| CaptureError::AccessibilityPermissionDenied)
}

fn map_internal<E: std::fmt::Display>(e: E) -> CaptureError {
    CaptureError::Internal(format!("atspi: {e}"))
}

/// Enumerate every top-level application frame in the AT-SPI desktop tree.
/// Shared by `list_windows` (which then filters) and `read_window_text` (which
/// re-resolves a `window_id` by re-hashing each frame's identity).
pub(crate) async fn enumerate_frames(
    conn: &zbus::Connection,
) -> Result<Vec<Frame>, CaptureError> {
    let root = AccessibleProxy::builder(conn)
        .destination(REGISTRY_DEST)
        .map_err(map_internal)?
        .path(ROOT_PATH)
        .map_err(map_internal)?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .map_err(map_internal)?;

    let apps = root.get_children().await.map_err(map_internal)?;
    let mut out = Vec::new();

    for app_ref in apps {
        let Ok(app_proxy) = app_ref.as_accessible_proxy(conn).await else {
            continue;
        };
        let app_name = app_proxy.name().await.unwrap_or_default();
        let Ok(frames) = app_proxy.get_children().await else {
            continue;
        };

        for frame_ref in frames {
            let Ok(fp) = frame_ref.as_accessible_proxy(conn).await else {
                continue;
            };
            let Ok(role) = fp.get_role().await else {
                continue;
            };
            if !is_window_role(role) {
                continue;
            }

            let state = fp.get_state().await.ok();
            let showing = state.map(|s| s.contains(State::Showing)).unwrap_or(false);
            let focused = state.map(|s| s.contains(State::Active)).unwrap_or(false);
            let title = fp.name().await.unwrap_or_default();
            let bounds = frame_extents(&fp).await;

            let bus = frame_ref.name.to_string();
            let path = frame_ref.path.to_string();
            let window_id = window_id_hash(&bus, &path);

            out.push(Frame {
                info: WindowInfo {
                    window_id,
                    app: app_name.clone(),
                    title,
                    bounds,
                    focused,
                },
                bus,
                path,
                showing,
            });
        }
    }
    Ok(out)
}

/// Read a frame's screen-space extents via the Component interface, defaulting
/// to zero bounds when the frame does not implement Component or the call fails.
async fn frame_extents(frame: &AccessibleProxy<'_>) -> Bounds {
    let Ok(proxies) = frame.proxies().await else {
        return ZERO_BOUNDS;
    };
    let Ok(component) = proxies.component().await else {
        return ZERO_BOUNDS;
    };
    match component.get_extents(CoordType::Screen).await {
        Ok((x, y, w, h)) => Bounds {
            x,
            y,
            width: w.max(0) as u32,
            height: h.max(0) as u32,
        },
        Err(_) => ZERO_BOUNDS,
    }
}

fn is_window_role(role: Role) -> bool {
    matches!(
        role,
        Role::Frame | Role::Window | Role::Dialog | Role::Alert | Role::FileChooser
    )
}

impl WindowInspector for LinuxWindowInspector {
    fn list_windows(&self, filter: WindowFilter) -> Result<Vec<WindowInfo>, CaptureError> {
        let rt = self.rt.lock().expect("runtime mutex");
        rt.block_on(async {
            let conn = connect().await?;
            let frames = enumerate_frames(conn.connection()).await?;
            let mut out: Vec<WindowInfo> = frames
                .into_iter()
                .filter(|f| !filter.on_screen_only || f.showing)
                .map(|f| f.info)
                .collect();
            if let Some(app) = filter.app_filter {
                out.retain(|w| w.app == app);
            }
            Ok(out)
        })
    }

    fn read_window_text(
        &self,
        _window_id: WindowId,
        _depth: u32,
    ) -> Result<WindowText, CaptureError> {
        // Implemented in Task 6.
        Err(CaptureError::Unsupported(
            "linux window text not yet implemented".into(),
        ))
    }
}
