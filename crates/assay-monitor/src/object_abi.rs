//! P1: pre-`Ebpf::load` CONFIG object-ABI gate via named ELF symbol.
use crate::MonitorError;
use assay_common::{monitor_object_abi_digest, OBJECT_ABI_SYMBOL};
use object::{Object, ObjectSection, ObjectSymbol, SectionKind, SymbolKind};

pub(crate) const REBUILD_EBPF_GUIDANCE: &str = "Rebuild the eBPF object from the same commit as \
assay-monitor:\n  scripts/ci/install-ebpf-toolchain.sh\n  cargo xtask build-ebpf --release --no-docker";

fn abi_err(detail: impl Into<String>) -> MonitorError {
    MonitorError::ObjectAbi {
        detail: detail.into(),
        guidance: REBUILD_EBPF_GUIDANCE,
    }
}

fn is_marker_data_section(kind: SectionKind) -> bool {
    matches!(
        kind,
        SectionKind::Data | SectionKind::ReadOnlyData | SectionKind::ReadOnlyDataWithRel
    )
}

/// Symbol VA → section-relative offset (`address - section.address`), checked.
fn file_offset_in_section(
    sym_addr: u64,
    section_addr: u64,
    len: usize,
) -> Result<usize, MonitorError> {
    let rel = sym_addr
        .checked_sub(section_addr)
        .ok_or_else(|| abi_err(format!("{OBJECT_ABI_SYMBOL} address before section base")))?;
    let start = usize::try_from(rel)
        .map_err(|_| abi_err(format!("{OBJECT_ABI_SYMBOL} offset overflows usize")))?;
    start
        .checked_add(4)
        .filter(|&end| end <= len)
        .ok_or_else(|| abi_err(format!("{OBJECT_ABI_SYMBOL} truncated")))?;
    Ok(start)
}

/// Read `ASSAY_MONITOR_OBJECT_ABI` (u32 LE) from a named ELF symbol.
pub(crate) fn read_object_abi_symbol(data: &[u8]) -> Result<u32, MonitorError> {
    let file = object::File::parse(data).map_err(|e| abi_err(format!("ELF parse failed: {e}")))?;
    let mut matches = file.symbols().filter(|s| s.name() == Ok(OBJECT_ABI_SYMBOL));
    let sym = matches.next().ok_or_else(|| {
        abi_err(format!(
            "missing symbol {OBJECT_ABI_SYMBOL} (expected digest {:#x})",
            monitor_object_abi_digest()
        ))
    })?;
    if matches.next().is_some() {
        return Err(abi_err(format!("duplicate symbol {OBJECT_ABI_SYMBOL}")));
    }
    if !sym.is_definition() {
        return Err(abi_err(format!(
            "{OBJECT_ABI_SYMBOL} is not a defined symbol"
        )));
    }
    if sym.kind() != SymbolKind::Data {
        return Err(abi_err(format!(
            "{OBJECT_ABI_SYMBOL} kind {:?} (want Data)",
            sym.kind()
        )));
    }
    if sym.size() != 4 {
        return Err(abi_err(format!(
            "{OBJECT_ABI_SYMBOL} size {} (want 4)",
            sym.size()
        )));
    }
    let section = match sym.section() {
        object::SymbolSection::Section(i) => file
            .section_by_index(i)
            .map_err(|e| abi_err(format!("symbol section: {e}")))?,
        _ => {
            return Err(abi_err(format!(
                "{OBJECT_ABI_SYMBOL} not in a data section"
            )))
        }
    };
    if !is_marker_data_section(section.kind()) {
        return Err(abi_err(format!(
            "{OBJECT_ABI_SYMBOL} section kind {:?} (want data/rodata)",
            section.kind()
        )));
    }
    let bytes = section
        .data()
        .map_err(|e| abi_err(format!("section data: {e}")))?;
    let start = file_offset_in_section(sym.address(), section.address(), bytes.len())?;
    let le: [u8; 4] = bytes[start..start + 4]
        .try_into()
        .map_err(|_| abi_err(format!("{OBJECT_ABI_SYMBOL} truncated")))?;
    Ok(u32::from_le_bytes(le))
}

/// Fail closed on missing/mismatched object-ABI symbol before Aya load.
pub(crate) fn verify_object_abi_marker(data: &[u8]) -> Result<(), MonitorError> {
    let expected = monitor_object_abi_digest();
    let observed = read_object_abi_symbol(data)?;
    if observed != expected {
        return Err(abi_err(format!(
            "digest mismatch: expected {expected:#x}, observed {observed:#x}"
        )));
    }
    Ok(())
}

#[cfg(test)]
#[rustfmt::skip]
mod tests {
    use super::*;
    use object::write::{Object as WObject, StandardSection, Symbol, SymbolFlags, SymbolKind, SymbolScope, SymbolSection};
    use object::{Architecture, BinaryFormat, Endianness};

