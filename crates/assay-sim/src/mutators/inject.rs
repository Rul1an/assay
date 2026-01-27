use super::Mutator;
use anyhow::Result;
use std::io::Cursor;

pub struct InjectFile {
    pub name: String,
    pub content: Vec<u8>,
}

impl Mutator for InjectFile {
    fn mutate(&self, data: &[u8]) -> Result<Vec<u8>> {
        // 1. Decode existing
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(Cursor::new(data)));

        // 2. Rebuild with injection
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        {
            let mut builder = tar::Builder::new(&mut encoder);

            // Copy existing
            for entry in archive.entries()? {
                let mut entry = entry?;
                let mut entry_content = Vec::new();
                std::io::Read::read_to_end(&mut entry, &mut entry_content)?;

                let header = entry.header().clone();
                builder.append(&header, entry_content.as_slice())?;
            }

            // Inject new
            let mut header = tar::Header::new_gnu();
            header.set_path(&self.name)?;
            header.set_size(self.content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append(&header, self.content.as_slice())?;

            builder.finish()?;
        }
        Ok(encoder.finish()?)
    }
}
