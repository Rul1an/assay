//! One consent helper for the CLI confirm sites (#2573).
//!
//! A prompt is allowed only when it can be shown. `preapproved` (`--yes` or
//! `--dry-run`) skips it. Non-terminal stdin or stderr refuses early with
//! an attributable reason instead of reading dialoguer's `NotConnected` as
//! a silent decline.

use std::fmt;
use std::io::IsTerminal;

use dialoguer::{theme::ColorfulTheme, Confirm};

/// The reason an interactive confirm prompt could not be shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromptRefusalReason {
    StdinNotTerminal,
    StderrNotTerminal,
    CouldNotRead,
}

impl PromptRefusalReason {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::StdinNotTerminal => "stdin is not a terminal",
            Self::StderrNotTerminal => "stderr is not a terminal",
            Self::CouldNotRead => "the prompt could not be read",
        }
    }
}

impl fmt::Display for PromptRefusalReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A confirm prompt that could not be shown.
///
/// The Display line names the invoked command, the prompt, the reason it could
/// not be shown, and `--yes`. The existing untyped `anyhow` funnel in `main` maps
/// that to `EXIT_CONFIG_ERROR`.
#[derive(Debug)]
pub(crate) struct PromptRefused {
    prompt: String,
    reason: PromptRefusalReason,
    hint: &'static str,
}

impl fmt::Display for PromptRefused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{command} cannot show prompt {prompt:?}: {reason}; pass {hint}",
            command = invocation_command(),
            prompt = self.prompt,
            reason = self.reason.as_str(),
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
            reason: PromptRefusalReason::StdinNotTerminal,
            hint: "--yes",
        });
    }
    if !std::io::stderr().is_terminal() {
        return Err(PromptRefused {
            prompt: prompt.to_string(),
            reason: PromptRefusalReason::StderrNotTerminal,
            hint: "--yes",
        });
    }
    Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .default(false)
        .interact()
        .map_err(|_| PromptRefused {
            prompt: prompt.to_string(),
            reason: PromptRefusalReason::CouldNotRead,
            hint: "--yes",
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_refused_display_includes_reason_and_hint() {
        let err = PromptRefused {
            prompt: "Apply fix?".to_string(),
            reason: PromptRefusalReason::StderrNotTerminal,
            hint: "--yes",
        };
        let rendered = err.to_string();
        assert!(rendered
            .contains("cannot show prompt \"Apply fix?\": stderr is not a terminal; pass --yes"));

        let stdin_err = PromptRefused {
            prompt: "Apply fix?".to_string(),
            reason: PromptRefusalReason::StdinNotTerminal,
            hint: "--yes",
        };
        let rendered_stdin = stdin_err.to_string();
        assert!(rendered_stdin
            .contains("cannot show prompt \"Apply fix?\": stdin is not a terminal; pass --yes"));

        let unreadable_err = PromptRefused {
            prompt: "Apply fix?".to_string(),
            reason: PromptRefusalReason::CouldNotRead,
            hint: "--yes",
        };
        let rendered_unreadable = unreadable_err.to_string();
        assert!(rendered_unreadable.contains(
            "cannot show prompt \"Apply fix?\": the prompt could not be read; pass --yes"
        ));
    }
}
