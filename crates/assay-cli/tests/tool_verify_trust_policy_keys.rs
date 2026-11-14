//! `assay mcp tool verify --trust-policy` acts on every key of the trust policy, or refuses the
//! policy and names the key.
//!
//! The trust policy loaded with serde's defaults and no unknown-key check, so a key the verifier
//! does not read was dropped without a word. `require_signd: true`, one letter short, left
//! `require_signed` at false, and an unsigned tool was accepted with exit 0. `trusted_key_ids`,
//! which `keygen` told users to fill in, never verified a signature: the verifier needs the key
//! itself, which only `trusted_keys[].public_key_path` gives it.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

fn assay() -> Command {
    Command::new(env!("CARGO_BIN_EXE_assay"))
}

struct Case {
    _tmp: TempDir,
    dir: PathBuf,
    key_id: String,
    public_key: PathBuf,
    signed: PathBuf,
    unsigned: PathBuf,
}

fn case() -> Case {
    let tmp = TempDir::new().expect("tempdir");
    let dir = tmp.path().to_path_buf();
    let keys = dir.join("keys");
    std::fs::create_dir_all(&keys).expect("keys dir");
    let keygen = assay()
        .args(["mcp", "tool", "keygen", "--out"])
        .arg(&keys)
        .output()
        .expect("run keygen");
    assert!(keygen.status.success(), "keygen: {keygen:?}");
    let stdout = String::from_utf8_lossy(&keygen.stdout);
    let key_id = stdout
        .lines()
        .find_map(|line| line.strip_prefix("key_id: "))
        .expect("keygen prints the key_id")
        .trim()
        .to_string();

    let unsigned = dir.join("tool.json");
    std::fs::write(
        &unsigned,
        r#"{"name":"deploy","description":"d","inputSchema":{"type":"object"}}"#,
    )
    .expect("write tool");
    let signed = dir.join("signed.json");
    let sign = assay()
        .args(["mcp", "tool", "sign", "--key"])
        .arg(keys.join("private_key.pem"))
        .arg("--out")
        .arg(&signed)
        .arg(&unsigned)
        .output()
        .expect("run sign");
    assert!(sign.status.success(), "sign: {sign:?}");

    Case {
        _tmp: tmp,
        dir,
        key_id,
        public_key: keys.join("public_key.pem"),
        signed,
        unsigned,
    }
}

impl Case {
    fn policy(&self, name: &str, body: &str) -> PathBuf {
        let path = self.dir.join(format!("{name}.yaml"));
        std::fs::write(&path, body).expect("write policy");
        path
    }

    /// A `trusted_keys` entry for this case's key, with any extra lines indented under it.
    fn trusted_key(&self, extra: &str) -> String {
        format!(
            "trusted_keys:\n  - key_id: \"{}\"\n    public_key_path: \"{}\"\n{extra}",
            self.key_id,
            self.public_key.display()
        )
    }
}

