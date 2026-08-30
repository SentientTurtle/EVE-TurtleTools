use std::error::Error;
use std::fs::File;
use std::time::Instant;
use evestaticdata::sde::load::{SDELoader};

#[allow(unreachable_code)]
pub fn main() -> Result<(), Box<dyn Error>> {
    return Ok(());

    let start = Instant::now();

    let mut loader = SDELoader::new(File::open("./temp/sde.zip")?)?;
    let full = loader.full()?;

    let elapsed = start.elapsed().as_secs_f64();
    println!("Took: {}s", elapsed);

    #[cfg(all(feature = "sde_update", feature="sde_load"))] {
        use std::fs;

        let mut version = evestaticdata::sde::update::update_sde("./temp/sde.zip")?;
        println!("{}", version);
        let full = evestaticdata::sde::load::SDELoader::new(fs::File::open("./temp/sde.zip")?)?.full()?;
        drop(full);

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
