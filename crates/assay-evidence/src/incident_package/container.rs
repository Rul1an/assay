//! Read-only POSIX ustar container reader for Incident Package v1 outer archives.
//!
//! Enforces all outer archive framing, member allowlist, ordering, field formatting,
//! and digest verification rules specified in SPEC-Incident-Package-v1 section 2.

use super::{IncidentReason, IncidentVerifyReport};
use hex::ToHex;
use sha2::{Digest, Sha256};

/// A single named member inside the outer archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerMember {
    /// Member path (e.g. `inventory.json`, `assessment.json`, `objects/<sha256>`).
    pub name: String,
    /// Uncompressed raw payload bytes.
    pub bytes: Vec<u8>,
}

/// Parsed members of an admitted incident package outer archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncidentContainer {
    /// Mandatory first member: `inventory.json`.
    pub inventory: ContainerMember,
    /// Mandatory second member: `assessment.json`.
    pub assessment: ContainerMember,
    /// Zero or more payload objects sorted in ascending path order.
    pub objects: Vec<ContainerMember>,
}

impl IncidentContainer {
    /// Convenience accessor for `inventory.json` raw bytes.
    pub fn inventory_bytes(&self) -> &[u8] {
        &self.inventory.bytes
    }

    /// Convenience accessor for `assessment.json` raw bytes.
    pub fn assessment_bytes(&self) -> &[u8] {
        &self.assessment.bytes
    }

    /// Convenience accessor for parsed objects.
    pub fn objects(&self) -> &[ContainerMember] {
        &self.objects
    }
}

#[allow(clippy::result_large_err)]
fn input_shape_refusal<T>() -> Result<T, IncidentVerifyReport> {
    Err(IncidentVerifyReport::refusal(IncidentReason::InputShape))
}