    struct MarkerSpec {
        payload: Vec<u8>,
        names: Vec<&'static str>,
        off: u64,
        size: u64,
        kind: SymbolKind,
        section: StandardSection,
    }

    fn elf(spec: MarkerSpec) -> Vec<u8> {
        let mut o = WObject::new(BinaryFormat::Elf, Architecture::X86_64, Endianness::Little);
        let s = o.section_id(spec.section);
        let _ = o.append_section_data(s, &spec.payload, 4);
        for n in spec.names {
            o.add_symbol(Symbol {
                name: n.as_bytes().to_vec(), value: spec.off, size: spec.size, kind: spec.kind,
                scope: SymbolScope::Linkage, weak: false, section: SymbolSection::Section(s),
                flags: SymbolFlags::None,
            });
        }
        o.write().expect("ELF")
    }

    fn good_payload() -> Vec<u8> {
        monitor_object_abi_digest().to_le_bytes().to_vec()
    }

    fn good() -> MarkerSpec {
        MarkerSpec {
            payload: good_payload(), names: vec![OBJECT_ABI_SYMBOL], off: 0, size: 4,
            kind: SymbolKind::Data, section: StandardSection::ReadOnlyData,
        }
    }

    #[test]
    fn missing_abi_symbol_rejected() {
        let mut s = good();
        s.names = vec!["OTHER"];
        s.payload = b"x".to_vec();
        let e = verify_object_abi_marker(&elf(s)).unwrap_err().to_string();
        assert!(e.contains("missing symbol") && e.contains(OBJECT_ABI_SYMBOL), "{e}");
        assert!(!e.contains("ELF parse failed"), "{e}");
        assert!(e.contains("install-ebpf-toolchain") && e.contains("build-ebpf"), "{e}");
    }

    #[test]
    fn mismatched_abi_symbol_rejected() {
        let mut s = good();
        s.payload = (monitor_object_abi_digest() ^ 1).to_le_bytes().to_vec();
        let e = verify_object_abi_marker(&elf(s)).unwrap_err().to_string();
        assert!(e.contains("mismatch") && e.contains("install-ebpf-toolchain"), "{e}");
    }

    #[test]
    fn matching_abi_and_relative_offset() {
        assert!(verify_object_abi_marker(&elf(good())).is_ok());
        let mut pad = vec![0u8; 64];
        pad.extend_from_slice(&good_payload());
        let mut s = good();
        s.payload = pad;
        s.off = 64;
        assert!(verify_object_abi_marker(&elf(s)).is_ok());
        assert_eq!(file_offset_in_section(0x1008, 0x1000, 16).unwrap(), 8);
        assert!(file_offset_in_section(0x0ff0, 0x1000, 16).is_err());
    }

    #[test]
    fn duplicate_matching_symbol_rejected() {
        let mut s = good();
        s.names = vec![OBJECT_ABI_SYMBOL, OBJECT_ABI_SYMBOL];
        let e = verify_object_abi_marker(&elf(s)).unwrap_err().to_string();
        assert!(e.contains("duplicate") && e.contains(OBJECT_ABI_SYMBOL), "{e}");
    }

    #[test]
    fn wrong_symbol_kind_rejected() {
        let mut s = good();
        s.kind = SymbolKind::Text;
        let e = verify_object_abi_marker(&elf(s)).unwrap_err().to_string();
        assert!(e.contains("kind") && e.contains("Data"), "{e}");
    }

    #[test]
    fn wrong_or_zero_symbol_size_rejected() {
        for size in [0u64, 2, 8] {
            let mut s = good();
            s.size = size;
            let e = verify_object_abi_marker(&elf(s)).unwrap_err().to_string();
            assert!(e.contains("size") && e.contains("want 4"), "{e}");
        }
    }

    #[test]
    fn truncated_marker_data_rejected() {
        let mut s = good();
        s.payload = monitor_object_abi_digest().to_le_bytes()[..2].to_vec();
        let e = verify_object_abi_marker(&elf(s)).unwrap_err().to_string();
        assert!(e.contains("truncated"), "{e}");
    }

    #[test]
    fn non_data_section_rejected() {
        let mut s = good();
        s.section = StandardSection::Text;
        let e = verify_object_abi_marker(&elf(s)).unwrap_err().to_string();
        assert!(
            e.contains("section kind") || e.contains("not in a data section"),
            "{e}"
        );
    }

    #[test]
    #[ignore = "requires target/assay-ebpf.o from cargo xtask build-ebpf"]
    fn baked_ebpf_object_exports_matching_abi_symbol() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/assay-ebpf.o");
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("missing {}: build-ebpf ({e})", path.display()));
        assert_eq!(read_object_abi_symbol(&bytes).expect("ABI symbol"), monitor_object_abi_digest());
    }
}
