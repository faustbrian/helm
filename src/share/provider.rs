//! Share provider definitions and command builders.

use std::fmt::{Display, Formatter};

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShareProvider {
    Cloudflare,
    Expose,
    Tailscale,
}

impl ShareProvider {
    pub fn command_binary(self) -> &'static str {
        match self {
            Self::Cloudflare => "cloudflared",
            Self::Expose => "expose",
            Self::Tailscale => "tailscale",
        }
    }

    pub fn command_args(self, local_url: &str) -> Vec<String> {
        match self {
            Self::Cloudflare => cloudflare_args(local_url),
            Self::Expose => expose_args(local_url),
            Self::Tailscale => tailscale_args(local_url),
        }
    }

    pub fn public_url_matches(self, url: &str) -> bool {
        match self {
            Self::Cloudflare => url.ends_with(".trycloudflare.com"),
            Self::Expose => url.contains(".sharedwithexpose.com") || url.contains(".expose.dev"),
            Self::Tailscale => url.contains(".ts.net"),
        }
    }
}

impl Display for ShareProvider {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cloudflare => write!(f, "cloudflare"),
            Self::Expose => write!(f, "expose"),
            Self::Tailscale => write!(f, "tailscale"),
        }
    }
}

fn cloudflare_args(local_url: &str) -> Vec<String> {
    let mut args = vec![
        "tunnel".to_owned(),
        "--url".to_owned(),
        local_url.to_owned(),
    ];
    if local_url.starts_with("https://") {
        args.push("--no-tls-verify".to_owned());
    }
    args
}

fn tailscale_args(local_url: &str) -> Vec<String> {
    let port = local_url
        .rsplit_once(':')
        .and_then(|(_, right)| right.split('/').next())
        .unwrap_or(local_url);
    vec!["funnel".to_owned(), "--bg".to_owned(), port.to_owned()]
}

fn expose_args(local_url: &str) -> Vec<String> {
    vec!["share".to_owned(), local_url.to_owned()]
}
