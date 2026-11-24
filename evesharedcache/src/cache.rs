use std::{fs, io};
use std::collections::{HashMap, HashSet};
use std::collections::hash_map::Keys;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{Duration};
use md5::{Digest, Md5};
use serde::Deserialize;

#[derive(Debug)]
pub enum CacheError {
    /// [`CacheDownloader`] may not be used on the sharedcache directory of a game install; Interacting with game install files must be done through the read-only [`CacheReader`]
    DownloadIntoGameInstall,
    /// [`CacheReader`] may only be used on the sharedcache directory of a game install
    NotGameInstall,
    /// Attempt to use CacheDownloader with a "protected" game server
    GameServerProtected,
    /// Cache index file could not be parsed, usually indicates out-of-date library
    MalformedIndexFile,
    /// HTTP error
    Reqwest(reqwest::Error),
    /// General IO error
    IO(io::Error),
    /// JSON parsing error, usually indicates out-of-date library
    JSON(serde_json::Error),
    /// The requested resource is not known in the sharedcache
    ///
    /// If using [`CacheReader`], ensure the game install is up-to-date and set to "download full game client"
    ResourceNotFound(String),
}

impl Display for CacheError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            CacheError::DownloadIntoGameInstall => write!(f, "CacheDownloader cannot be used on a game install; Use CacheReader instead"),
            CacheError::NotGameInstall => write!(f, "CacheReader must be used on the game install `SharedCache` folder"),
            CacheError::ResourceNotFound(resource) => write!(f, "resource not found: `{}`", resource),
            CacheError::MalformedIndexFile => write!(f, "malformed index file"),
            CacheError::Reqwest(err) => write!(f, "HTTP error: {}", err),
            CacheError::IO(err) => write!(f, "IO error: {}", err),
            CacheError::JSON(err) => write!(f, "JSON parsing error: {}", err),
            CacheError::GameServerProtected => write!(f, "Game Server Protected")
        }
    }
}

impl Error for CacheError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            CacheError::DownloadIntoGameInstall => None,
            CacheError::NotGameInstall => None,
            CacheError::MalformedIndexFile => None,
            CacheError::ResourceNotFound(_) => None,
            CacheError::Reqwest(err) => Some(err),
            CacheError::IO(err) => Some(err),
            CacheError::JSON(err) => Some(err),
            CacheError::GameServerProtected => None
        }
    }
}

impl From<io::Error> for CacheError {
    fn from(value: io::Error) -> Self {
        CacheError::IO(value)
    }
}

impl From<reqwest::Error> for CacheError {
    fn from(value: reqwest::Error) -> Self {
        CacheError::Reqwest(value)
    }
}

impl From<serde_json::Error> for CacheError {
    fn from(value: serde_json::Error) -> Self {
        CacheError::JSON(value)
    }
}

/// Single entry for a file in the sharedcache
#[allow(unused)]
#[derive(Debug, Clone)]
struct IndexEntry {
    path: String,
    md5: String,
    size: u64,
    compressed: u64
}

impl IndexEntry {
    fn load_index(index_text: &str, index: &mut HashMap<String, IndexEntry>) -> Result<(), CacheError> {
        for line in index_text.lines() {
            if line.trim().is_empty() {
                continue;
            }

            // skip 6th field, which are the filesystem permissions
            let mut split = line.splitn(6, ',');
            if let (Some(resource), Some(path), Some(md5), Some(size), Some(compressed)) = (split.next(), split.next(), split.next(), split.next(), split.next()) {
                index.insert(
                    resource.replace('\\', "/").to_ascii_lowercase(),
                    IndexEntry {
                        path: path.to_string(),
                        md5: md5.to_string(),
                        size: u64::from_str(size).map_err(|_| CacheError::MalformedIndexFile)?,
                        compressed: u64::from_str(compressed).map_err(|_| CacheError::MalformedIndexFile)?,
                    }
                );
            } else {
                return Err(CacheError::MalformedIndexFile);
            }
        }

        Ok(())
    }
}

