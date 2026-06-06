#[path = "path_projection_next/mod.rs"]
mod path_projection_next;

pub use path_projection_next::{
    project_filesystem_paths, DeclaredPathProjectionRules, DeclaredPathRule, PathProjection,
    PathProjectionMapping, UnmatchedPathSummary, PATH_PROJECTION_SCHEMA,
};
