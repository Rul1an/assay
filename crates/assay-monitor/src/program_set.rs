//! P2: exact Aya program-name set vs declared inventory (after `Ebpf::load`, before attach).
use crate::object_abi::REBUILD_EBPF_GUIDANCE;
use crate::probes::PROBE_PROGRAMS;
use crate::MonitorError;
use std::collections::BTreeSet;

#[rustfmt::skip]
fn ps_err(detail: impl Into<String>) -> MonitorError {
    MonitorError::ProgramSet { detail: detail.into(), guidance: REBUILD_EBPF_GUIDANCE }
}

/// Exact-set compare of observed program names (Aya `programs()`) against [`PROBE_PROGRAMS`].
pub(crate) fn compare_program_names<'a, I>(observed: I) -> Result<(), MonitorError>
where
    I: IntoIterator<Item = &'a str>,
{
    let observed: BTreeSet<&str> = observed.into_iter().collect();
    let expected: BTreeSet<&str> = PROBE_PROGRAMS.iter().map(|p| p.elf_name).collect();
    if observed == expected {
        return Ok(());
    }
    let missing: Vec<_> = expected.difference(&observed).copied().collect();
    let extra: Vec<_> = observed.difference(&expected).copied().collect();
    Err(ps_err(format!(
        "missing [{}]; extra [{}]",
        missing.join(", "),
        extra.join(", ")
    )))
}

#[cfg(test)]
#[rustfmt::skip]
mod tests {
    use super::*;
    use crate::probes::{AttachSpec, ProbeProgram, PROBE_PROGRAMS};

    fn names() -> Vec<&'static str> { PROBE_PROGRAMS.iter().map(|p| p.elf_name).collect() }

    #[test]
    fn missing_bpf_program_section_is_rejected() {
        let n: Vec<_> = names().into_iter().filter(|n| *n != "assay_monitor_sendto").collect();
        let e = compare_program_names(n).unwrap_err().to_string();
        assert!(e.contains("missing") && e.contains("assay_monitor_sendto") && e.contains("build-ebpf"), "{e}");
    }

    #[test]
    fn extra_bpf_program_section_is_rejected() {
        let mut n = names(); n.push("rogue_probe");
        let e = compare_program_names(n).unwrap_err().to_string();
        assert!(e.contains("extra") && e.contains("rogue_probe") && !e.contains("ELF parse failed"), "{e}");
    }

    #[test]
    fn exact_name_set_is_accepted() {
        assert!(compare_program_names(names()).is_ok());
    }

    #[test]
    fn p2_enumerates_aya_programs_not_elf_sections() {
        let loader = include_str!("loader.rs");
        let prod = include_str!("program_set.rs").split("#[cfg(test)]").next().unwrap();
        assert!(
            loader.contains(".programs()") && !prod.contains("File::parse") && !prod.contains("starts_with('.')"),
            "P2 must compare Aya programs() names, not reparse ELF sections"
        );
    }

    #[test]
    fn loader_attach_sites_use_table_fields_not_literals() {
        let src = include_str!("loader.rs");
        for p in PROBE_PROGRAMS {
            let row = ProbeProgram::by_elf(p.elf_name).expect(p.elf_name);
            let lookup = format!("by_elf(\"{}\")", p.elf_name);
            assert!(!src.contains(&format!("program_mut(\"{}\")", p.elf_name)), "hardcoded program_mut({:?})", p.elf_name);
            if matches!(p.attach, AttachSpec::None) {
                assert!(!src.contains(&lookup), "unattached lookup {}", p.elf_name);
                continue;
            }
            assert!(src.contains(&lookup), "missing by_elf {}", p.elf_name);
            assert!(!src.contains(&format!("attached(\"{}\")", p.surface_name)), "hardcoded attached({:?})", p.surface_name);
            assert!(!src.contains(&format!("skipped(\"{}\")", p.surface_name)), "hardcoded skipped({:?})", p.surface_name);
            match p.attach {
                AttachSpec::Tp(..) => assert!(!src.contains(&format!("attach(\"{}\", \"{}\")", row.tp().0, row.tp().1)), "hardcoded tp {}", p.elf_name),
                AttachSpec::Lsm(_) => assert!(!src.contains(&format!("load(\"{}\"", row.lsm())), "hardcoded lsm {}", p.elf_name),
                AttachSpec::Cgroup4 | AttachSpec::None => {}
            }
        }
    }

    #[test]
    fn loader_reads_all_send_honesty_counters() {
        let src = include_str!("loader.rs");
        for counter in [
            "MONITOR_STAT_SENDTO_NO_PEER",
            "MONITOR_STAT_SENDMSG_NO_PEER",
            "MONITOR_STAT_SENDTO_NON_IP_FAMILY",
            "MONITOR_STAT_SENDMSG_NON_IP_FAMILY",
        ] {
            assert_eq!(
                src.matches(&format!(".get(&{counter}, 0)")).count(),
                1,
                "loader must read {counter} exactly once"
            );
        }
    }
}