/// Trait to abstract over different SharedCache data sources
/// * [`CacheReader`] provides READ-ONLY access to a locally-installed copy of the game
/// * [`CacheDownloader`]  provides access to the game file CDN, creating a local on-disk cache
pub trait SharedCache {
    /// Retrieves the current game client version
    /// for [`CacheReader`] this is the currently-installed version
    /// for [`CacheDownloader`] this is the version currently on the CDN
    fn client_version(&self) -> &str;
    /// Iterator view on all resources known in this SharedCache
    fn iter_resources(&self) -> impl Iterator<Item=&str>;
    /// Returns true if the resource is available in this SharedCache
    /// for [`CacheReader`] this returns true if a resource is listed in the index file but not yet downloaded by the game launcher
    fn has_resource(&self, resource: &str) -> bool;
    /// Retrieves the bytes of a resource
    ///
    /// for [`CacheDownloader`] downloads if necessary
    fn fetch(&self, resource: &str) -> Result<Vec<u8>, CacheError>;
    /// Retrieves the bytes of a resource, if extant.
    ///
    /// Returns `Ok(None)` for non-extant resources
    ///
    /// for [`CacheDownloader`] downloads if necessary
    fn try_fetch(&self, resource: &str) -> Result<Option<Vec<u8>>, CacheError>;
    /// Retrieves the local-system path of a resource, may be a local or absolute path
    /// for [`CacheDownloader`] downloads if necessary
    fn path_of(&self, resource: &str) -> Result<PathBuf, CacheError>;
    /// Retrieves the md5 hash of a resource
    /// Downloading the file is not necessary
    fn hash_of(&self, resource: &str) -> Result<&str, CacheError>;
}

/// Provides READ-ONLY access to a locally-installed copy of the game
pub struct CacheReader {
    res_dir: PathBuf,
    client_version: String,
    index: HashMap<String, IndexEntry>
}

impl CacheReader {
    /// Loads the cache of a game install,
    ///
    /// # Arguments
    ///
    /// * `directory`: Directory to load, must be the `SharedCache` folder of a game install
    ///
    /// returns: Result<CacheReader, CacheError>
    pub fn load<T: Into<PathBuf>>(directory: T) -> Result<CacheReader, CacheError> {
        let cache_dir = directory.into();

        let start_ini = fs::read_to_string(cache_dir.join("tq/start.ini"))?;
        let client_version = start_ini
            .lines()
            .filter(|line| line.starts_with("build = "))
            .next()
            .ok_or(CacheError::NotGameInstall)?
            .strip_prefix("build = ")
            .unwrap();

        let res_dir = cache_dir.join("ResFiles");
        if !fs::exists(&res_dir)? {
            return Err(CacheError::NotGameInstall);
        }

        let mut reader = CacheReader {
            res_dir,
            client_version: client_version.to_string(),
            index: HashMap::new()
        };

        IndexEntry::load_index(&*fs::read_to_string(cache_dir.join("index_tranquility.txt"))?, &mut reader.index)?;
        IndexEntry::load_index(&*String::from_utf8(reader.fetch("app:/resfileindex.txt")?).map_err(io::Error::other)?, &mut reader.index)?;

        Ok(reader)
    }
}

impl SharedCache for CacheReader {
    fn client_version(&self) -> &str {
        &*self.client_version
    }

    fn iter_resources(&self) -> impl Iterator<Item=&str> {
        self.index.keys().map(String::as_str)
    }

    fn has_resource(&self, resource: &str) -> bool {
        self.index.contains_key(&resource.to_ascii_lowercase().replace('\\', "/"))
    }

    fn fetch(&self, resource: &str) -> Result<Vec<u8>, CacheError> {
        let resource = resource.to_ascii_lowercase().replace('\\', "/");
        let path = if let Some(IndexEntry { path, .. }) = self.index.get(&resource) {
            self.res_dir.join(path)
        } else {
            return Err(CacheError::ResourceNotFound(resource));
        };

        if fs::exists(&path)? {
            Ok(fs::read(path)?)
        } else {
            Err(CacheError::ResourceNotFound(resource))
        }
    }

