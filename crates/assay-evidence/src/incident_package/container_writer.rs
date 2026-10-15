//! Canonical framing for already selected Incident Package v1 member bytes.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use super::{ContextLimits, IncidentReason};
use crate::json_strict::{check_next, BudgetDimension, BudgetLimit, BudgetTier};

/// Frame original member bytes as a deterministic, uncompressed ustar container.
///
/// Inventory and assessment bytes are preserved verbatim. Objects are stored once
/// per SHA-256 digest, in digest order, assuming SHA-256 collision resistance.
/// Hard container and metadata byte limits apply before output allocation.
///
/// This only constructs framing: it does not validate JSON, inventory references,
/// input occurrences or format-specific budgets, and does not verify or publish a
/// package. Callers must verify the completed package before publishing it.
pub fn write_incident_container(
    inventory: &[u8],
    assessment: &[u8],
    objects: &[&[u8]],
) -> Result<Vec<u8>, IncidentReason> {
    write_incident_container_with_limits(inventory, assessment, objects, &ContextLimits::HARD)
}

pub(crate) fn write_incident_container_with_limits(
    inventory: &[u8],
    assessment: &[u8],
    objects: &[&[u8]],
    limits: &ContextLimits,
) -> Result<Vec<u8>, IncidentReason> {
    if !limits.is_valid() {
        return Err(IncidentReason::TrustInput);
    }
    let hard = &ContextLimits::HARD;
    charge(
        0,
        inventory.len() as u64,
        hard.inventory_bytes,
        limits.inventory_bytes,
    )?;
    charge(
        0,
        assessment.len() as u64,
        hard.assessment_bytes,
        limits.assessment_bytes,
    )?;
    let mut members = charge(0, 2, hard.members, limits.members)?;
    charge(0, 14, hard.outer_path_bytes, limits.outer_path_bytes)?;
    charge(0, 15, hard.outer_path_bytes, limits.outer_path_bytes)?;
    let mut total = charge(
        0,
        member_size(inventory.len())?,
        hard.outer_bytes,
        limits.outer_bytes,
    )?;
    total = charge(
        total,
        member_size(assessment.len())?,
        hard.outer_bytes,
        limits.outer_bytes,
    )?;
    total = charge(total, 1024, hard.outer_bytes, limits.outer_bytes)?;
    let fixed = total;
    let mut unique = BTreeMap::<[u8; 32], &[u8]>::new();
    for &object in objects {
        charge(0, 72, hard.outer_path_bytes, limits.outer_path_bytes)?;
        let size = member_size(object.len())?;
        // Bound hashing by the smallest archive containing this object. Checking
        // the remaining cumulative budget here would wrongly reject duplicates.
        charge(fixed, size, hard.outer_bytes, limits.outer_bytes)?;
        let digest: [u8; 32] = Sha256::digest(object).into();
        if let std::collections::btree_map::Entry::Vacant(entry) = unique.entry(digest) {
            members = charge(members, 1, hard.members, limits.members)?;
            total = charge(total, size, hard.outer_bytes, limits.outer_bytes)?;
            entry.insert(object);
        }
    }

    let mut output = Vec::with_capacity(total as usize);
    append_member(&mut output, "inventory.json", inventory);
    append_member(&mut output, "assessment.json", assessment);
    for (digest, object) in unique {
        append_member(
            &mut output,
            &format!("objects/{}", hex::encode(digest)),
            object,
        );
    }
    output.resize(output.len() + 1024, 0);
    debug_assert_eq!(output.len() as u64, total);
    Ok(output)
}

fn charge(previous: u64, delta: u64, hard: u64, selected: u64) -> Result<u64, IncidentReason> {
    check_next(
        previous,
        delta,
        BudgetLimit { hard, selected },
        BudgetDimension::Items,
    )
    .map_err(|hit| match hit.tier {
        BudgetTier::Hard => IncidentReason::ResourceHard,
        BudgetTier::Selected => IncidentReason::ResourceSelected,
    })
}

fn member_size(len: usize) -> Result<u64, IncidentReason> {
    let len = u64::try_from(len).map_err(|_| IncidentReason::ResourceHard)?;
    len.checked_add(511)
        .map(|n| (n / 512) * 512)
        .and_then(|n| n.checked_add(512))
        .ok_or(IncidentReason::ResourceHard)
}

