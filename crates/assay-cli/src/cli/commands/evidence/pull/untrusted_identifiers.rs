//! Untrusted identifiers reaching a terminal and a filesystem path.
//!
//! A bundle id comes from the command line and a run id can come from a store listing; both are
//! echoed and one of them names a file. Neither was treated as untrusted.

use super::*;
use assay_evidence::store::BundleStore;
use assay_evidence::Bytes;

/// The acceptance twin, and the shape every existing caller already sees. A well-formed id
/// must keep the name it has always had, or this stops being a hardening change and becomes a
/// rename that breaks whatever is looking for the file.
#[test]
fn a_well_formed_id_keeps_its_familiar_name() {
    assert_eq!(bundle_filename("sha256:abc"), "sha256_abc.tar.gz");
    assert_eq!(
        bundle_filename("sha256:0a1b2c3d4e5f"),
        "sha256_0a1b2c3d4e5f.tar.gz"
    );
}

/// The filename is always exactly one component. Asserting on the string alone would miss the
/// point, so this asks the path type: after joining, is the parent still the directory the
/// operator chose?
#[test]
fn no_id_can_name_a_file_outside_the_chosen_directory() {
    let out = Path::new("/tmp/assay-out");
    for hostile in [
        "../../etc/passwd",
        "../escape",
        "..",
        "/absolute/path",
        r"..\..\windows\system32\cfg",
        r"C:\Windows\System32\drivers",
        "nested/dir/file",
        "a/../../b",
        "\u{0}nul",
    ] {
        let name = bundle_filename(hostile);
        let joined = out.join(&name);

        assert_eq!(
            joined.parent(),
            Some(out),
            "id {hostile:?} produced {name:?}, which does not sit directly in the out dir"
        );
        assert_eq!(
            Path::new(&name).components().count(),
            1,
            "id {hostile:?} produced {name:?}, which is more than one component"
        );
        assert!(
            !name.contains('/') && !name.contains('\\'),
            "id {hostile:?} produced {name:?}, which still carries a separator"
        );
        assert!(name.ends_with(".tar.gz"), "{name:?}");
    }
}

/// Terminal control sequences never reach the filename either. `..` survives as text because
/// `.` is a legitimate character in a name; what makes it harmless is that no separator does.
#[test]
fn the_filename_allowlist_admits_only_plain_ascii() {
    let name = bundle_filename("sha\u{1b}[31m256:\u{7}abc\u{9c}");
    assert_eq!(name, "sha__31m256__abc_.tar.gz");
    assert!(
        name.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')),
        "{name:?}"
    );
}

/// An id of nothing but rejected characters is still a name, and an empty one does not become
/// a hidden file called `.tar.gz`.
#[test]
fn a_degenerate_id_still_yields_a_usable_name() {
    assert_eq!(bundle_filename(""), "bundle.tar.gz");
    assert_eq!(bundle_filename("///"), "___.tar.gz");
}

/// Escape sequences are stripped before anything is printed. The identifier itself is not
/// changed by this — only what is written to the terminal.
#[test]
fn identifiers_are_stripped_before_they_reach_a_terminal() {
    let hostile = "sha256:abc\u{1b}[2K\u{1b}]0;pwned\u{7}";
    let rendered = shown(hostile);
    for control in ['\u{1b}', '\u{7}'] {
        assert!(
            !rendered.contains(control),
            "control character {control:?} survived into {rendered:?}"
        );
    }
    assert!(
        rendered.contains("sha256:abc"),
        "the readable part must survive: {rendered:?}"
    );
}

/// Sanitizing is for display only. The lookup uses the original string, because an id altered
/// on the way to the store is a request for a different object — which would turn a hardening
/// change into a silent wrong answer.
#[tokio::test]
async fn the_store_lookup_uses_the_unsanitized_id() {
    // A legitimate id that `bundle_filename` would rewrite, so a lookup keyed on the display
    // form would miss it.
    let id = "sha256:abc";
    let store = ObjectStoreBundleStore::memory();
    store
        .put_bundle(id, Bytes::from(vec![b'x'; 32]))
        .await
        .expect("seed");
    let out = tempfile::tempdir().expect("tmp");

    let code = pull_single(
        &store,
        id,
        out.path(),
        false,
        StreamCeiling::new(100 * 1024),
    )
    .await
    .expect("download");
    assert_eq!(code, 0, "the raw id must still resolve in the store");

    let written: Vec<String> = std::fs::read_dir(out.path())
        .expect("read dir")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(written, vec!["sha256_abc.tar.gz".to_string()]);
}