    fn try_fetch(&self, resource: &str) -> Result<Option<Vec<u8>>, CacheError> {
        let resource = resource.to_ascii_lowercase().replace('\\', "/");
        let path = if let Some(IndexEntry { path, .. }) = self.index.get(&resource) {
            self.res_dir.join(path)
        } else {
            return Ok(None);
        };

        if fs::exists(&path)? {
            Ok(Some(fs::read(path)?))
        } else {
            Err(CacheError::ResourceNotFound(resource))
        }
    }

    fn path_of(&self, resource: &str) -> Result<PathBuf, CacheError> {
        let resource = resource.to_ascii_lowercase().replace('\\', "/");
        let path = if let Some(IndexEntry { path, .. }) = self.index.get(&resource) {
            self.res_dir.join(path)
        } else {
            return Err(CacheError::ResourceNotFound(resource));
        };

        if fs::exists(&path)? {
            Ok(path)
        } else {
            Err(CacheError::ResourceNotFound(resource))
        }
    }

    fn hash_of(&self, resource: &str) -> Result<&str, CacheError> {
        let resource = resource.to_ascii_lowercase().replace('\\', "/");
        let IndexEntry { md5, .. } = self.index.get(&resource)
            .ok_or_else(|| CacheError::ResourceNotFound(resource))?;
        Ok(md5)
    }
}

/// Provides access to the game file CDN, creating a local on-disk cache
pub struct CacheDownloader {
    cache_dir: PathBuf,
    http_client: reqwest::blocking::Client,
    client_version: String,
    app_index: HashMap<String, IndexEntry>,
    res_index: HashMap<String, IndexEntry>
}

impl CacheDownloader {
    /// Provides access to the game file CDN, creating a local on-disk cache
    ///
    /// # Arguments
    ///
    /// * `directory`: Directory for local caching of downloaded files, created if not existing
    /// * `use_macos_build`: If true, download macOS build of the game, if false, download Windows files
    /// * `user_agent`: User Agent to use with HTTP requests
    ///
    /// returns: Result<CacheDownloader, CacheError>
    pub fn initialize<T: Into<PathBuf>>(directory: T, use_macos_build: bool, user_agent: &str) -> Result<CacheDownloader, CacheError> {
        let cache_dir = directory.into();
        fs::create_dir_all(&cache_dir)?;
        let http_client = reqwest::blocking::Client::builder().user_agent(format!("{} turtletools:{}/{} +{}", user_agent, crate::CRATE_NAME, crate::CRATE_VERSION, crate::CRATE_REPO)).build()?;

        if fs::exists(cache_dir.join("updater.exe"))? || fs::exists(cache_dir.join("tq"))? {
            return Err(CacheError::DownloadIntoGameInstall);
        }

        #[allow(non_snake_case)]
        #[derive(Deserialize)]
        pub struct ClientVersion {
            buildNumber: String,
            protected: Option<bool>
        }

        let client_version = http_client.get("https://binaries.eveonline.com/eveclient_TQ.json")
            .send()?
            .error_for_status()?
            .json::<ClientVersion>()?;

        if client_version.protected == Some(true) {
            Err(CacheError::GameServerProtected)?;
        }

        let mut downloader = CacheDownloader {
            cache_dir,
            http_client,
            client_version: client_version.buildNumber,
            app_index: HashMap::new(),
            res_index: HashMap::new()
        };

        let file = downloader.cache_dir.join(format!("eveonline_{}.txt", downloader.client_version));

        let url = if use_macos_build {
            format!("https://binaries.eveonline.com/eveonlinemacOS_{}.txt", downloader.client_version)
        } else {
            format!("https://binaries.eveonline.com/eveonline_{}.txt", downloader.client_version)
        };

        IndexEntry::load_index(&*String::from_utf8(downloader.fetch_file(file, url)?).map_err(io::Error::other)?, &mut downloader.app_index)?;
        IndexEntry::load_index(&*String::from_utf8(downloader.fetch("app:/resfileindex.txt")?).map_err(io::Error::other)?, &mut downloader.res_index)?;

        Ok(downloader)
    }

