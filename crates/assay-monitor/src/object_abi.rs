//! P1: pre-`Ebpf::load` CONFIG object-ABI gate via named ELF symbol.
use crate::MonitorError;
use assay_common::{monitor_object_abi_digest, OBJECT_ABI_SYMBOL};
use object::{Object, ObjectSection, ObjectSymbol};

pub(crate) const REBUILD_EBPF_GUIDANCE: &str = "Rebuild the eBPF object from the same commit as \
assay-monitor:\n  scripts/ci/install-ebpf-toolchain.sh\n  cargo xtask build-ebpf --release --no-docker";

fn abi_err(detail: impl Into<String>) -> MonitorError {
    MonitorError::ObjectAbi {
        detail: detail.into(),
        guidance: REBUILD_EBPF_GUIDANCE,
    }
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
    let sym = file
        .symbols()
        .find(|s| s.name() == Ok(OBJECT_ABI_SYMBOL))
        .ok_or_else(|| {
            abi_err(format!(
                "missing symbol {OBJECT_ABI_SYMBOL} (expected digest {:#x})",
                monitor_object_abi_digest()
            ))
        })?;
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

    fn elf(payload: &[u8], name: Option<&str>, off: u64) -> Vec<u8> {
        let mut o = WObject::new(BinaryFormat::Elf, Architecture::X86_64, Endianness::Little);
        let s = o.section_id(StandardSection::ReadOnlyData);
        let _ = o.append_section_data(s, payload, 4);
        if let Some(n) = name {
            o.add_symbol(Symbol {
                name: n.as_bytes().to_vec(), value: off, size: 4, kind: SymbolKind::Data,
                scope: SymbolScope::Linkage, weak: false, section: SymbolSection::Section(s),
                flags: SymbolFlags::None,
            });
        }
        o.write().expect("ELF")
    }

    #[test]
    fn missing_abi_symbol_rejected() {
        let e = verify_object_abi_marker(&elf(b"x", Some("OTHER"), 0)).unwrap_err().to_string();
        assert!(e.contains("missing symbol") && e.contains(OBJECT_ABI_SYMBOL), "{e}");
        assert!(!e.contains("ELF parse failed"), "{e}");
        assert!(e.contains("install-ebpf-toolchain") && e.contains("build-ebpf"), "{e}");
    }

    #[test]
    fn mismatched_abi_symbol_rejected() {
        let d = (monitor_object_abi_digest() ^ 1).to_le_bytes();
        let e = verify_object_abi_marker(&elf(&d, Some(OBJECT_ABI_SYMBOL), 0)).unwrap_err().to_string();
        assert!(e.contains("mismatch") && e.contains("install-ebpf-toolchain"), "{e}");
    }

    #[test]
    fn matching_abi_and_relative_offset() {
        let d = monitor_object_abi_digest();
        assert!(verify_object_abi_marker(&elf(&d.to_le_bytes(), Some(OBJECT_ABI_SYMBOL), 0)).is_ok());
        let mut pad = vec![0u8; 64];
        pad.extend_from_slice(&d.to_le_bytes());
        assert!(verify_object_abi_marker(&elf(&pad, Some(OBJECT_ABI_SYMBOL), 64)).is_ok());
        assert_eq!(file_offset_in_section(0x1008, 0x1000, 16).unwrap(), 8);
        assert!(file_offset_in_section(0x0ff0, 0x1000, 16).is_err());
    }

    #[test]
    #[ignore = "requires target/assay-ebpf.o from cargo xtask build-ebpf"]
    fn baked_ebpf_object_exports_matching_abi_symbol() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/assay-ebpf.o");
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("missing {}: build-ebpf ({e})", path.display()));
        assert_eq!(read_object_abi_symbol(&bytes).expect("ABI symbol"), monitor_object_abi_digest());
    }
}
