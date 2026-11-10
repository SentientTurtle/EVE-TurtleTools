use std::io::Write;
use std::collections::HashSet;
use std::error::Error;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::PathBuf;
use zip::{ZipArchive, ZipWriter};
use zip::read::ZipFile;
use zip::result::ZipError;
use zip::write::FileOptions;

fn data_equals<R1: Read, R2: Read>(r1: ZipFile<R1>, r2: ZipFile<R2>) -> Result<bool, Box<dyn Error>> {
    // CRC32's are fairly collision-prone at the scale we're working; ~70,000 items. So we only use them to prove _inequality_, and test equality properly below
    if r1.crc32() != r2.crc32() || r1.size() != r2.size() { return Ok(false); }

    let mut data_equal = true;
    for (left, right) in Iterator::zip(BufReader::new(r1).bytes(), BufReader::new(r2).bytes()) {    // iter::zip is safe here as we have already length-checked the files
        data_equal = left? == right?;
        if !data_equal { break; }
    }
    Ok(data_equal)
}

pub fn build_patch(from: &PathBuf, to: &PathBuf, out: &PathBuf) -> Result<(), Box<dyn Error>> {
    let mut from = ZipArchive::new(File::open(from)?)?;
    let mut to = ZipArchive::new(File::open(to)?)?;
    let mut out = ZipWriter::new(File::create(out)?);

    let mut seen_files = HashSet::new();
    for idx in 0..to.len() {
        let mut new_file = to.by_index(idx).expect("we iterate over len, so index access must be valid");

        seen_files.insert(Box::<[u8]>::from(new_file.name_raw()));

        let file_added = new_file.name()?.ends_with(".json") ||
            match from.by_name_raw(new_file.name_raw()) {
                Ok(old_file) => {
                    let data_eq = data_equals(new_file, old_file)?;
                    // Reinstate new_file as it has just been consumed by data_equals reading the data, and we may need it later
                    new_file = to.by_index(idx).expect("we iterate over len, so index access must be valid");
                    // If data is NOT equal, file was updated and should be added to patch file
                    !data_eq
                },
                Err(ZipError::FileNotFound) => true,
                Err(err) => return Err(err)?,
            };

        if file_added {
            out.raw_copy_file(new_file)?;
        }
    }

    // Always include DELETED.txt, even if it's empty, so users will see that it exists even if the patch file they download has no deleted files
    out.start_file("DELETED.txt", FileOptions::<()>::default())?;
    for idx in 0..from.len() {
        let old_file = to.by_index(idx).expect("we iterate over len, so index access must be valid");
        if !seen_files.contains(old_file.name_raw()) {
            writeln!(out, "{}", old_file.name()?)?;
        }
    }

    Ok(())
}