//! Exclusive publication of a report and its inputs record into a fresh directory.
//!
//! The directory must not exist: it is created with `create_dir`, never reused, and
//! each member is created with `create_new`. The report is written and synced first
//! and the sidecar last, so a sidecar never names a report that was not written
//! before it. Nothing is cleaned up or retried after a failure, and nothing here is
//! atomic or durable beyond what the syncs ask of the platform: a failure after the
//! directory exists leaves whatever was written, and the process exit says it failed.
//!
//! The caller is assumed to control the parent directory. A same-user process racing
//! on that parent is outside this writer's threat model.

use super::inputs::{INPUTS_FILE, REPORT_FILE};
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// The two finished documents, rendered and validated as a pair before any output
/// exists.
pub(super) struct Publication {
    pub report: Vec<u8>,
    pub sidecar: Vec<u8>,
}

/// File-system steps of a publication, so tests can fail any single one.
pub(super) trait PublishOps {
    type Member: Write;
    fn create_dir(&mut self, dir: &Path) -> io::Result<()>;
    fn create_new(&mut self, path: &Path) -> io::Result<Self::Member>;
    fn sync_member(&mut self, member: &mut Self::Member) -> io::Result<()>;
    fn sync_dir(&mut self, dir: &Path) -> io::Result<()>;
}

pub(super) struct OsOps;

impl PublishOps for OsOps {
    type Member = File;

    fn create_dir(&mut self, dir: &Path) -> io::Result<()> {
        std::fs::create_dir(dir)
    }

    fn create_new(&mut self, path: &Path) -> io::Result<File> {
        OpenOptions::new().write(true).create_new(true).open(path)
    }

    fn sync_member(&mut self, member: &mut File) -> io::Result<()> {
        member.sync_all()
    }

    #[cfg(unix)]
    fn sync_dir(&mut self, dir: &Path) -> io::Result<()> {
        File::open(dir)?.sync_all()
    }

    /// Directory sync is not available through std on this platform.
    #[cfg(not(unix))]
    fn sync_dir(&mut self, _dir: &Path) -> io::Result<()> {
        Ok(())
    }
}

/// Why the destination was refused before anything was created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Destination {
    Exists,
    MissingParent,
    ParentNotDirectory,
}

impl Destination {
    pub(super) fn describe(self) -> &'static str {
        match self {
            Self::Exists => "already exists; a fresh path is required",
            Self::MissingParent => "parent directory does not exist",
            Self::ParentNotDirectory => "parent is not a directory",
        }
    }
}

#[derive(Debug)]
pub(super) enum PublishError {
    /// Refused destination: nothing was created. Exit 2.
    Refused(Destination),
    /// Any other failure, before or after the directory exists. Exit 3.
    Io { target: PathBuf, error: io::Error },
}

fn classify_create_dir(dir: &Path, error: io::Error) -> PublishError {
    let refused = match error.kind() {
        io::ErrorKind::AlreadyExists => Some(Destination::Exists),
        io::ErrorKind::NotFound => Some(Destination::MissingParent),
        io::ErrorKind::NotADirectory => Some(Destination::ParentNotDirectory),
        _ => None,
    };
    match refused {
        Some(destination) => PublishError::Refused(destination),
        None => PublishError::Io {
            target: dir.to_path_buf(),
            error,
        },
    }
}

fn io_at(target: &Path) -> impl FnOnce(io::Error) -> PublishError + '_ {
    move |error| PublishError::Io {
        target: target.to_path_buf(),
        error,
    }
}

fn write_member<O: PublishOps>(ops: &mut O, path: &Path, bytes: &[u8]) -> Result<(), PublishError> {
    let mut member = ops.create_new(path).map_err(io_at(path))?;
    member.write_all(bytes).map_err(io_at(path))?;
    member.flush().map_err(io_at(path))?;
    ops.sync_member(&mut member).map_err(io_at(path))
}

/// Create `dir` and write the pair into it, report first and sidecar last.
pub(super) fn publish<O: PublishOps>(
    ops: &mut O,
    dir: &Path,
    publication: &Publication,
) -> Result<(), PublishError> {
    ops.create_dir(dir)
        .map_err(|error| classify_create_dir(dir, error))?;
    write_member(ops, &dir.join(REPORT_FILE), &publication.report)?;
    write_member(ops, &dir.join(INPUTS_FILE), &publication.sidecar)?;
    ops.sync_dir(dir).map_err(io_at(dir))
}

