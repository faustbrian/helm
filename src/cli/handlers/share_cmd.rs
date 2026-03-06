//! cli handlers share cmd module.
//!
//! Contains share command handling for provider tunnels.

use anyhow::{Context, Result};

use crate::cli::handlers::serialize;
use crate::share::{self, ShareProvider};
use crate::{cli::args::ShareProviderArg, config};

#[derive(Clone, Copy)]
pub(crate) struct ShareProviderSelection {
    provider: Option<ShareProviderArg>,
    cloudflare: bool,
    expose: bool,
    tailscale: bool,
}

impl ShareProviderSelection {
    pub(crate) fn new(
        provider: Option<ShareProviderArg>,
        cloudflare: bool,
        expose: bool,
        tailscale: bool,
    ) -> Self {
        Self {
            provider,
            cloudflare,
            expose,
            tailscale,
        }
    }
}

impl From<crate::cli::args::ShareProviderSelectionArgs> for ShareProviderSelection {
    fn from(value: crate::cli::args::ShareProviderSelectionArgs) -> Self {
        Self::new(
            value.provider,
            value.cloudflare,
            value.expose,
            value.tailscale,
        )
    }
}

pub(crate) struct HandleShareStartOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) selection: ShareProviderSelection,
    pub(crate) detached: bool,
    pub(crate) timeout: u64,
    pub(crate) json: bool,
}

pub(crate) fn handle_share_start(
    config: &config::Config,
    options: HandleShareStartOptions<'_>,
) -> Result<()> {
    let provider =
        resolve_provider(options.selection, true)?.context("share start requires a provider")?;
    let result = share::start(
        config,
        options.service,
        provider,
        options.detached,
        options.timeout,
    )?;

    if options.json {
        serialize::print_json_pretty(&serde_json::json!({
            "session": result.session,
            "foreground_exit_code": result.foreground_exit_code,
        }))?;
    } else {
        print_session("started", &result.session);
        if let Some(code) = result.foreground_exit_code {
            println!("foreground process exited with code {code}");
        }
    }

    Ok(())
}

pub(crate) fn handle_share_status(
    service: Option<&str>,
    selection: ShareProviderSelection,
    json: bool,
) -> Result<()> {
    let provider = resolve_provider(selection, false)?;
    let sessions = share::status(service, provider)?;
    print_sessions(&sessions, "session", "No share sessions found.", json)
}

pub(crate) struct HandleShareStopOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) selection: ShareProviderSelection,
    pub(crate) all: bool,
    pub(crate) json: bool,
}

pub(crate) fn handle_share_stop(options: HandleShareStopOptions<'_>) -> Result<()> {
    let provider = resolve_provider(options.selection, false)?;
    let stopped = share::stop(options.service, provider, options.all)?;
    print_sessions(
        &stopped,
        "stopped",
        "No matching share sessions to stop.",
        options.json,
    )
}

fn print_sessions(
    sessions: &[share::ShareSessionStatus],
    label: &str,
    empty_message: &str,
    json: bool,
) -> Result<()> {
    if json {
        serialize::print_json_pretty(sessions)?;
    } else if sessions.is_empty() {
        println!("{empty_message}");
    } else {
        for session in sessions {
            print_session(label, session);
        }
    }
    Ok(())
}

fn print_session(label: &str, session: &share::ShareSessionStatus) {
    println!("{label}: {} ({})", session.service, session.provider);
    println!("  id: {}", session.id);
    println!("  local: {}", session.local_url);
    if let Some(public_url) = &session.public_url {
        println!("  public: {public_url}");
    }
    println!("  running: {}", session.running);
    if let Some(pid) = session.pid {
        println!("  pid: {pid}");
    }
    println!("  log: {}", session.log_path);
}

fn to_provider(provider: ShareProviderArg) -> ShareProvider {
    match provider {
        ShareProviderArg::Cloudflare => ShareProvider::Cloudflare,
        ShareProviderArg::Expose => ShareProvider::Expose,
        ShareProviderArg::Tailscale => ShareProvider::Tailscale,
    }
}

fn resolve_provider(
    selection: ShareProviderSelection,
    required: bool,
) -> Result<Option<ShareProvider>> {
    let resolved = if selection.cloudflare {
        Some(ShareProvider::Cloudflare)
    } else if selection.expose {
        Some(ShareProvider::Expose)
    } else if selection.tailscale {
        Some(ShareProvider::Tailscale)
    } else {
        selection.provider.map(to_provider)
    };

    if required && resolved.is_none() {
        anyhow::bail!(
            "provider is required: pass --provider <cloudflare|expose|tailscale>, \
            --cloudflare, --expose, or --tailscale"
        );
    }

    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use crate::cli::args::{ShareProviderArg, ShareProviderSelectionArgs};

    use super::resolve_provider;

    #[test]
    fn resolve_provider_respects_cloudflare_flag() {
        let selection = ShareProviderSelectionArgs {
            provider: Some(ShareProviderArg::Expose),
            cloudflare: true,
            expose: false,
            tailscale: false,
        };
        let provider = resolve_provider(selection.into(), false).expect("cloudflare preferred");
        assert_eq!(
            provider.map(|value| value.to_string()),
            Some("cloudflare".to_string())
        );
    }

    #[test]
    fn resolve_provider_requires_provider_when_required() {
        let selection = ShareProviderSelectionArgs {
            provider: None,
            cloudflare: false,
            expose: false,
            tailscale: false,
        };
        assert!(resolve_provider(selection.into(), true).is_err());
    }

    #[test]
    fn resolve_provider_defaults_to_none_when_optional() {
        let selection = ShareProviderSelectionArgs {
            provider: None,
            cloudflare: false,
            expose: false,
            tailscale: false,
        };
        assert_eq!(
            resolve_provider(selection.into(), false).expect("optional"),
            None
        );
    }
}
