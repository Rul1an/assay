use super::aggregate::{aggregate_run, merge_run};
use super::*;
use crate::cli::commands::profile_types::{Profile, ProfileEntry};

#[test]
fn aggregate_dedup() {
    let events = vec![
        Event::FileOpen {
            path: "/a".into(),
            timestamp: 100,
        },
        Event::FileOpen {
            path: "/a".into(),
            timestamp: 200,
        },
        Event::FileOpen {
            path: "/b".into(),
            timestamp: 150,
        },
    ];
    let run = aggregate_run(&events);
    assert_eq!(run.files.len(), 2);
    assert_eq!(run.files["/a"].hits, 2);
    assert_eq!(run.files["/a"].timestamp, 200);
}

#[test]
fn merge_new_entries() {
    let mut profile = Profile::new("test", None);
    let events = vec![Event::FileOpen {
        path: "/a".into(),
        timestamp: 100,
    }];
    let run = aggregate_run(&events);
    let (new, updated) = merge_run(&mut profile, &run);

    assert_eq!(new, 1);
    assert_eq!(updated, 0);
    assert_eq!(profile.entries.files["/a"].runs_seen, 1);
}

#[test]
fn merge_existing_entries() {
    let mut profile = Profile::new("test", None);
    profile
        .entries
        .files
        .insert("/a".into(), ProfileEntry::new(100, 5));

    let events = vec![
        Event::FileOpen {
            path: "/a".into(),
            timestamp: 200,
        },
        Event::FileOpen {
            path: "/a".into(),
            timestamp: 200,
        },
    ];
    let run = aggregate_run(&events);
    let (new, updated) = merge_run(&mut profile, &run);

    assert_eq!(new, 0);
    assert_eq!(updated, 1);
    assert_eq!(profile.entries.files["/a"].runs_seen, 2);
    assert_eq!(profile.entries.files["/a"].hits_total, 7); // 5 + 2
}

#[test]
fn scope_guard_mismatch() {
    let mut p = Profile::new("test", Some("scope-A".into()));
    let new_scope = Some("scope-B".to_string());

    // Mismatch without force -> Error
    let res = enforce_scope(&mut p, new_scope.as_ref(), false);
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("Scope mismatch"));

    // Mismatch with force -> Ok (no change to profile scope effectively, runs just get merged)
    // Wait, current logic allows update but doesn't change profile scope. That's desired behavior.
    let res_force = enforce_scope(&mut p, new_scope.as_ref(), true);
    assert!(res_force.is_ok());
    assert_eq!(p.scope.as_deref(), Some("scope-A"));
}

#[test]
fn scope_guard_init() {
    let mut p = Profile::new("test", None);
    let new_scope = Some("scope-init".to_string());

    // First time -> set scope
    assert!(enforce_scope(&mut p, new_scope.as_ref(), false).is_ok());
    assert_eq!(p.scope.as_deref(), Some("scope-init"));
}

#[test]
fn scope_guard_noop() {
    let mut p = Profile::new("test", Some("scope-A".into()));

    // Matching scope -> Ok
    assert!(enforce_scope(&mut p, Some(&"scope-A".to_string()), false).is_ok());

    // No incoming scope -> Ok
    assert!(enforce_scope(&mut p, None, false).is_ok());
}
