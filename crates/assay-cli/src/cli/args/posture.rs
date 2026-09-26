//! S1/S2 caller posture (#2573): top-level `--quiet`, `--color`, and
//! `--non-interactive`.
//!
//! `--quiet` suppresses progress and banner lines of `run`/`ci`/`watch`/
//! `replay` only; it never touches warnings, reason codes, fatal
//! diagnostics, or machine output on stdout. `--non-interactive` is
//! fail-closed: any code path that would prompt refuses with exit 2 and a
//! named reason instead of assuming defaults. Both are plain top-level flags,
//! deliberately NOT clap-global, and `ASSAY_QUIET` / `ASSAY_NON_INTERACTIVE`
//! are read here rather than via clap `env =`: neither path can reach
//! another command's local flag (`sandbox`, `monitor`, `mcp tool verify`
//! keep their own `quiet`; `setup` keeps its hidden `--non-interactive`
//! alias, which ORs into the same posture).
//!
//! Place `--quiet` / `--non-interactive` (and their `ASSAY_*` envs) before
//! the subcommand: `assay --non-interactive doctor --fix ...`. After a
//! subcommand each is a clap usage error, except where a command defines its
//! own local spelling.
//!
//! An empty `ASSAY_QUIET`, `ASSAY_COLOR`, or `ASSAY_NON_INTERACTIVE` counts
//! as unset (templated CI environments export empty variables).

use clap::ValueEnum;
use std::ffi::{OsStr, OsString};
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

/// The colour rule for the operator-diagnostic sites it governs
/// (`run`/`ci`/`watch`/`replay` failures via `emit_operator_diagnostic`):
/// the explicit `--color` flag beats `NO_COLOR`, which beats TTY detection.
///
/// `no_color` is presence, not content: a present-but-empty `NO_COLOR` still
/// disables under `auto`, exactly as the previous inline
/// `var_os("NO_COLOR").is_none()` check behaved (coordinator decision: no
/// empty-string rule change).
///
/// Not yet governed (pre-existing, out of S1 scope): `validate` and `demo`
/// render `format_terminal()` unconditionally, and the assay-core legacy
/// policy path emits raw ANSI. See docs/reference/cli.
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

/// `NO_COLOR` wiring seam (F5): the lookup is injected so tests pin the
/// exact variable read and the presence-not-content rule without touching
/// the process environment. `color_enabled` below is the single live caller.
pub(crate) fn color_enabled_from(
    choice: ColorChoice,
    stderr_tty: bool,
    lookup: &dyn Fn(&str) -> Option<OsString>,
) -> bool {
    resolve_color(choice, stderr_tty, lookup("NO_COLOR").as_deref())
}

/// Answer the colour rule from the live process state.
pub(crate) fn color_enabled(choice: ColorChoice) -> bool {
    color_enabled_from(choice, std::io::stderr().is_terminal(), &|key| {
        std::env::var_os(key)
    })
}

/// clap `BoolishValueParser` literals (clap_builder `util::str_to_bool`):
/// `y`/`yes`/`t`/`true`/`on`/`1` enable, `n`/`no`/`f`/`false`/`off`/`0`
/// disable, case-insensitive. Anything else is rejected, exactly as the old
/// clap `env = "ASSAY_QUIET"` binding rejected it with exit 2.
fn parse_boolish(raw: &str) -> Option<bool> {
    match raw.to_lowercase().as_str() {
        "y" | "yes" | "t" | "true" | "on" | "1" => Some(true),
        "n" | "no" | "f" | "false" | "off" | "0" => Some(false),
        _ => None,
    }
}

