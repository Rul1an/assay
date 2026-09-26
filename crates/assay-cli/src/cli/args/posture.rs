//! S1 caller posture (#2573): global `--quiet` and `--color`.
//!
//! `--quiet` suppresses progress and banner lines only; it never touches
//! warnings, reason codes, fatal diagnostics, or machine output on stdout.
//! Per-command `--quiet` flags keep their own meaning, except that clap
//! merges a same-spelling global and local flag into one occurrence (a second
//! id sharing the `--quiet` long is a build error), so for `mcp tool verify`
//! either position triggers the grandfathered error-text suppression. The
//! exit code is unchanged, and the global flag never re-enables output a
//! local flag suppressed.

use clap::ValueEnum;
use std::ffi::OsStr;
use std::io::IsTerminal;

/// Global `--color` vocabulary.
///
/// The variants carry no per-value help on purpose: documented values would
/// flip clap into its multi-line help layout for every subcommand, which the
/// help-parsing contract tests (`format_value_parser.rs`) do not accept.
/// The meanings live in `docs/reference/cli` instead.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum ColorChoice {
    #[default]
    Auto,
    Always,
    Never,
}

/// The single colour rule: the explicit `--color` flag beats `NO_COLOR`,
/// which beats TTY detection.
///
/// `no_color` is presence, not content: a present-but-empty `NO_COLOR` still
/// disables under `auto`, exactly as the previous inline
/// `var_os("NO_COLOR").is_none()` check behaved (coordinator decision: no
/// empty-string rule change).
pub(crate) fn resolve_color(
    choice: ColorChoice,
    stderr_tty: bool,
    no_color: Option<&OsStr>,
) -> bool {
    match choice {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => stderr_tty && no_color.is_none(),
    }
}

/// Answer the single colour rule from the live process state.
pub(crate) fn color_enabled(choice: ColorChoice) -> bool {
    resolve_color(
        choice,
        std::io::stderr().is_terminal(),
        std::env::var_os("NO_COLOR").as_deref(),
    )
}
