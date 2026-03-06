//! Dockerfile generator for serve derived images.

use crate::node::{JavaScriptRuntime, VersionManager};
use crate::serve::sql_client_flavor::SqlClientFlavor;

/// Renders a complete Dockerfile for a derived serve image.
///
/// The generated image can add:
/// - missing PHP extensions
/// - optional JS/package-manager tooling
/// - MySQL client wrapper compatibility scripts
pub(super) fn render_derived_dockerfile(
    base_image: &str,
    extensions: &[String],
    include_js_tooling: bool,
    runtime: JavaScriptRuntime,
    version_manager: VersionManager,
    node_version: Option<&str>,
    sql_client_flavor: SqlClientFlavor,
) -> String {
    let mut dockerfile = format!("FROM {base_image}\n");
    let sql_client_package = sql_client_flavor.apt_package();
    let sql_client_flavor = sql_client_flavor.as_str();

    if include_js_tooling {
        dockerfile.push_str(
            &format!(
                "RUN apt-get update \\\n    && apt-get install -y --no-install-recommends bash curl ca-certificates gnupg unzip ghostscript {sql_client_package} postgresql-client \\\n    && {} \\\n    && curl -fsSL https://getcomposer.org/installer | php -- --install-dir=/usr/local/bin --filename=composer \\\n    && rm -rf /var/lib/apt/lists/*\n",
                render_js_tooling_install(runtime, version_manager, node_version),
            ),
        );
    }

    dockerfile.push_str(
        &format!(
            "RUN cat <<'EOF' >/usr/local/bin/mysqldump\n#!/usr/bin/env bash\nset -euo pipefail\n\nHELM_SQL_CLIENT_FLAVOR={sql_client_flavor}\nreal_mysqldump=\"/usr/bin/mysqldump\"\nif [[ ! -x \"$real_mysqldump\" ]]; then\n  real_mysqldump=\"$(command -v mariadb-dump || command -v mysqldump)\"\nfi\n\nsanitized=()\nresult_file=\"\"\nfor arg in \"$@\"; do\n  case \"$arg\" in\n    --column-statistics=0|--set-gtid-purged=OFF)\n      continue\n      ;;\n    --result-file=*)\n      result_file=\"${{arg#--result-file=}}\"\n      ;;\n  esac\n  sanitized+=(\"$arg\")\ndone\n\nstderr_file=\"$(mktemp)\"\nif \"$real_mysqldump\" \"${{sanitized[@]}}\" 2>\"$stderr_file\"; then\n  if [[ \"$HELM_SQL_CLIENT_FLAVOR\" == \"mysql\" && -n \"$result_file\" && -f \"$result_file\" ]]; then\n    sed -i '/^\\/\\*M\\!.*\\*\\/;*$/d' \"$result_file\"\n  fi\n  rm -f \"$stderr_file\"\n  exit 0\nfi\nstatus=$?\n\nif grep -qi \"self-signed certificate in certificate chain\" \"$stderr_file\"; then\n  if \"$real_mysqldump\" --help 2>/dev/null | grep -q -- \"--ssl-mode\"; then\n    \"$real_mysqldump\" --ssl-mode=DISABLED \"${{sanitized[@]}}\"\n  else\n    \"$real_mysqldump\" --skip-ssl \"${{sanitized[@]}}\"\n  fi\n  retry_status=$?\n  if [[ \"$HELM_SQL_CLIENT_FLAVOR\" == \"mysql\" && -n \"$result_file\" && -f \"$result_file\" ]]; then\n    sed -i '/^\\/\\*M\\!.*\\*\\/;*$/d' \"$result_file\"\n  fi\n  rm -f \"$stderr_file\"\n  exit \"$retry_status\"\nfi\n\ncat \"$stderr_file\" >&2\nrm -f \"$stderr_file\"\nexit \"$status\"\nEOF\nRUN chmod +x /usr/local/bin/mysqldump\n"
        ),
    );
    dockerfile.push_str(
        "RUN cat <<'EOF' >/usr/local/bin/mysql\n#!/usr/bin/env bash\nset -euo pipefail\n\nreal_mysql=\"/usr/bin/mysql\"\nif [[ ! -x \"$real_mysql\" ]]; then\n  real_mysql=\"$(command -v mariadb || command -v mysql)\"\nfi\n\nstderr_file=\"$(mktemp)\"\nif \"$real_mysql\" \"$@\" 2>\"$stderr_file\"; then\n  rm -f \"$stderr_file\"\n  exit 0\nfi\nstatus=$?\n\nif grep -qi \"self-signed certificate in certificate chain\" \"$stderr_file\"; then\n  if \"$real_mysql\" --help 2>/dev/null | grep -q -- \"--ssl-mode\"; then\n    \"$real_mysql\" --ssl-mode=DISABLED \"$@\"\n  else\n    \"$real_mysql\" --skip-ssl \"$@\"\n  fi\n  retry_status=$?\n  rm -f \"$stderr_file\"\n  exit \"$retry_status\"\nfi\n\ncat \"$stderr_file\" >&2\nrm -f \"$stderr_file\"\nexit \"$status\"\nEOF\nRUN chmod +x /usr/local/bin/mysql\n",
    );

    if !extensions.is_empty() {
        dockerfile.push_str(&format!(
            "RUN install-php-extensions {}\n",
            extensions.join(" ")
        ));
    }

    if include_js_tooling {
        // Keep a high default for all PHP processes inside the app container,
        // including child test runners spawned by `artisan test`.
        dockerfile.push_str(
            "RUN echo 'memory_limit=2048M' > /usr/local/etc/php/conf.d/zz-helm-memory-limit.ini\n",
        );
    }

    dockerfile
}