fn verify(tool: &Path, policy: &Path) -> Output {
    assay()
        .args(["mcp", "tool", "verify"])
        .arg(tool)
        .arg("--trust-policy")
        .arg(policy)
        .output()
        .expect("run verify")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The refusal names the key by its path, quoted, so a reader can find it in the file.
fn refused(output: &Output, path: &str) -> bool {
    let stderr = text(&output.stderr);
    output.status.code() == Some(1)
        && stderr.contains("assay mcp tool verify refuses this trust policy")
        && stderr.contains(&format!("\"{path}\""))
}

#[test]
fn a_misspelt_require_signed_refuses_the_policy_instead_of_accepting_an_unsigned_tool() {
    let case = case();
    let typo = case.policy("typo", "require_signd: true\n");
    let output = verify(&case.unsigned, &typo);
    assert!(
        refused(&output, "require_signd"),
        "an unsigned tool must not pass a policy that meant to require a signature: {output:?}"
    );
    assert!(
        !text(&output.stdout).contains("Accepted"),
        "the tool was accepted: {output:?}"
    );

    // Control: spelt right, the same policy rejects the unsigned tool as unsigned (exit 2).
    let right = case.policy("right", "require_signed: true\n");
    assert_eq!(verify(&case.unsigned, &right).status.code(), Some(2));
}

#[test]
fn an_unknown_key_anywhere_in_the_policy_is_named_with_its_path() {
    let case = case();
    let nested = case.policy(
        "nested",
        &format!(
            "trusted_keys:\n  - key_id: \"{}\"\n    public_key_pth: \"{}\"\n",
            case.key_id,
            case.public_key.display()
        ),
    );
    let output = verify(&case.signed, &nested);
    assert!(
        refused(&output, "trusted_keys.0.public_key_pth"),
        "{output:?}"
    );

    let root = case.policy(
        "root",
        &format!("trusted_key:\n  - key_id: \"{}\"\n", case.key_id),
    );
    assert!(
        refused(&verify(&case.signed, &root), "trusted_key"),
        "a misspelt root key"
    );

    // Every refused key is named, not only the first.
    let both = case.policy(
        "both",
        &format!(
            "require_signd: true\n{}",
            case.trusted_key("    display_name: ci\n")
        ),
    );
    let output = verify(&case.signed, &both);
    assert!(
        refused(&output, "require_signd") && refused(&output, "trusted_keys.0.display_name"),
        "{output:?}"
    );
}

#[test]
fn trusted_key_ids_is_refused_and_the_refusal_says_where_the_key_goes() {
    let case = case();
    // Alone it never verified: the key id was "trusted" and there was no key to check against.
    let alone = case.policy(
        "alone",
        &format!(
            "require_signed: true\ntrusted_key_ids: [\"{}\"]\n",
            case.key_id
        ),
    );
    let output = verify(&case.signed, &alone);
    assert!(refused(&output, "trusted_key_ids"), "{output:?}");
    let stderr = text(&output.stderr);
    assert!(
        stderr.contains("trusted_keys") && stderr.contains("public_key_path"),
        "the refusal must say what does verify: {stderr}"
    );

    // Beside a trusted_keys entry it had no effect at all: the entry verified.
    let beside = case.policy(
        "beside",
        &format!(
            "trusted_key_ids: [\"sha256:{}\"]\n{}",
            "0".repeat(64),
            case.trusted_key("")
        ),
    );
    assert!(refused(&verify(&case.signed, &beside), "trusted_key_ids"));
}

/// Controls: a policy that holds only keys the verifier acts on still verifies, still accepts an
/// unsigned tool when it does not require a signature, and still rejects one when it does.
#[test]
fn a_policy_of_keys_the_verifier_acts_on_still_verifies() {
    let case = case();
    let full = case.policy(
        "full",
        &format!(
            "require_signed: true\n{}",
            case.trusted_key("    name: release-key\n")
        ),
    );
    let output = verify(&case.signed, &full);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(text(&output.stdout).contains("Verification successful"));

    let open = case.policy("open", "require_signed: false\ntrusted_keys: []\n");
    assert_eq!(verify(&case.unsigned, &open).status.code(), Some(0));
    assert_eq!(verify(&case.unsigned, &full).status.code(), Some(2));
}

/// `name` is documented as the key's display name. It is shown when the key verifies a tool.
#[test]
fn the_name_of_the_trusted_key_that_verified_is_shown() {
    let case = case();
    let named = case.policy("named", &case.trusted_key("    name: release-key\n"));
    let output = verify(&case.signed, &named);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let stdout = text(&output.stdout);
    assert!(
        stdout
            .lines()
            .any(|line| line.trim() == "name:      release-key"),
        "stdout: {stdout}"
    );

    let unnamed = case.policy("unnamed", &case.trusted_key(""));
    let stdout = text(&verify(&case.signed, &unnamed).stdout);
    assert!(!stdout.contains("name:"), "no name was set: {stdout}");
}

/// The refusal and the name are text from the policy file. Neither may write a control byte to the
/// terminal, whatever the file holds.
#[test]
fn text_from_the_policy_reaches_the_terminal_without_control_bytes() {
    let case = case();
    let hostile = "\"\\e]52;c;cHduZWQ=\\a\\e[31m\\u009b31m\\nFATAL: forged\"";
    let key = case.policy("key", &format!("{hostile}: true\n"));
    let output = verify(&case.unsigned, &key);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        text(&output.stderr).contains("refuses this trust policy"),
        "{output:?}"
    );
    let name = case.policy("name", &case.trusted_key(&format!("    name: {hostile}\n")));
    let named = verify(&case.signed, &name);
    assert_eq!(named.status.code(), Some(0), "{named:?}");

    for (label, bytes) in [("stderr", &output.stderr), ("stdout", &named.stdout)] {
        for (index, byte) in bytes.iter().enumerate() {
            assert!(
                (*byte >= 0x20 && *byte != 0x7f) || *byte == b'\n',
                "{label} byte {index} is the control byte {byte:#04x}: {}",
                text(bytes)
            );
        }
        let shown = text(bytes);
        assert!(
            !shown.chars().any(|c| ('\u{80}'..='\u{9f}').contains(&c)),
            "{label} carries a C1 control: {shown}"
        );
        assert!(
            !shown.lines().any(|line| line.starts_with("FATAL: forged")),
            "{label} carries a line the policy wrote: {shown}"
        );
    }
}

/// `keygen` said "Add this key_id to your trust policy", which led to `trusted_key_ids`. It now
/// prints the entry that verifies.
#[test]
fn keygen_prints_the_trusted_keys_entry_that_verifies() {
    let tmp = TempDir::new().expect("tempdir");
    let output = assay()
        .args(["mcp", "tool", "keygen", "--out"])
        .arg(tmp.path())
        .output()
        .expect("run keygen");
    assert!(output.status.success(), "{output:?}");
    let stdout = text(&output.stdout);
    assert!(
        stdout.contains("trusted_keys:")
            && stdout.contains("key_id:")
            && stdout.contains("public_key_path:"),
        "stdout: {stdout}"
    );
    assert!(!stdout.contains("trusted_key_ids"), "stdout: {stdout}");
}

/// The format in the tool signing specification loads: an unsigned tool is rejected as unsigned
/// (exit 2), not refused as a policy (exit 1).
#[test]
fn the_trust_policy_in_the_tool_signing_spec_loads() {
    let spec = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/architecture/SPEC-Tool-Signing-v1.md"),
    )
    .expect("read the spec");
    let section = spec
        .split("### 7.1 Format (YAML)")
        .nth(1)
        .expect("the spec has section 7.1");
    let body = section
        .split("```yaml\n")
        .nth(1)
        .and_then(|rest| rest.split("```").next())
        .expect("section 7.1 has a yaml block");
    let case = case();
    let policy = case.policy("spec", body);
    let output = verify(&case.unsigned, &policy);
    assert_eq!(
        output.status.code(),
        Some(2),
        "the documented policy must load: {output:?}"
    );
}
