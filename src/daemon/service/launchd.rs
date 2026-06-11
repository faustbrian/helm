//! launchd definition rendering for daemon watch services.

use anyhow::Result;
use std::path::Path;

use super::{DaemonServiceDefinition, ServiceContext, ServiceManager};

pub(super) fn definition(
    context: &ServiceContext,
    label: &str,
    path: &Path,
) -> Result<DaemonServiceDefinition> {
    Ok(DaemonServiceDefinition {
        manager: ServiceManager::Launchd,
        label: label.to_owned(),
        path: path.to_path_buf(),
        contents: render_plist(context, label),
    })
}

fn render_plist(context: &ServiceContext, label: &str) -> String {
    let program_arguments = std::iter::once(&context.binary)
        .chain(context.args.iter())
        .map(|value| format!("    <string>{}</string>", xml_escape(value)))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{label}</string>
  <key>ProgramArguments</key>
  <array>
{program_arguments}
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>StandardOutPath</key>
  <string>{stdout_path}</string>
  <key>StandardErrorPath</key>
  <string>{stderr_path}</string>
</dict>
</plist>
"#,
        label = xml_escape(label),
        program_arguments = program_arguments,
        stdout_path = xml_escape(&context.stdout_path.to_string_lossy()),
        stderr_path = xml_escape(&context.stderr_path.to_string_lossy()),
    )
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
