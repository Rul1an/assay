//! One consent helper for CLI prompts (#2573).
//!
//! A prompt is allowed only when it can be shown. `preapproved` (`--yes` or
//! `--dry-run`) skips a confirm. Non-terminal stdin or stderr refuses early
//! with an attributable reason instead of reading dialoguer's `NotConnected`
//! as a silent decline. The OpenAI embedder secret prompt uses the same
//! check and names `OPENAI_API_KEY` instead of `--yes`.
//!
//! S2 adds the fail-closed posture: with `--non-interactive` /
//! `ASSAY_NON_INTERACTIVE` set (`main` records it via
//! [`set_non_interactive`]), the same shared check refuses before any TTY
//! inspection — even on a real terminal — so no prompt site can ask. Like
//! certbot's `--non-interactive`, it refuses rather than assuming defaults.

use std::fmt;
use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};

use dialoguer::{theme::ColorfulTheme, Confirm};

/// Process-wide fail-closed posture, set once in `main` from the resolved
/// `--non-interactive` / `ASSAY_NON_INTERACTIVE` value (top-level flag OR the
/// hidden `setup` alias OR the env). Every prompt site reads it through the
/// one shared check below, so the posture cannot reach one site and miss
/// another.
static NON_INTERACTIVE: AtomicBool = AtomicBool::new(false);

/// Record the resolved non-interactive posture for this process. Called once
/// in `main` before dispatch.
pub(crate) fn set_non_interactive(enabled: bool) {
    NON_INTERACTIVE.store(enabled, Ordering::SeqCst);
}

/// Whether this process runs under the fail-closed non-interactive posture.
pub(crate) fn non_interactive() -> bool {
    NON_INTERACTIVE.load(Ordering::SeqCst)
}

/// The reason an interactive confirm prompt could not be shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromptRefusalReason {
    NonInteractive,
    StdinNotTerminal,
    StderrNotTerminal,
    CouldNotRead,
}

impl PromptRefusalReason {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::NonInteractive => "--non-interactive is set",
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

/// Refuse when a prompt must not be shown. Callers read a line only after `Ok`.
///
/// The fail-closed posture is decided first: with `--non-interactive` /
/// `ASSAY_NON_INTERACTIVE` set, every prompt refuses before any TTY
/// inspection, so a real terminal does not re-enable asking. Otherwise stdin
/// is decided before stderr, so a pipe on both streams reports the stdin
/// reason. A confirm and the embedder secret prompt share this decision.
/// A library error on a non-terminal is not this decision.
pub(crate) fn refuse_if_prompt_not_showable(
    prompt: &str,
    remedy: &'static str,
) -> Result<(), PromptRefused> {
    let reason = if non_interactive() {
        PromptRefusalReason::NonInteractive
    } else if !std::io::stdin().is_terminal() {
        PromptRefusalReason::StdinNotTerminal
    } else if !std::io::stderr().is_terminal() {
        PromptRefusalReason::StderrNotTerminal
    } else {
        return Ok(());
    };
    Err(PromptRefused {
        prompt: prompt.to_string(),
        reason,
        remedy,
    })
}

pub(crate) fn confirm(prompt: &str, preapproved: bool) -> Result<bool, PromptRefused> {
    if preapproved {
        return Ok(true);
    }
    const REMEDY: &str = "pass --yes";
    refuse_if_prompt_not_showable(prompt, REMEDY)?;
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
    use std::sync::Mutex;

    /// The posture cell is process-wide: serialize the tests that flip it so
    /// parallel unit tests cannot observe each other's flag.
    static NON_INTERACTIVE_GUARD: Mutex<()> = Mutex::new(());

    /// S2 (#2573): with `--non-interactive` / `ASSAY_NON_INTERACTIVE` set,
    /// the shared check refuses before any TTY inspection — this holds on a
    /// real terminal too, where both `is_terminal()` calls would pass.
    #[test]
    fn non_interactive_flag_refuses_before_any_prompt() {
        let _guard = NON_INTERACTIVE_GUARD.lock().expect("guard");
        set_non_interactive(true);
        let refused = refuse_if_prompt_not_showable("Apply fix?", "pass --yes")
            .expect_err("the flag must refuse even where a prompt could show");
        assert_eq!(refused.reason, PromptRefusalReason::NonInteractive);
        let rendered = refused.to_string();
        assert!(
            rendered.contains("--non-interactive is set"),
            "the refusal must name the flag reason; got: {rendered}"
        );
        assert!(
            rendered.contains("pass --yes"),
            "the refusal must keep the remedy; got: {rendered}"
        );
        // `confirm` takes the same path: preapproval still proceeds (explicit
        // consent), everything else refuses with the named reason.
        assert!(confirm("Apply fix?", true).expect("preapproved stays approved"));
        let refused = confirm("Apply fix?", false)
            .expect_err("unapproved confirm must refuse under the flag");
        assert_eq!(refused.reason, PromptRefusalReason::NonInteractive);
        set_non_interactive(false);
    }

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

        let secret_stderr_err = PromptRefused {
            prompt: "OPENAI_API_KEY not set. Enter key:".to_string(),
            reason: PromptRefusalReason::StderrNotTerminal,
            remedy: "set OPENAI_API_KEY",
        };
        let rendered_secret_stderr = secret_stderr_err.to_string();
        assert!(rendered_secret_stderr.contains(
            "cannot show prompt \"OPENAI_API_KEY not set. Enter key:\": stderr is not a terminal; set OPENAI_API_KEY"
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
