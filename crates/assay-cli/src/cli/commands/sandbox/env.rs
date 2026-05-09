use crate::cli::args::SandboxArgs;
use crate::env_filter::EnvFilter;

/// Build the environment filter based on CLI args.
pub(super) fn build_env_filter(args: &SandboxArgs) -> EnvFilter {
    if args.env_passthrough {
        return EnvFilter::passthrough();
    }

    let mut filter = if args.env_strict {
        EnvFilter::strict()
    } else {
        EnvFilter::default()
    };

    if args.env_strip_exec {
        filter = filter.with_strip_exec(true);
    }

    if let Some(ref allowed) = args.env_allow {
        filter = filter.with_allowed(allowed);
    }

    if args.env_safe_path {
        filter = filter.with_safe_path(true);
    }

    filter
}
