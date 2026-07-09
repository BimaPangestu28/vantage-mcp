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

/// Resolve which act tools to mount. `VANTAGE_ACT_TOOLS` / `--act-tools=<csv>`
/// selects a subset (wins over the all-switch); `--allow-act` /
/// `VANTAGE_ALLOW_ACT` (truthy) selects all six; otherwise none. Unknown names
/// are warned and ignored. Resolved once at startup; never from agent input.
fn act_tools(
    args: impl Iterator<Item = String>,
    allow_env: Option<String>,
    tools_env: Option<String>,
) -> Vec<String> {
    let mut flag_all = false;
    let mut flag_csv: Option<String> = None;
    for a in args {
        if a == "--allow-act" {
            flag_all = true;
        } else if let Some(csv) = a.strip_prefix("--act-tools=") {
            flag_csv = Some(csv.to_string());
        }
    }
    let allow_env = allow_env
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);

    if let Some(csv) = flag_csv.or(tools_env).filter(|s| !s.trim().is_empty()) {
        return csv
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .filter(|s| {
                let ok = handler::ACT_TOOL_NAMES.contains(&s.as_str());
                if !ok {
                    tracing::warn!("ignoring unknown act tool in config: {s:?}");
                }
                ok
            })
            .collect();
    }
    if flag_all || allow_env {
        return handler::ACT_TOOL_NAMES
            .iter()
            .map(|s| s.to_string())
            .collect();
    }
    Vec::new()
}

#[tokio::main]
async fn main() -> Result<()> {
    logging::init();
    tracing::info!("vantage-mcp starting (stdio); logging on stderr only");

    let allowed_act = act_tools(
        std::env::args(),
        std::env::var("VANTAGE_ALLOW_ACT").ok(),
        std::env::var("VANTAGE_ACT_TOOLS").ok(),
    );
    if !allowed_act.is_empty() {
        tracing::warn!("act tools ENABLED: {}", allowed_act.join(", "));
    }

    let (windows, capturer, ocr, clipboard, input) = backend::backends();

    let service = Vantage::new(windows, capturer, ocr, clipboard, input, allowed_act)
        .serve(stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty() -> std::iter::Empty<String> {
        std::iter::empty()
    }

    #[test]
    fn none_by_default() {
        assert!(act_tools(empty(), None, None).is_empty());
        assert!(act_tools(empty(), Some("0".into()), None).is_empty());
        assert!(act_tools(empty(), Some("".into()), Some("".into())).is_empty());
    }

    #[test]
    fn all_via_flag_or_env() {
        assert_eq!(
            act_tools(["--allow-act".to_string()].into_iter(), None, None).len(),
            6
        );
        assert_eq!(act_tools(empty(), Some("1".into()), None).len(), 6);
        assert_eq!(act_tools(empty(), Some("TRUE".into()), None).len(), 6);
    }

    #[test]
    fn subset_via_env_or_flag_and_drops_unknown() {
        let v = act_tools(empty(), None, Some("write_clipboard, click, bogus".into()));
        assert_eq!(v, vec!["write_clipboard".to_string(), "click".to_string()]);
        // A CSV wins over the all-switch.
        let f = act_tools(
            ["--act-tools=key_press".to_string()].into_iter(),
            Some("1".into()),
            None,
        );
        assert_eq!(f, vec!["key_press".to_string()]);
    }
}
