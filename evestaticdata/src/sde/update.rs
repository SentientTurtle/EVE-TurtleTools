#![allow(non_snake_case, non_camel_case_types)] // Use of serialized types, whose names match the output fields

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::Path;
use std::{fs, io};
use std::fmt::{Display, Formatter};

pub const VERSION_URL: &'static str = "https://developers.eveonline.com/static-data/tranquility/latest.jsonl";
pub const SDE_URL: &'static str = "https://developers.eveonline.com/static-data/eve-online-static-data-latest-jsonl.zip";

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "_key")]
pub enum SdeVersion {
    sde { buildNumber: u32, releaseDate: Option<String> }
}

impl SdeVersion {
    pub fn from_sde_zip<P: AsRef<Path>>(path: P) -> Result<SdeVersion, io::Error> {
        let mut archive = zip::ZipArchive::new(File::open(path)?).map_err(io::Error::other)?;
        serde_json::from_reader(archive.by_name("_sde.jsonl").map_err(io::Error::other)?).map_err(io::Error::other)
    }

    pub fn try_sde_zip<P: AsRef<Path>>(path: P) -> Result<SdeVersion, io::Error> {
        if fs::exists(&path)? {
            #[allow(unused_qualifications)]
            Self::from_sde_zip(path)
        } else {
            Ok(SdeVersion::sde { buildNumber: 0, releaseDate: None })
        }
    }

    pub fn build_number(&self) -> u32 {
        let SdeVersion::sde { buildNumber, .. } = self;
        *buildNumber
    }

    pub fn release_date(&self) -> Option<&str> {
        let SdeVersion::sde { releaseDate, .. } = self;
        releaseDate.as_ref().map(String::as_str)
    }

    pub fn fetch_latest() -> Result<SdeVersion, io::Error> {
        reqwest::blocking::get(VERSION_URL).map_err(io::Error::other)?
            .json::<SdeVersion>().map_err(io::Error::other)
    }

    pub fn sde_url(&self) -> String {
        let SdeVersion::sde { buildNumber, .. } = self;
        format!("https://developers.eveonline.com/static-data/tranquility/eve-online-static-data-{}-jsonl.zip", buildNumber)
    }

    pub fn changelog_url(&self) -> String {
        let SdeVersion::sde { buildNumber, .. } = self;
        format!("https://developers.eveonline.com/static-data/tranquility/changes/{}.jsonl", buildNumber)
    }

    pub fn changelog(&self) -> Result<ChangeLog, io::Error> {
        let body = reqwest::blocking::get(self.changelog_url()).map_err(io::Error::other)?.error_for_status().map_err(io::Error::other)?.text().map_err(io::Error::other)?;

        let mut lines = body.lines();
        let mut change_log = serde_json::from_str::<ChangeLog>(lines.next().unwrap_or("")).map_err(io::Error::other)?;

        for line in lines {
            change_log.changes.push(serde_json::from_str(line).map_err(io::Error::other)?);
        }

        Ok(change_log)
    }

    pub fn previous(&self) -> Result<SdeVersion, io::Error> {
        let body = reqwest::blocking::get(self.changelog_url()).map_err(io::Error::other)?.error_for_status().map_err(io::Error::other)?.text().map_err(io::Error::other)?;

        Ok(SdeVersion::sde {
            buildNumber: serde_json::from_str::<ChangeLog>(body.lines().next().unwrap_or("")).map_err(io::Error::other)?.lastBuildNumber,
            releaseDate: None
        })
    }

    pub fn download_sde<P: AsRef<Path>>(&self, file: P) -> Result<SdeVersion, io::Error> {
        reqwest::blocking::get(self.sde_url()).map_err(io::Error::other)?
            .copy_to(&mut File::create(&file)?).map(|_| ()).map_err(io::Error::other)?;

        SdeVersion::from_sde_zip(file)
    }
}

impl Display for SdeVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let SdeVersion::sde { buildNumber, releaseDate } = self;

        if let Some(date) = releaseDate {
            write!(f, "{} ({})", buildNumber, date)
        } else {
            write!(f, "{}", buildNumber)
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChangeLog {
    pub buildNumber: u32,
    pub lastBuildNumber: u32,
    pub releaseDate: String,
    #[serde(skip)]
    pub changes: Vec<ChangeLogEntry>
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChangeLogEntry {
    #[serde(rename="_key")]
    file: String,
    #[serde(default)]
    changed: Vec<u32>,
    #[serde(default)]
    added: Vec<u32>,
    #[serde(default)]
    removed: Vec<u32>,
}

pub fn download_latest_sde<P: AsRef<Path>>(file: P) -> Result<SdeVersion, io::Error> {
    reqwest::blocking::get(SDE_URL).map_err(io::Error::other)?
        .copy_to(&mut File::create(&file)?).map(|_| ()).map_err(io::Error::other)?;

    SdeVersion::from_sde_zip(file)
}

pub fn update_sde<P: AsRef<Path>>(file: P) -> Result<SdeVersion, io::Error> {
    let current @ SdeVersion::sde { buildNumber: current_version, .. } = SdeVersion::try_sde_zip(&file)?;
    let SdeVersion::sde { buildNumber: latest, .. } = SdeVersion::fetch_latest()?;
    if current_version < latest {
        download_latest_sde(file)
    } else {
        Ok(current)
    }
}