/// Flag and env names of the caller postures, stated once so `describe`
/// cannot drift from the parser (pinned by
/// `posture_names_match_the_clap_definition`).
pub(crate) const QUIET_FLAG: &str = "--quiet";
pub(crate) const QUIET_SHORT: &str = "-q";
pub(crate) const QUIET_ENV: &str = "ASSAY_QUIET";
pub(crate) const COLOR_FLAG: &str = "--color";
pub(crate) const COLOR_ENV: &str = "ASSAY_COLOR";
pub(crate) const NON_INTERACTIVE_FLAG: &str = "--non-interactive";
pub(crate) const NON_INTERACTIVE_ENV: &str = "ASSAY_NON_INTERACTIVE";

/// `ASSAY_QUIET`: unset or empty counts as unset (off); otherwise boolish.
pub(crate) fn parse_quiet_env(raw: Option<&OsStr>) -> Result<bool, String> {
    let Some(raw) = raw else {
        return Ok(false);
    };
    if raw.is_empty() {
        return Ok(false);
    }
    let text = raw.to_str().ok_or_else(|| {
        format!(
            "invalid ASSAY_QUIET value {}: expected a boolean (1/0, true/false, yes/no, on/off)",
            raw.to_string_lossy()
        )
    })?;
    parse_boolish(text).ok_or_else(|| {
        format!(
            "invalid ASSAY_QUIET value {text:?}: \
             expected a boolean (1/0, true/false, yes/no, on/off)"
        )
    })
}

/// `ASSAY_COLOR`: unset or empty counts as unset (`None`, so the default
/// `auto` applies); otherwise `auto`|`always`|`never`.
pub(crate) fn parse_color_env(raw: Option<&OsStr>) -> Result<Option<ColorChoice>, String> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    if raw.is_empty() {
        return Ok(None);
    }
    let text = raw.to_str().ok_or_else(|| {
        format!(
            "invalid ASSAY_COLOR value {}: expected auto|always|never",
            raw.to_string_lossy()
        )
    })?;
    match text {
        "auto" => Ok(Some(ColorChoice::Auto)),
        "always" => Ok(Some(ColorChoice::Always)),
        "never" => Ok(Some(ColorChoice::Never)),
        _ => Err(format!(
            "invalid ASSAY_COLOR value {text:?}: expected auto|always|never"
        )),
    }
}

/// `ASSAY_NON_INTERACTIVE`: unset or empty counts as unset (off); otherwise
/// the same boolish vocabulary as `ASSAY_QUIET`. Anything else is a usage
/// error, exactly as the old clap `env =` binding rejected it with exit 2.
pub(crate) fn parse_non_interactive_env(raw: Option<&OsStr>) -> Result<bool, String> {
    let Some(raw) = raw else {
        return Ok(false);
    };
    if raw.is_empty() {
        return Ok(false);
    }
    let text = raw.to_str().ok_or_else(|| {
        format!(
            "invalid {NON_INTERACTIVE_ENV} value {}: expected a boolean (1/0, true/false, yes/no, on/off)",
            raw.to_string_lossy()
        )
    })?;
    parse_boolish(text).ok_or_else(|| {
        format!(
            "invalid {NON_INTERACTIVE_ENV} value {text:?}: \
             expected a boolean (1/0, true/false, yes/no, on/off)"
        )
    })
}

/// Resolve the caller posture from already-parsed top-level flags plus raw
/// env values. The explicit `--color` flag beats `ASSAY_COLOR`, which beats
/// the `auto` default; `--quiet` is the flag OR the env, and
/// `--non-interactive` is the flag OR the env.
pub(crate) fn resolve_posture_with(
    top_quiet: bool,
    top_color: Option<ColorChoice>,
    top_non_interactive: bool,
    quiet_env: Option<&OsStr>,
    color_env: Option<&OsStr>,
    non_interactive_env: Option<&OsStr>,
) -> Result<(bool, ColorChoice, bool), String> {
    let quiet = top_quiet || parse_quiet_env(quiet_env)?;
    let color = top_color
        .or(parse_color_env(color_env)?)
        .unwrap_or_default();
    let non_interactive = top_non_interactive || parse_non_interactive_env(non_interactive_env)?;
    Ok((quiet, color, non_interactive))
}