impl PublishError {
    /// Exit 2 for a refused destination, 3 for everything else.
    pub(super) fn exit_code(&self) -> i32 {
        match self {
            Self::Refused(_) => crate::exit_codes::EXIT_CONFIG_ERROR,
            Self::Io { .. } => crate::exit_codes::EXIT_INFRA_ERROR,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::inputs::{check_directory, OverallStatus};
    use super::*;
    use crate::exit_codes::{EXIT_CONFIG_ERROR, EXIT_INFRA_ERROR};
    use std::sync::{Arc, Barrier};

    /// One publication step, in the order `publish` performs them.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Step {
        CreateDir,
        CreateReport,
        WriteReport,
        SyncReport,
        CreateSidecar,
        WriteSidecar,
        SyncSidecar,
        SyncDir,
    }

    const POST_CREATE: [Step; 7] = [
        Step::CreateReport,
        Step::WriteReport,
        Step::SyncReport,
        Step::CreateSidecar,
        Step::WriteSidecar,
        Step::SyncSidecar,
        Step::SyncDir,
    ];

    /// Real file-system operations with one step replaced by an injected failure.
    /// A failed write still writes the first half of its buffer, so a partial member
    /// is left behind exactly as a short write would leave it.
    struct Faulty {
        fail: Step,
        kind: io::ErrorKind,
        members_created: usize,
    }

    struct FaultyMember {
        file: File,
        fail_write: Option<io::ErrorKind>,
    }

    impl Write for FaultyMember {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            match self.fail_write {
                Some(kind) => {
                    self.file.write_all(&buf[..buf.len() / 2])?;
                    Err(io::Error::new(kind, "injected write failure"))
                }
                None => self.file.write(buf),
            }
        }

        fn flush(&mut self) -> io::Result<()> {
            self.file.flush()
        }
    }

    impl Faulty {
        fn new(fail: Step) -> Self {
            Self {
                fail,
                kind: io::ErrorKind::Other,
                members_created: 0,
            }
        }

        fn inject(&self, step: Step) -> io::Result<()> {
            if step == self.fail {
                Err(io::Error::new(self.kind, "injected failure"))
            } else {
                Ok(())
            }
        }

        fn steps(&self) -> (Step, Step, Step) {
            if self.members_created <= 1 {
                (Step::CreateReport, Step::WriteReport, Step::SyncReport)
            } else {
                (Step::CreateSidecar, Step::WriteSidecar, Step::SyncSidecar)
            }
        }
    }

    impl PublishOps for Faulty {
        type Member = FaultyMember;

        fn create_dir(&mut self, dir: &Path) -> io::Result<()> {
            self.inject(Step::CreateDir)?;
            OsOps.create_dir(dir)
        }

        fn create_new(&mut self, path: &Path) -> io::Result<FaultyMember> {
            self.members_created += 1;
            let (create, write, _) = self.steps();
            self.inject(create)?;
            Ok(FaultyMember {
                file: OsOps.create_new(path)?,
                fail_write: (write == self.fail).then_some(self.kind),
            })
        }

        fn sync_member(&mut self, member: &mut FaultyMember) -> io::Result<()> {
            let (_, _, sync) = self.steps();
            self.inject(sync)?;
            OsOps.sync_member(&mut member.file)
        }

        fn sync_dir(&mut self, dir: &Path) -> io::Result<()> {
            self.inject(Step::SyncDir)?;
            OsOps.sync_dir(dir)
        }
    }

    /// A pair the reader binds: built by hand so this test does not depend on the
    /// generator it is not testing.
    fn publication() -> Publication {
        let report = b"{\n  \"claims\": []\n}\n".to_vec();
        let sidecar = format!(
            "{{\n  \"schema\": \"assay.trust-basis.inputs.v0\",\n  \"report\": {{\n    \"sha256\": \"{}\",\n    \"bytes\": {}\n  }},\n  \"bundle\": {{\n    \"sha256\": \"{}\",\n    \"bytes\": 1\n  }},\n  \"pack_digest_domain\": \"assay.parsed-pack-definition.jcs.sha256\",\n  \"packs\": [],\n  \"limits\": {{\n    \"max_bundle_bytes\": 1,\n    \"max_decode_bytes\": 1,\n    \"max_manifest_bytes\": 1,\n    \"max_events_bytes\": 1,\n    \"max_events\": 1,\n    \"max_line_bytes\": 1,\n    \"max_path_len\": 1,\n    \"max_json_depth\": 1\n  }},\n  \"lint\": {{\n    \"enabled\": false\n  }},\n  \"reported_assay_version\": \"test\"\n}}\n",
            super::super::inputs::sha256_hex(&report),
            report.len(),
            "0".repeat(64),
        );
        Publication {
            report,
            sidecar: sidecar.into_bytes(),
        }
    }

    fn members(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn the_hand_built_pair_binds_when_nothing_fails() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("out");
        publish(&mut OsOps, &dir, &publication()).unwrap();
        assert_eq!(check_directory(&dir, None).status(), OverallStatus::Bound);
    }