    fn ensure_cached<P: AsRef<Path>, U: reqwest::IntoUrl>(&self, file: P, url: U) -> Result<Option<Vec<u8>>, CacheError> {
        let file = file.as_ref();
        if fs::exists(&file)? {
            Ok(None)
        } else {
            let response = self.http_client.get(url)
                .send()?
                .error_for_status()?;

            let buffer = response.bytes()?;

            if let Some(parent) = file.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(file, &buffer)?;

            Ok(Some(buffer.to_vec()))
        }
    }

    fn fetch_file<P: AsRef<Path>, U: reqwest::IntoUrl>(&self, file: P, url: U) -> Result<Vec<u8>, CacheError> {
        self.ensure_cached(file.as_ref(), url)
            .and_then(|buffer_opt| {
                if let Some(buffer) = buffer_opt {
                    Ok(buffer)
                } else {
                    fs::read(file).map_err(CacheError::from)
                }
            })
    }

    /// Pre-download files into the local directory, performs downloads in a single thread
    ///
    /// # Arguments
    ///
    /// * `max_items`: Maximum amount of items to download
    /// * `sleep`: Time spent waiting between downloads, set to None for no wait
    ///
    /// returns: Result<u64, CacheError>
    pub fn preload(&self, max_items: u64, sleep: Option<Duration>) -> Result<u64, CacheError> {
        let mut downloaded = 0;
        for  IndexEntry { path, .. } in self.res_index.values() {
            if self.ensure_cached(self.cache_dir.join(path), format!("https://resources.eveonline.com/{}", path))?.is_some() { downloaded += 1 };
            if downloaded >= max_items {
                break;
            }

            if let Some(sleep_duration) = sleep {
                std::thread::sleep(sleep_duration);
            }
        }
        Ok(downloaded)
    }

    pub fn validate(&self) -> Result<(usize, usize), CacheError> {
        let mut valid = 0usize;
        let mut invalid = 0usize;
        let path_map = self.res_index.values().chain(self.app_index.values()).map(|entry| (entry.path.as_str(), entry)).collect::<HashMap<&str, &IndexEntry>>();

        for dir in self.cache_dir.read_dir()? {
            let dir = dir?;
            if dir.metadata()?.is_dir() {
                for file in dir.path().read_dir()? {
                    let file = file?;
                    if file.metadata()?.is_file() {
                        let path = file.path();

                        let resource_path = path.strip_prefix(&self.cache_dir)
                            .unwrap_or(&path)
                            .to_string_lossy()
                            .replace('\\', "/");

                        if let Some(index_entry) = path_map.get(&*resource_path) {
                            let mut md5 = Md5::new();

                            // Rustcrypto is being dumb and moved ::Write implementations to a whole separate crate. This isn't NPM >.>
                            struct MD5Writer<'a>(&'a mut Md5);
                            impl Write for MD5Writer<'_> {
                                fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                                    Digest::update(self.0, buf);
                                    Ok(buf.len())
                                }

                                fn flush(&mut self) -> io::Result<()> {
                                    Ok(())
                                }
                            }

                            std::io::copy(&mut BufReader::new(File::open(&path)?), &mut MD5Writer(&mut md5))?;

                            if u128::from_be_bytes(md5.finalize().into()) == u128::from_str_radix(&*index_entry.md5, 16).map_err(|_| CacheError::MalformedIndexFile)? {
                                valid += 1;
                            } else {
                                invalid += 1;
                                fs::remove_file(path)?;
                            }
                        }
                    }
                }
            }
        }

        Ok((valid, invalid))
    }

    /// Remove local directory files not in the current sharedcache index
    ///
    /// Used to clean up files from older versions of the game
    ///
    /// WARNING: Deletes files in the directory this instance of [`CacheDownloader`] has been initialized to, including any not created by this tool
    pub fn purge(&self, keep_files: &[&str]) -> Result<(), io::Error> {
        let mut valid_paths = self.res_index.values()
            .chain(self.app_index.values())
            .map(|entry| &*entry.path)
            .collect::<HashSet<&str>>();
        valid_paths.extend(keep_files);

        let client_index = format!("eveonline_{}.txt", self.client_version);

        for parent_entry in fs::read_dir(&self.cache_dir)? {
            let parent_entry = parent_entry?;
            let parent_dir = parent_entry.file_name();  // Split for ownership
            let parent_name = parent_dir.to_str().unwrap();

            if parent_entry.file_type()?.is_dir() {
                for file_entry in fs::read_dir(parent_entry.path())? {
                    let file_entry = file_entry?;
                    let file_path = format!("{}/{}", parent_name, &file_entry.file_name().to_str().unwrap()).to_ascii_lowercase();
                    if !valid_paths.contains(&*file_path.to_ascii_lowercase()) {
                        fs::remove_file(file_entry.path())?;
                    }
                }
            } else {
                if parent_name != client_index && !valid_paths.contains(&parent_name) {
                    fs::remove_file(parent_entry.path())?;
                }
            }
        }

        Ok(())
    }
}

