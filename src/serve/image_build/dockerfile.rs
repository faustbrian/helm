use crate::serve::sql_client_flavor::SqlClientFlavor;

pub(super) fn render_derived_dockerfile(
    base_image: &str,
    extensions: &[String],
    include_js_tooling: bool,
    sql_client_flavor: SqlClientFlavor,
) -> String {
    let mut dockerfile = format!("FROM {base_image}\n");
    let sql_client_package = sql_client_flavor.apt_package();
    let sql_client_flavor = sql_client_flavor.as_str();

    if include_js_tooling {
        dockerfile.push_str(
            &format!(
                "RUN apt-get update \\\n    && apt-get install -y --no-install-recommends curl ca-certificates gnupg unzip {sql_client_package} postgresql-client \\\n    && curl -fsSL https://deb.nodesource.com/setup_lts.x | bash - \\\n    && apt-get install -y --no-install-recommends nodejs \\\n    && corepack enable \\\n    && curl -fsSL https://getcomposer.org/installer | php -- --install-dir=/usr/local/bin --filename=composer \\\n    && curl -fsSL https://bun.sh/install | bash \\\n    && ln -sf /root/.bun/bin/bun /usr/local/bin/bun \\\n    && rm -rf /var/lib/apt/lists/*\n"
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