fn append_member(output: &mut Vec<u8>, name: &str, bytes: &[u8]) {
    let mut header = tar::Header::new_ustar();
    header.set_mode(0o644);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    header.set_size(bytes.len() as u64);
    header.set_entry_type(tar::EntryType::Regular);
    let header = header.as_mut_bytes();
    // Names are generated ASCII constants/digests, not platform filesystem paths.
    header[..name.len()].copy_from_slice(name.as_bytes());
    header[148..156].fill(b' ');
    let checksum: u32 = header.iter().map(|&b| u32::from(b)).sum();
    header[148..156].copy_from_slice(format!("{checksum:06o}\0 ").as_bytes());
    output.extend_from_slice(header);
    output.extend_from_slice(bytes);
    output.resize(output.len().next_multiple_of(512), 0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_container_limits_are_inclusive_and_duplicates_are_free() {
        let object = [7; 600];
        let mut limits = ContextLimits::HARD;
        limits.inventory_bytes = 3;
        limits.assessment_bytes = 3;
        limits.members = 3;
        limits.outer_path_bytes = 72;
        limits.outer_bytes = 4608;
        let bytes =
            write_incident_container_with_limits(b"inv", b"ass", &[&object, &object], &limits)
                .unwrap();
        assert_eq!(bytes.len(), 4608);
        for dimension in 0..5 {
            let mut lower = limits.clone();
            match dimension {
                0 => lower.inventory_bytes -= 1,
                1 => lower.assessment_bytes -= 1,
                2 => lower.members -= 1,
                3 => lower.outer_path_bytes -= 1,
                _ => lower.outer_bytes -= 1,
            }
            assert_eq!(
                write_incident_container_with_limits(b"inv", b"ass", &[&object, &object], &lower),
                Err(IncidentReason::ResourceSelected),
                "dimension {dimension}"
            );
        }
    }

    #[test]
    fn invalid_limits_refuse_and_hard_limits_win_over_selected() {
        let mut limits = ContextLimits::HARD;
        limits.members = 0;
        assert_eq!(
            write_incident_container_with_limits(b"", b"", &[], &limits),
            Err(IncidentReason::TrustInput)
        );
        limits = ContextLimits::HARD;
        limits.outer_bytes += 1;
        assert_eq!(
            write_incident_container_with_limits(b"", b"", &[], &limits),
            Err(IncidentReason::TrustInput)
        );
        let inventory = vec![0; ContextLimits::HARD.inventory_bytes as usize + 1];
        limits = ContextLimits::HARD;
        limits.inventory_bytes = 1;
        assert_eq!(
            write_incident_container_with_limits(&inventory, b"", &[], &limits),
            Err(IncidentReason::ResourceHard)
        );
        let assessment = vec![0; ContextLimits::HARD.assessment_bytes as usize + 1];
        assert_eq!(
            write_incident_container_with_limits(b"", &assessment, &[], &limits),
            Err(IncidentReason::ResourceHard)
        );
    }

    #[test]
    fn physical_member_limit_counts_unique_objects() {
        let data: Vec<[u8; 2]> = (0u16..127).map(u16::to_le_bytes).collect();
        let objects: Vec<&[u8]> = data.iter().map(|x| x.as_slice()).collect();
        let accepted = write_incident_container(b"", b"", &objects[..126]).unwrap();
        assert_eq!(
            super::super::read_incident_container(&accepted)
                .unwrap()
                .objects
                .len(),
            126
        );
        assert_eq!(
            write_incident_container(b"", b"", &objects),
            Err(IncidentReason::ResourceHard)
        );
    }

    #[test]
    fn outer_limit_accumulates_unique_objects_and_includes_eof() {
        let mut limits = ContextLimits::HARD;
        limits.outer_bytes = 3072;
        assert_eq!(
            write_incident_container_with_limits(b"", b"", &[b"a", b"a"], &limits)
                .unwrap()
                .len(),
            3072
        );
        assert_eq!(
            write_incident_container_with_limits(b"", b"", &[b"a", b"b"], &limits),
            Err(IncidentReason::ResourceSelected)
        );
        limits.outer_bytes = 4096;
        assert_eq!(
            write_incident_container_with_limits(b"", b"", &[b"a", b"b"], &limits)
                .unwrap()
                .len(),
            4096
        );
        limits.outer_bytes = 2048;
        assert_eq!(
            write_incident_container_with_limits(b"", b"", &[], &limits)
                .unwrap()
                .len(),
            2048
        );
        limits.outer_bytes -= 1;
        assert_eq!(
            write_incident_container_with_limits(b"", b"", &[], &limits),
            Err(IncidentReason::ResourceSelected)
        );
    }

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn member_framing_arithmetic_refuses_overflow() {
        assert_eq!(member_size(usize::MAX), Err(IncidentReason::ResourceHard));
        assert_eq!(
            member_size(usize::MAX - 511),
            Err(IncidentReason::ResourceHard)
        );
        assert_eq!(member_size(0), Ok(512));
        assert_eq!(member_size(512), Ok(1024));
        assert_eq!(member_size(513), Ok(1536));
    }
}
