//! One consent helper for the CLI confirm sites (#2573 slice 1).
//!
//! A prompt is allowed only when it can be shown. `preapproved` (`--yes` or
//! `--dry-run`) skips it. A non-terminal stdin refuses instead of reading
//! dialoguer's `NotConnected` as a silent decline.

use std::fmt;
use std::io::IsTerminal;

use dialoguer::{theme::ColorfulTheme, Confirm};

/// A confirm prompt that could not be shown.
///
/// The Display line names the invoked command, the prompt, and `--yes`. The
/// existing untyped `anyhow` funnel in `main` maps that to `EXIT_CONFIG_ERROR`.
#[derive(Debug)]
pub(crate) struct PromptRefused {
    prompt: String,
    hint: &'static str,
}

impl fmt::Display for PromptRefused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{command} cannot show prompt {prompt:?}; pass {hint}",
            command = invocation_command(),
            prompt = self.prompt,
            hint = self.hint
        )
    }
}

impl std::error::Error for PromptRefused {}

fn invocation_command() -> String {
    std::env::args()
        .nth(1)
        .unwrap_or_else(|| "assay".to_string())
}

pub(crate) fn confirm(prompt: &str, preapproved: bool) -> Result<bool, PromptRefused> {
    if preapproved {
        return Ok(true);
    }
    if !std::io::stdin().is_terminal() {
        return Err(PromptRefused {
            prompt: prompt.to_string(),
            hint: "--yes",
        });
    }
    Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .default(false)
        .interact()
        .map_err(|_| PromptRefused {
            prompt: prompt.to_string(),
            hint: "--yes",
        })
}