fn render_js_tooling_install(
    runtime: JavaScriptRuntime,
    version_manager: VersionManager,
    node_version: Option<&str>,
) -> String {
    match runtime {
        JavaScriptRuntime::Node => render_node_tooling_install(version_manager, node_version),
        JavaScriptRuntime::Bun => render_bun_install(node_version),
        JavaScriptRuntime::Deno => render_deno_install(node_version),
    }
}

fn render_node_tooling_install(
    version_manager: VersionManager,
    node_version: Option<&str>,
) -> String {
    match version_manager {
        VersionManager::System => render_system_node_install(node_version),
        VersionManager::Fnm => render_fnm_install(node_version),
        VersionManager::Nvm => render_nvm_install(node_version),
        VersionManager::Volta => render_volta_install(node_version),
    }
}

fn render_deno_install(version: Option<&str>) -> String {
    let version_arg = version
        .map(|value| format!(" --version {value}"))
        .unwrap_or_default();

    format!(
        "curl -fsSL https://deno.land/install.sh | sh -s --{version_arg} \\\n    && ln -sf /root/.deno/bin/deno /usr/local/bin/deno"
    )
}

fn render_bun_install(version: Option<&str>) -> String {
    let version_arg = version
        .map(|value| format!("bun-v{value}"))
        .unwrap_or_else(|| "latest".to_owned());

    format!(
        "curl -fsSL https://bun.sh/install | bash -s -- {version_arg} \\\n    && ln -sf /root/.bun/bin/bun /usr/local/bin/bun"
    )
}

fn render_system_node_install(node_version: Option<&str>) -> String {
    let channel = node_version
        .and_then(extract_node_major)
        .map(|major| format!("setup_{major}.x"))
        .unwrap_or_else(|| "setup_lts.x".to_owned());

    format!(
        "curl -fsSL https://deb.nodesource.com/{channel} | bash - \\\n    && apt-get install -y --no-install-recommends nodejs \\\n    && npm install -g pnpm yarn"
    )
}

fn render_fnm_install(node_version: Option<&str>) -> String {
    let mut install = "curl -fsSL https://fnm.vercel.app/install | bash -s -- --install-dir /usr/local/fnm --skip-shell \\\n    && ln -sf /usr/local/fnm/fnm /usr/local/bin/fnm".to_owned();
    if let Some(version) = node_version {
        install.push_str(&format!(
            " \\\n    && eval \"$(fnm env --shell bash)\" \\\n    && fnm install {version} \\\n    && fnm default {version} \\\n    && fnm exec --using {version} npm install -g pnpm yarn"
        ));
    }
    install
}

fn render_nvm_install(node_version: Option<&str>) -> String {
    let mut install = "export NVM_DIR=/usr/local/nvm \\\n    && curl -fsSL https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh | bash".to_owned();
    if let Some(version) = node_version {
        install.push_str(&format!(
            " \\\n    && export NVM_DIR=/usr/local/nvm \\\n    && . \"$NVM_DIR/nvm.sh\" \\\n    && nvm install {version} \\\n    && nvm alias default {version} \\\n    && nvm exec {version} npm install -g pnpm yarn"
        ));
    }
    install
}

fn render_volta_install(node_version: Option<&str>) -> String {
    let mut install = "export VOLTA_HOME=/usr/local/volta \\\n    && curl -fsSL https://get.volta.sh | bash -s -- --skip-setup \\\n    && ln -sf /usr/local/volta/bin/volta /usr/local/bin/volta".to_owned();
    if let Some(version) = node_version {
        install.push_str(&format!(
            " \\\n    && export VOLTA_HOME=/usr/local/volta \\\n    && export PATH=\"$VOLTA_HOME/bin:$PATH\" \\\n    && volta install node@{version} \\\n    && volta run --node {version} npm install -g pnpm yarn"
        ));
    }
    install
}

fn extract_node_major(version: &str) -> Option<&str> {
    let trimmed = version.trim();
    if trimmed.is_empty() {
        return None;
    }

    let prefix_len = trimmed
        .chars()
        .take_while(|ch| ch.is_ascii_digit() || *ch == '.')
        .count();
    let numeric = trimmed.get(..prefix_len)?;
    let major = numeric.split('.').next()?;
    (!major.is_empty()).then_some(major)
}
