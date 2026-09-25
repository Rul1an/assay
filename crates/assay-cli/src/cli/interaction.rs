//! One consent helper for CLI prompts (#2573).
//!
//! A prompt is allowed only when it can be shown. `preapproved` (`--yes` or
//! `--dry-run`) skips a confirm. Non-terminal stdin or stderr refuses early
//! with an attributable reason instead of reading dialoguer's `NotConnected`
//! as a silent decline. The OpenAI embedder secret prompt uses the same stdin
//! check and names `OPENAI_API_KEY` instead of `--yes`.

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

/// A prompt that could not be shown.
///
/// The Display line names the invoked command, the prompt, the reason it could
/// not be shown, and the remedy (`pass --yes` or `set OPENAI_API_KEY`). The
/// existing untyped `anyhow` funnel in `main`, and the run pipeline's config
/// fallback, map that to exit 2.
#[derive(Debug)]
pub(crate) struct PromptRefused {
    prompt: String,
    reason: PromptRefusalReason,
    /// Full clause after the semicolon, for example `pass --yes`.
    remedy: &'static str,
}

impl fmt::Display for PromptRefused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{command} cannot show prompt {prompt:?}: {reason}; {remedy}",
            command = invocation_command(),
            prompt = self.prompt,
            reason = self.reason.as_str(),
            remedy = self.remedy
        )
    }
}

impl std::error::Error for PromptRefused {}

fn invocation_command() -> String {
    std::env::args()
        .nth(1)
        .unwrap_or_else(|| "assay".to_string())
}

/// Refuse when stdin is not a terminal. Callers read a line only after `Ok`.
///
/// The check lives here so a confirm and the embedder secret prompt share one
/// decision. A library error on a non-terminal is not this decision.
pub(crate) fn refuse_if_stdin_not_terminal(
    prompt: &str,
    remedy: &'static str,
) -> Result<(), PromptRefused> {
    if std::io::stdin().is_terminal() {
        Ok(())
    } else {
        Err(PromptRefused {
            prompt: prompt.to_string(),
            reason: PromptRefusalReason::StdinNotTerminal,
            remedy,
        })
    }
}

pub(crate) fn confirm(prompt: &str, preapproved: bool) -> Result<bool, PromptRefused> {
    if preapproved {
        return Ok(true);
    }
    const REMEDY: &str = "pass --yes";
    refuse_if_stdin_not_terminal(prompt, REMEDY)?;
    if !std::io::stderr().is_terminal() {
        return Err(PromptRefused {
            prompt: prompt.to_string(),
            reason: PromptRefusalReason::StderrNotTerminal,
            remedy: REMEDY,
        });
    }
    Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .default(false)
        .interact()
        .map_err(|_| PromptRefused {
            prompt: prompt.to_string(),
            reason: PromptRefusalReason::CouldNotRead,
            remedy: REMEDY,
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
            remedy: "pass --yes",
        };
        let rendered = err.to_string();
        assert!(rendered
            .contains("cannot show prompt \"Apply fix?\": stderr is not a terminal; pass --yes"));

        let stdin_err = PromptRefused {
            prompt: "Apply fix?".to_string(),
            reason: PromptRefusalReason::StdinNotTerminal,
            remedy: "pass --yes",
        };
        let rendered_stdin = stdin_err.to_string();
        assert!(rendered_stdin
            .contains("cannot show prompt \"Apply fix?\": stdin is not a terminal; pass --yes"));

        let secret_err = PromptRefused {
            prompt: "OPENAI_API_KEY not set. Enter key:".to_string(),
            reason: PromptRefusalReason::StdinNotTerminal,
            remedy: "set OPENAI_API_KEY",
        };
        let rendered_secret = secret_err.to_string();
        assert!(rendered_secret.contains(
            "cannot show prompt \"OPENAI_API_KEY not set. Enter key:\": stdin is not a terminal; set OPENAI_API_KEY"
        ));

        let unreadable_err = PromptRefused {
            prompt: "Apply fix?".to_string(),
            reason: PromptRefusalReason::CouldNotRead,
            remedy: "pass --yes",
        };
        let rendered_unreadable = unreadable_err.to_string();
        assert!(rendered_unreadable.contains(
            "cannot show prompt \"Apply fix?\": the prompt could not be read; pass --yes"
        ));
    }
}