    #[test]
    fn every_failure_after_the_directory_exists_is_exit_three_and_nothing_is_cleaned_up() {
        let pair = publication();
        for step in POST_CREATE {
            let root = tempfile::tempdir().unwrap();
            let dir = root.path().join("out");
            let error = publish(&mut Faulty::new(step), &dir, &pair).unwrap_err();
            assert_eq!(error.exit_code(), EXIT_INFRA_ERROR, "{step:?}");
            match &error {
                PublishError::Io { error, .. } => {
                    assert!(
                        error.to_string().starts_with("injected"),
                        "{step:?}: {error}"
                    )
                }
                other => panic!("{step:?}: expected an io failure, got {other:?}"),
            }
            assert!(dir.is_dir(), "{step:?}: the directory stays");

            let report = std::fs::read(dir.join(REPORT_FILE)).ok();
            let sidecar = std::fs::read(dir.join(INPUTS_FILE)).ok();
            let status = check_directory(&dir, None).status();
            match step {
                Step::CreateReport => {
                    assert_eq!(members(&dir), Vec::<String>::new());
                    assert_eq!(status, OverallStatus::Incomplete);
                }
                Step::WriteReport => {
                    assert_eq!(report.unwrap(), pair.report[..pair.report.len() / 2]);
                    assert_eq!(
                        sidecar, None,
                        "the sidecar is never written after a failure"
                    );
                    assert_eq!(status, OverallStatus::Incomplete);
                }
                Step::SyncReport | Step::CreateSidecar => {
                    assert_eq!(report.unwrap(), pair.report);
                    assert_eq!(sidecar, None);
                    assert_eq!(status, OverallStatus::Incomplete);
                }
                Step::WriteSidecar => {
                    assert_eq!(report.unwrap(), pair.report);
                    assert_eq!(sidecar.unwrap(), pair.sidecar[..pair.sidecar.len() / 2]);
                    assert_eq!(status, OverallStatus::Invalid);
                }
                // Both members are complete; only the sync failed. The files may
                // bind, and the process exit still reports the failure.
                Step::SyncSidecar | Step::SyncDir => {
                    assert_eq!(report.unwrap(), pair.report);
                    assert_eq!(sidecar.unwrap(), pair.sidecar);
                    assert_eq!(status, OverallStatus::Bound);
                }
                Step::CreateDir => unreachable!(),
            }
        }
    }

    #[test]
    fn create_dir_failures_split_into_refused_destinations_and_other_errors() {
        let cases = [
            (io::ErrorKind::AlreadyExists, EXIT_CONFIG_ERROR),
            (io::ErrorKind::NotFound, EXIT_CONFIG_ERROR),
            (io::ErrorKind::NotADirectory, EXIT_CONFIG_ERROR),
            (io::ErrorKind::PermissionDenied, EXIT_INFRA_ERROR),
            (io::ErrorKind::StorageFull, EXIT_INFRA_ERROR),
            (io::ErrorKind::Other, EXIT_INFRA_ERROR),
        ];
        for (kind, exit) in cases {
            let root = tempfile::tempdir().unwrap();
            let dir = root.path().join("out");
            let mut ops = Faulty::new(Step::CreateDir);
            ops.kind = kind;
            let error = publish(&mut ops, &dir, &publication()).unwrap_err();
            assert_eq!(error.exit_code(), exit, "{kind:?}");
            assert!(!dir.exists(), "{kind:?}: nothing was created");
        }
    }

    #[test]
    fn real_refusals_are_classified_by_the_operating_system() {
        let root = tempfile::tempdir().unwrap();
        let existing = root.path().join("existing");
        std::fs::create_dir(&existing).unwrap();
        let file = root.path().join("file");
        std::fs::write(&file, b"x").unwrap();
        let cases = [
            (existing, Destination::Exists),
            (
                root.path().join("absent").join("out"),
                Destination::MissingParent,
            ),
            (file.join("out"), Destination::ParentNotDirectory),
        ];
        for (dir, expected) in cases {
            match publish(&mut OsOps, &dir, &publication()) {
                Err(PublishError::Refused(destination)) => assert_eq!(destination, expected),
                other => panic!("{}: expected {expected:?}, got {other:?}", dir.display()),
            }
        }
    }

    #[test]
    fn two_writers_racing_for_one_directory_produce_exactly_one_publication() {
        let root = tempfile::tempdir().unwrap();
        let dir = Arc::new(root.path().join("out"));
        let barrier = Arc::new(Barrier::new(2));
        let writers: Vec<_> = (0..2u8)
            .map(|writer| {
                let dir = Arc::clone(&dir);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    let mut pair = publication();
                    // Distinct report bytes per writer, so a mixed directory is visible.
                    pair.report = format!("writer {writer}\n").into_bytes();
                    barrier.wait();
                    (
                        writer,
                        publish(&mut OsOps, &dir, &pair).map(|()| pair.report),
                    )
                })
            })
            .collect();
        let results: Vec<_> = writers.into_iter().map(|w| w.join().unwrap()).collect();

        let winners: Vec<_> = results.iter().filter(|(_, r)| r.is_ok()).collect();
        assert_eq!(winners.len(), 1, "exactly one writer creates the directory");
        let losers: Vec<_> = results.iter().filter(|(_, r)| r.is_err()).collect();
        match &losers[0].1 {
            Err(PublishError::Refused(Destination::Exists)) => {}
            other => panic!("the loser must be refused as existing, got {other:?}"),
        }
        let winning_report = winners[0].1.as_ref().unwrap();
        assert_eq!(
            &std::fs::read(dir.join(REPORT_FILE)).unwrap(),
            winning_report
        );
        assert_eq!(members(&dir), [INPUTS_FILE, REPORT_FILE]);
    }
}