impl SharedCache for CacheDownloader {
    fn client_version(&self) -> &str {
        &*self.client_version
    }

    fn iter_resources(&self) -> impl Iterator<Item=&str> {
        Keys::chain(self.app_index.keys(), self.res_index.keys()).map(String::as_str)
    }

    fn has_resource(&self, resource: &str) -> bool {
        let resource = resource.to_ascii_lowercase().replace('\\', "/");
        self.app_index.contains_key(&resource) || self.res_index.contains_key(&resource)
    }

    fn fetch(&self, resource: &str) -> Result<Vec<u8>, CacheError> {
        let resource = resource.to_ascii_lowercase().replace('\\', "/");
        if let Some(IndexEntry { path, .. }) = self.app_index.get(&resource) {
            self.fetch_file(self.cache_dir.join(path), format!("https://binaries.eveonline.com/{}", path))
        } else if let Some(IndexEntry { path, ..}) = self.res_index.get(&resource) {
            self.fetch_file(self.cache_dir.join(path), format!("https://resources.eveonline.com/{}", path))
        } else {
            Err(CacheError::ResourceNotFound(resource))
        }
    }

    fn try_fetch(&self, resource: &str) -> Result<Option<Vec<u8>>, CacheError> {
        let resource = resource.to_ascii_lowercase().replace('\\', "/");
        if let Some(IndexEntry { path, .. }) = self.app_index.get(&resource) {
            self.fetch_file(self.cache_dir.join(path), format!("https://binaries.eveonline.com/{}", path)).map(Option::Some)
        } else if let Some(IndexEntry { path, ..}) = self.res_index.get(&resource) {
            self.fetch_file(self.cache_dir.join(path), format!("https://resources.eveonline.com/{}", path)).map(Option::Some)
        } else {
            Ok(None)
        }
    }

    fn path_of(&self, resource: &str) -> Result<PathBuf, CacheError> {
        let resource = resource.to_ascii_lowercase().replace('\\', "/");
        if let Some(IndexEntry { path, .. }) = self.app_index.get(&resource) {
            let path_buf = self.cache_dir.join(path);
            self.ensure_cached(path_buf.as_path(), format!("https://binaries.eveonline.com/{}", path))
                .map(|_| path_buf)
        } else if let Some(IndexEntry { path, ..}) = self.res_index.get(&resource) {
            let path_buf = self.cache_dir.join(path);
            self.ensure_cached(path_buf.as_path(), format!("https://resources.eveonline.com/{}", path))
                .map(|_| path_buf)
        } else {
            Err(CacheError::ResourceNotFound(resource))
        }
    }

    fn hash_of(&self, resource: &str) -> Result<&str, CacheError> {
        let resource = resource.to_ascii_lowercase().replace('\\', "/");
        let IndexEntry { md5, .. } = self.app_index.get(&resource)
            .or_else(|| self.res_index.get(&resource))
            .ok_or_else(|| CacheError::ResourceNotFound(resource.to_string()))?;
        Ok(md5)
    }
}
