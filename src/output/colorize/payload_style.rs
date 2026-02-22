//! Payload token styling helpers for output colorization.

use colored::Colorize;

use super::payload_scan::SummaryTone;

pub(super) fn style_url(token: &str) -> String {
    token.truecolor(80, 220, 255).underline().to_string()
}

pub(super) fn style_path(token: &str) -> String {
    token.truecolor(120, 170, 255).to_string()
}

pub(super) fn style_quoted(token: &str) -> String {
    token.truecolor(255, 200, 87).to_string()
}

pub(super) fn style_summary_count(token: &str, tone: SummaryTone) -> String {
    match tone {
        SummaryTone::Success => token.green().to_string(),
        SummaryTone::Failure => token.red().to_string(),
        SummaryTone::Skipped => token.yellow().to_string(),
    }
}

pub(super) fn style_number(token: &str) -> String {
    token.truecolor(255, 120, 220).to_string()
}

pub(super) fn style_image_like(token: &str) -> String {
    token.truecolor(255, 200, 87).to_string()
}
