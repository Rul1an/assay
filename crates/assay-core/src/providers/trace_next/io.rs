use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use super::errors;

pub(crate) fn open_reader<P: AsRef<Path>>(path: P) -> anyhow::Result<BufReader<File>> {
    let file =
        File::open(path.as_ref()).map_err(|e| errors::open_trace_file_error(path.as_ref(), &e))?;
    Ok(BufReader::new(file))
}
