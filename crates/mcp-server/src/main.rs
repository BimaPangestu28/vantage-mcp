mod error_map;
mod handler;
mod image_out;
mod logging;

use anyhow::Result;
use rmcp::{transport::stdio, ServiceExt};

use handler::Vantage;

#[cfg(target_os = "linux")]
use vantage_platform_linux as backend;
#[cfg(target_os = "macos")]
use vantage_platform_macos as backend;
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
compile_error!("vantage-mcp supports macOS and Linux only");

/// Act tools are enabled only if the `--allow-act` flag is present OR
/// `VANTAGE_ALLOW_ACT` is truthy (`1`/`true`/`yes`). Resolved once at startup;
/// never from per-call agent input.
fn act_enabled(mut args: impl Iterator<Item = String>, env: Option<String>) -> bool {
    let flag = args.any(|a| a == "--allow-act");
    let env = env
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);
    flag || env
}

#[tokio::main]
async fn main() -> Result<()> {
    logging::init();
    tracing::info!("vantage-mcp starting (stdio); logging on stderr only");

    let allow_act = act_enabled(std::env::args(), std::env::var("VANTAGE_ALLOW_ACT").ok());
    if allow_act {
        tracing::warn!(
            "act tools ENABLED (clipboard_write/type_text/click/focus_window are mounted)"
        );
    }

    let (windows, capturer, ocr, clipboard, input) = backend::backends();

    let service = Vantage::new(windows, capturer, ocr, clipboard, input, allow_act)
        .serve(stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_off_by_default_on_by_flag_or_env() {
        assert!(!act_enabled(std::iter::empty(), None));
        assert!(act_enabled(["--allow-act".to_string()].into_iter(), None));
        assert!(act_enabled(std::iter::empty(), Some("1".into())));
        assert!(act_enabled(std::iter::empty(), Some("TRUE".into())));
        assert!(act_enabled(std::iter::empty(), Some("yes".into())));
        assert!(!act_enabled(std::iter::empty(), Some("0".into())));
        assert!(!act_enabled(std::iter::empty(), Some("".into())));
    }
}
