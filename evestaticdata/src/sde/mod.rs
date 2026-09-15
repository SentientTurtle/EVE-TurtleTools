#[cfg(feature = "sde_load")]
pub mod load;
#[cfg(feature= "sde_update")]
pub mod update;

#[cfg(feature = "sde_diff")]
pub mod diff {
    use std::cmp::Ordering;
    use std::error::Error;
    use std::fs::File;
    use std::io;
    use std::io::{BufRead, BufReader, Bytes, Read, Write};
    use std::path::Path;
    use indexmap::IndexSet;
    use json_patch::Patch;
    use serde::Serialize;
    use serde_json::Value;
    use zip::{ZipArchive, ZipWriter};
    use zip::write::FileOptions;

    pub fn build_diff<P1: AsRef<Path>, P2: AsRef<Path>, OP: AsRef<Path>>(current: P1, previous: P2, out: OP) -> Result<(), Box<dyn Error>> {
        let mut current = ZipArchive::new(File::open(current)?)?;
        let mut previous = ZipArchive::new(File::open(previous)?)?;

        let mut out = ZipWriter::new(File::create(out)?);

        let current_names = current.file_names().map(str::to_owned).collect::<IndexSet<_>>();
        let prev_names = previous.file_names().map(str::to_owned).collect::<IndexSet<_>>();

        for filename in current_names.intersection(&prev_names) {
            let mut current_lines = BufReader::new(current.by_name(filename)?).lines();
            let mut prev_lines = BufReader::new(previous.by_name(filename)?).lines();

            let mut c_peek = current_lines.next().transpose()?;
            let mut p_peek = prev_lines.next().transpose()?;

            let mut started_outfile = false;

            loop {
                let (patch_key, patch) = match (&c_peek, &p_peek) {
                    (Some(c_line), Some(p_line)) => {
                        // Optimization stolen from FC Pinky
                        if c_line == p_line {
                            c_peek = current_lines.next().transpose()?;
                            p_peek = prev_lines.next().transpose()?;
                            continue;
                        }

                        let c_value: serde_json::Map<String, Value> = serde_json::from_str(c_line)?;
                        let p_value: serde_json::Map<String, Value> = serde_json::from_str(p_line)?;

                        fn try_comp<'a, 'b>(lhs: Option<&'a Value>, rhs: Option<&'b Value>) -> Option<((&'a Value, &'b Value), Ordering)> {
                            if let (Some(lhs), Some(rhs)) = (lhs, rhs) {
                                if let (Some(l), Some(r)) = (lhs.as_u64(), rhs.as_u64()) {
                                    Some(((lhs, rhs), Ord::cmp(&l, &r)))
                                } else if let (Some(l), Some(r)) = (lhs.as_str(), rhs.as_str()) {
                                    Some(((lhs, rhs), Ord::cmp(&l, &r)))
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        }

                        let ((c_key, p_key), ordering) = try_comp(c_value.get("_key"), p_value.get("_key")).ok_or_else(|| format!("SDE format without _key?! ({})", filename))?;

                        match ordering {
                            Ordering::Equal => {
                                c_peek = current_lines.next().transpose()?;
                                p_peek = prev_lines.next().transpose()?;
                                (c_key.clone(), json_patch::diff(&Value::Object(p_value), &Value::Object(c_value)))
                            }
                            Ordering::Less => {
                                // Current key added
                                c_peek = current_lines.next().transpose()?;
                                (c_key.clone(), json_patch::diff(&Value::Null, &Value::Object(c_value)))
                            }
                            Ordering::Greater => {
                                // Previous key removed
                                p_peek = prev_lines.next().transpose()?;
                                (p_key.clone(), json_patch::diff(&Value::Object(p_value), &Value::Null))
                            }
                        }
                    },
                    (Some(c_line), None) => {
                        // Current key added
                        let c_value: serde_json::Map<String, Value> = serde_json::from_str(&*c_line)?;
                        let c_key = c_value.get("_key").ok_or_else(|| format!("SDE format without _key?! ({})", filename))?;
                        c_peek = current_lines.next().transpose()?;
                        (c_key.clone(), json_patch::diff(&Value::Null, &Value::Object(c_value)))
                    },
                    (None, Some(p_line)) => {
                        // Previous key removed
                        let p_value: serde_json::Map<String, Value> = serde_json::from_str(&*p_line)?;
                        let p_key = p_value.get("_key").ok_or_else(|| format!("SDE format without _key?! ({})", filename))?;
                        p_peek = prev_lines.next().transpose()?;
                        (p_key.clone(), json_patch::diff(&Value::Object(p_value), &Value::Null))
                    },
                    (None, None) => break
                };

                if patch.len() > 0 {
                    if !started_outfile {
                        out.start_file(format!("patch_{}", filename), FileOptions::<()>::default().compression_level(Some(9)))?;
                        started_outfile = true;
                    }

                    #[derive(Serialize)]
                    struct PatchEntry {
                        _key: Value,
                        patch: Patch
                    }

                    serde_json::to_writer(&mut out, &PatchEntry { _key: patch_key, patch })?;
                    out.write_all(b"\n")?;
                }
            }
        }

        'file_loop: for filename in current_names.difference(&prev_names) { // Added files
            let mut current_file = current.by_name(filename)?;

            for i in 0..previous.len() {
                let prev_file = previous.by_index(i)?;
                if current_file.crc32() == prev_file.crc32() && current_file.size() == prev_file.size() {
                    let files_identical = Bytes::zip(current_file.bytes(), prev_file.bytes())
                        .map(|(l, r)| {
                            match (l, r) {
                                (Ok(lb), Ok(rb)) => Ok(lb == rb),
                                (Err(err), _) | (Ok(_), Err(err)) => Err(err)
                            }
                        })
                        .fold(Ok(true), |acc, value| {
                            acc.and_then(|lhs| value.map(|rhs| lhs && rhs))
                        })?;

                    if files_identical {
                        let filename_old = previous.name_for_index(i).expect("index i has already been accessed, and must exist at this point");
                        if filename.contains('\'') || filename_old.contains('\'') { return Err(format!("Renamed file with apostrophe in name, cannot rename from [{}] to [{}]", filename_old, filename))? }

                        out.start_file(format!("rename_'{}'_'{}'", filename_old, filename), FileOptions::<()>::default())?;

                        continue 'file_loop;
                    } else {
                        // Reset cursor position for current_file
                        current_file = current.by_name(filename)?;
                    }
                }
            }

            out.start_file(format!("add_{}", filename), FileOptions::<()>::default().compression_level(Some(9)))?;
            io::copy(&mut current_file, &mut out)?;
        }
        for filename in prev_names.difference(&current_names) { // Removed files
            out.start_file(format!("remove_{}", filename), FileOptions::<()>::default())?;
        }

        out.finish()?;

        Ok(())
    }
}

#[allow(non_snake_case)]
#[cfg(all(feature="export_sqlite", feature="sde_load"))]
pub mod sqlite;

#[cfg(feature = "npc_data")]
pub mod npc;