use anyhow::Result;
use flate2::write::GzEncoder;
use flate2::{Compression, GzBuilder};
use std::io::Write;
use tar::{Builder, Header};

pub(crate) fn create_deterministic_tar<W: Write>(writer: W) -> Builder<GzEncoder<W>> {
    let encoder = GzBuilder::new()
        .mtime(0)
        .operating_system(255)
        .write(writer, Compression::best());

    let mut tar = Builder::new(encoder);
    tar.mode(tar::HeaderMode::Deterministic);
    tar
}

pub(crate) fn write_entry<T: Write>(tar: &mut Builder<T>, path: &str, data: &[u8]) -> Result<()> {
    let mut header = Header::new_gnu();
    header.set_path(path)?;
    header.set_size(data.len() as u64);
    header.set_mode(0o644);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    header.set_username("assay")?;
    header.set_groupname("assay")?;
    header.set_cksum();

    tar.append(&header, data)?;
    Ok(())
}