/// Read and parse an uncompressed POSIX ustar incident package archive from bytes.
///
/// Returns `Ok(IncidentContainer)` if all container admission checks pass,
/// or `Err(IncidentVerifyReport)` with reason `input_shape` upon any structural defect.
#[allow(clippy::result_large_err)]
pub fn read_incident_container(bytes: &[u8]) -> Result<IncidentContainer, IncidentVerifyReport> {
    let mut pos = 0;
    let mut inventory: Option<ContainerMember> = None;
    let mut assessment: Option<ContainerMember> = None;
    let mut objects: Vec<ContainerMember> = Vec::new();
    let mut last_object_path: Option<String> = None;

    while pos < bytes.len() {
        // Need at least 512 bytes for a header block
        if pos + 512 > bytes.len() {
            return input_shape_refusal();
        }

        let header = &bytes[pos..pos + 512];

        // Check if this is a zero block
        if header.iter().all(|&b| b == 0) {
            // Both inventory.json and assessment.json must be present before EOF
            if inventory.is_none() || assessment.is_none() {
                return input_shape_refusal();
            }

            // Exactly two trailing zero blocks terminate the archive, with no suffix.
            // pos points to the first zero block (512 bytes).
            let remaining = &bytes[pos..];
            if remaining.len() != 1024 {
                return input_shape_refusal();
            }
            if !remaining.iter().all(|&b| b == 0) {
                return input_shape_refusal();
            }

            return Ok(IncidentContainer {
                inventory: inventory.expect("checked present"),
                assessment: assessment.expect("checked present"),
                objects,
            });
        }

        // Validate 512-byte ustar header
        // 1. Checksum field (148..156): 6 octal digits, NUL, space
        if !header[148..154].iter().all(|&b| matches!(b, b'0'..=b'7')) {
            return input_shape_refusal();
        }
        if header[154] != 0 || header[155] != b' ' {
            return input_shape_refusal();
        }

        let chksum_str = match std::str::from_utf8(&header[148..154]) {
            Ok(s) => s,
            Err(_) => return input_shape_refusal(),
        };
        let recorded_chksum = match u32::from_str_radix(chksum_str, 8) {
            Ok(v) => v,
            Err(_) => return input_shape_refusal(),
        };

        let mut computed_chksum = 0u32;
        for (i, &b) in header.iter().enumerate() {
            if (148..156).contains(&i) {
                computed_chksum += 0x20;
            } else {
                computed_chksum += b as u32;
            }
        }
        if recorded_chksum != computed_chksum {
            return input_shape_refusal();
        }

        // 2. Fixed octal fields with zero fill and NUL terminator
        // mode 0644 (100..108): "0000644\0"
        if &header[100..108] != b"0000644\0" {
            return input_shape_refusal();
        }
        // uid zero (108..116): "0000000\0"
        if &header[108..116] != b"0000000\0" {
            return input_shape_refusal();
        }
        // gid zero (116..124): "0000000\0"
        if &header[116..124] != b"0000000\0" {
            return input_shape_refusal();
        }
        // mtime zero (136..148): "00000000000\0"
        if &header[136..148] != b"00000000000\0" {
            return input_shape_refusal();
        }

        // 3. typeflag (156): ASCII '0'
        if header[156] != b'0' {
            return input_shape_refusal();
        }

        // 4. magic (257..263): "ustar\0", version (263..265): "00"
        if &header[257..263] != b"ustar\0" || &header[263..265] != b"00" {
            return input_shape_refusal();
        }

        // 5. Unused header bytes must be zero
        // linkname (157..257, 100 bytes)
        if !header[157..257].iter().all(|&b| b == 0) {
            return input_shape_refusal();
        }
        // uname (265..297, 32 bytes)
        if !header[265..297].iter().all(|&b| b == 0) {
            return input_shape_refusal();
        }
        // gname (297..329, 32 bytes)
        if !header[297..329].iter().all(|&b| b == 0) {
            return input_shape_refusal();
        }
        // devmajor (329..337, 8 bytes)
        if !header[329..337].iter().all(|&b| b == 0) {
            return input_shape_refusal();
        }
        // devminor (337..345, 8 bytes)
        if !header[337..345].iter().all(|&b| b == 0) {
            return input_shape_refusal();
        }
        // prefix (345..500, 155 bytes)
        if !header[345..500].iter().all(|&b| b == 0) {
            return input_shape_refusal();
        }
        // pad (500..512, 12 bytes)
        if !header[500..512].iter().all(|&b| b == 0) {
            return input_shape_refusal();
        }

        // 6. size (124..136): 11 octal digits + NUL
        if !header[124..135].iter().all(|&b| matches!(b, b'0'..=b'7')) {
            return input_shape_refusal();
        }
        if header[135] != 0 {
            return input_shape_refusal();
        }
        let size_str = match std::str::from_utf8(&header[124..135]) {
            Ok(s) => s,
            Err(_) => return input_shape_refusal(),
        };
        let size = match usize::from_str_radix(size_str, 8) {
            Ok(s) => s,
            Err(_) => return input_shape_refusal(),
        };

        // 7. Path name (0..100) and member ordering
        let path_name: String;
        if inventory.is_none() {
            // First member must be inventory.json
            if &header[0..14] != b"inventory.json" {
                return input_shape_refusal();
            }
            if !header[14..100].iter().all(|&b| b == 0) {
                return input_shape_refusal();
            }
            path_name = "inventory.json".to_string();
        } else if assessment.is_none() {
            // Second member must be assessment.json
            if &header[0..15] != b"assessment.json" {
                return input_shape_refusal();
            }
            if !header[15..100].iter().all(|&b| b == 0) {
                return input_shape_refusal();
            }
            path_name = "assessment.json".to_string();
        } else {
            // Subsequent members must be objects/<64 lowercase hex sha256>
            if &header[0..8] != b"objects/" {
                return input_shape_refusal();
            }
            // 64 lowercase hex characters
            let hex_slice = &header[8..72];
            if !hex_slice
                .iter()
                .all(|&b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
            {
                return input_shape_refusal();
            }
            // Must have NUL at index 72 and remaining bytes zero
            if header[72] != 0 || !header[73..100].iter().all(|&b| b == 0) {
                return input_shape_refusal();
            }

            let obj_path = match std::str::from_utf8(&header[0..72]) {
                Ok(s) => s.to_string(),
                Err(_) => return input_shape_refusal(),
            };

            // Ascending byte order check
            if let Some(prev) = &last_object_path {
                if obj_path.as_bytes() <= prev.as_bytes() {
                    return input_shape_refusal();
                }
            }
            last_object_path = Some(obj_path.clone());
            path_name = obj_path;
        }

        // Move past 512-byte header
        pos += 512;

        // Calculate 512-byte padded data size
        let padded_size = if size % 512 == 0 {
            size
        } else {
            size + (512 - (size % 512))
        };

        if pos + padded_size > bytes.len() {
            return input_shape_refusal();
        }

        let payload = &bytes[pos..pos + size];
        let padding = &bytes[pos + size..pos + padded_size];

        // Data padding must be all zero
        if !padding.iter().all(|&b| b == 0) {
            return input_shape_refusal();
        }

        // For objects/<sha256>, verify stored bytes hash to the 64 hex digits in path
        if path_name.starts_with("objects/") {
            let mut hasher = Sha256::new();
            hasher.update(payload);
            let actual_digest = hasher.finalize().encode_hex::<String>();
            let expected_digest = &path_name[8..72];
            if actual_digest != expected_digest {
                return input_shape_refusal();
            }
        }

        let member = ContainerMember {
            name: path_name,
            bytes: payload.to_vec(),
        };

        if inventory.is_none() {
            inventory = Some(member);
        } else if assessment.is_none() {
            assessment = Some(member);
        } else {
            objects.push(member);
        }

        pos += padded_size;
    }

    // If loop finishes without encountering trailing zero blocks
    input_shape_refusal()
}
