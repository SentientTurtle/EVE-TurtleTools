use std::error::Error;


#[allow(unreachable_code)]
pub fn main() -> Result<(), Box<dyn Error>> {
    #[cfg(all(feature = "sde_update", feature="sde_diff"))] {
        use std::ffi::OsString;
        use std::fs;

        let mut version = evestaticdata::sde::update::update_sde("./temp/sde.zip")?;

        // Do not download any SDE files if the most recent diff is present
        let suffix = OsString::from(format!("{}.zip", version.build_number()));
        for entry in fs::read_dir("./temp/diff/out")? {
            let entry = entry?;
            if entry.file_name().as_encoded_bytes().ends_with(suffix.as_encoded_bytes()) {
                return Ok(())
            }
        }

        version.download_sde("./temp/diff/latest.zip")?;
        let mut latest_build_number = version.build_number();

        while version.build_number() > 2960198 {
            version = version.previous()?;
            let prev_build_number = version.build_number();
            version.download_sde("./temp/diff/previous.zip")?;

            let out_path = format!("./temp/diff/out/{}→{}.zip", prev_build_number, latest_build_number);
            if fs::exists(&out_path)? {
                println!("\tDiff already exists: {}", out_path);
                break;
            }
            println!("\tBuilding: {}", out_path);
            evestaticdata::sde::diff::build_diff("./temp/diff/latest.zip", "./temp/diff/previous.zip", out_path)?;

            fs::rename("./temp/diff/previous.zip", "./temp/diff/latest.zip")?;
            latest_build_number = prev_build_number;
        }
    }

    Ok(())
}
