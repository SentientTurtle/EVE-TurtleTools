use evesharedcache::cache::{CacheError, SharedCache};
use evestaticdata::sde::load::{SDELoadError, SDELoader, TypeList};
use evestaticdata::types::{ids, values};
use image::imageops::FilterType;
use image::{imageops, DynamicImage, GrayAlphaImage, ImageFormat, ImageReader, LumaA};
use image_blend::BufferBlend;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::io::{BufRead, BufReader, ErrorKind};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::{io, mem};
use std::borrow::Cow;
use std::sync::{LazyLock, Mutex};
use fs_err as fs;
use fs_err::File;
use regex::Regex;
use zip::write::FileOptions;
use zip::{CompressionMethod, ZipWriter};

// Industry "reaction" blueprints use a different background
const REACTION_GROUPS: [u32; 4] = [1888, 1889, 1890, 4097];

pub mod hash {
    //! Not used for security, using md5 as it's good enough and has consistency with EVE Online's sharedcache.
    //! Actual hash algorithm subject to future change.

    #[allow(private_bounds)]
    pub fn index_key<T: HashTuple>(ext: &'static str, items: T) -> String {
        let mut context = md5::Context::new();
        items.hash_all(|bytes| context.consume(bytes));
        format!("{:X}.{}", context.finalize(), ext)
    }

    trait IndexHash: Copy {
        type Output: AsRef<[u8]>;

        fn cache_bytes(&self) -> Self::Output;
    }

    impl<'a> IndexHash for &'a str {
        type Output = &'a [u8];

        fn cache_bytes(&self) -> Self::Output {
            self.as_bytes()
        }
    }

    impl<'a> IndexHash for Option<&'a str> {
        type Output = &'a [u8];

        fn cache_bytes(&self) -> Self::Output {
            self.map(str::as_bytes).unwrap_or(b"")
        }
    }

    trait HashTuple: Copy {
        fn hash_all<F: FnMut(&[u8])>(self, consumer: F);
    }

    impl<A: IndexHash> HashTuple for A {
        fn hash_all<F: FnMut(&[u8])>(self, mut consumer: F) {
            consumer(self.cache_bytes().as_ref());
        }
    }

    impl<A: IndexHash> HashTuple for (A,) {
        fn hash_all<F: FnMut(&[u8])>(self, mut consumer: F) {
            consumer(self.0.cache_bytes().as_ref());
        }
    }

    impl<A: IndexHash, B: IndexHash> HashTuple for (A, B) {
        fn hash_all<F: FnMut(&[u8])>(self, mut consumer: F) {
            consumer(self.0.cache_bytes().as_ref());
            consumer(self.1.cache_bytes().as_ref());
        }
    }

    impl<A: IndexHash, B: IndexHash, C: IndexHash> HashTuple for (A, B, C) {
        fn hash_all<F: FnMut(&[u8])>(self, mut consumer: F) {
            consumer(self.0.cache_bytes().as_ref());
            consumer(self.1.cache_bytes().as_ref());
            consumer(self.2.cache_bytes().as_ref());
        }
    }

    impl<A: IndexHash, B: IndexHash, C: IndexHash, D: IndexHash> HashTuple for (A, B, C, D) {
        fn hash_all<F: FnMut(&[u8])>(self, mut consumer: F) {
            consumer(self.0.cache_bytes().as_ref());
            consumer(self.1.cache_bytes().as_ref());
            consumer(self.2.cache_bytes().as_ref());
            consumer(self.3.cache_bytes().as_ref());
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct IconConfig {
    pub external_overlays: bool,
    pub use_old_overlays: bool,
    pub module_overlays: bool,
    pub clone_overlays: bool,
    pub image_format: IconImageFormat
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum IconImageFormat {
    Native,
    #[cfg(feature = "webp")]
    WEBP(Option<u32>)
}

impl IconImageFormat {
    pub fn help_text(&self) -> &'static str {
        match self {
            IconImageFormat::Native => "As in game files",
            #[cfg(feature = "webp")]
            IconImageFormat::WEBP(None) => "WEBP (Lossless)",
            #[cfg(feature = "webp")]
            IconImageFormat::WEBP(Some(_)) => "WEBP (Lossy)",
        }
    }

    pub fn extension(&self) -> Option<&'static str> {
        match self {
            IconImageFormat::Native => None,
            #[cfg(feature = "webp")]
            IconImageFormat::WEBP(_) => Some("webp"),
        }
    }

    pub fn write_cacheimage<P: AsRef<Path>, W: Write>(&self, cache_file: P, out: &mut W) -> Result<(), IconError>{
        #[allow(irrefutable_let_patterns)]  // Other patterns are enabled with cargo features
        if let IconImageFormat::Native = self {
            io::copy(&mut File::open(cache_file.as_ref())?, out)?;
            Ok(())
        } else {
            self.write_dyimage(
                ImageReader::open(cache_file)?.with_guessed_format()?.decode()?,
                out,
                |_, _| unreachable!()
            )
        }
    }

    pub fn write_dyimage<W: Write, F: Fn(DynamicImage, &mut W) -> Result<(), IconError>>(&self, image: DynamicImage, out: &mut W, native: F) -> Result<(), IconError>{
        match self {
            IconImageFormat::Native => native(image, out),
            #[cfg(feature = "webp")]
            IconImageFormat::WEBP(quality) => {
                // `webp` crate only supports RGBA8 images
                let image = DynamicImage::ImageRgba8(image.into_rgba8());
                let encoder = webp::Encoder::from_image(&image).expect("dynamic image is converted to rgba before exporting to webp");
                if let Some(quality) = quality {
                    let mut config = webp::WebPConfig::new().unwrap();
                    config.method = 4;
                    config.quality = *quality as f32;

                    io::copy(&mut &*encoder.encode_advanced(&config).map_err(|e| format!("{:?}", e)).map_err(io::Error::other)?, out)?;
                } else {
                    io::copy(&mut &*encoder.encode_lossless(), out)?;
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug)]
pub struct TypeInfo {
    pub group_id: ids::GroupID,
    pub category_id: ids::CategoryID,
    pub icon_id: Option<ids::IconID>,
    pub graphic_id: Option<ids::GraphicID>,
    pub meta_group_id: Option<ids::MetaGroupID>,
    pub is_renderable: bool,
    pub module_slot: Option<ModuleSlot>,
    pub omega_required: Option<bool>,
    pub blueprint_output_metagroup: Option<ids::MetaGroupID>
}

#[derive(Debug)]
pub struct GraphicInfo {
    pub folder: Option<String>,
    pub hull: Option<String>
}

static OVERLAY_CACHE: LazyLock<Mutex<HashMap<&'static str, &'static DynamicImage>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

// Overlays are declared static so their metadata exists only once in the binary. ("::Bytes" overlays include the image data and mustn't be duplicated)
static NEW_OVERLAY_T1: IconOverlay = IconOverlay::Resource { resource: "res:/ui/texture/icons/73_16_241.png", identifier: "meta-t1" };
static NEW_OVERLAY_T2: IconOverlay = IconOverlay::Resource { resource: "res:/ui/texture/icons/73_16_242.png", identifier: "meta-t2" };
static NEW_OVERLAY_STORYLINE: IconOverlay = IconOverlay::Resource { resource: "res:/ui/texture/icons/73_16_245.png", identifier: "meta-storyline" };
static NEW_OVERLAY_FACTION: IconOverlay = IconOverlay::Resource { resource: "res:/ui/texture/icons/73_16_246.png", identifier: "meta-faction" };
static NEW_OVERLAY_OFFICER: IconOverlay = IconOverlay::Resource { resource: "res:/ui/texture/icons/73_16_248.png", identifier: "meta-officer" };
static NEW_OVERLAY_DEADSPACE: IconOverlay = IconOverlay::Resource { resource: "res:/ui/texture/icons/73_16_247.png", identifier: "meta-deadspace" };
static NEW_OVERLAY_T3: IconOverlay = IconOverlay::Resource { resource: "res:/ui/texture/icons/73_16_243.png", identifier: "meta-t3" };
static NEW_OVERLAY_ABYSSAL: IconOverlay = IconOverlay::Resource { resource: "res:/ui/texture/icons/itemoverlay/abyssal.png", identifier: "meta-abyssal" };
static NEW_OVERLAY_NES: IconOverlay = IconOverlay::Resource { resource: "res:/ui/texture/icons/itemoverlay/nes.png", identifier: "meta-nes" };
static NEW_OVERLAY_TIMELIMITED: IconOverlay = IconOverlay::Resource { resource: "res:/ui/texture/icons/itemoverlay/timelimited.png", identifier: "meta-timelimited" };
static NEW_OVERLAY_STRUCTURE_FACTION: IconOverlay = IconOverlay::Resource { resource: "res:/ui/texture/shared/structureoverlayfaction.png", identifier: "meta-structure-faction" };
static NEW_OVERLAY_STRUCTURE_T2: IconOverlay = IconOverlay::Resource { resource: "res:/ui/texture/shared/structureoverlayt2.png", identifier: "meta-structure-t2" };
static NEW_OVERLAY_STRUCTURE_T1: IconOverlay = IconOverlay::Resource { resource: "res:/ui/texture/shared/structureoverlay.png", identifier: "meta-structure-t1" };
static NEW_OVERLAY_ACADEMY: IconOverlay = IconOverlay::Resource { resource: "res:/ui/texture/icons/itemoverlay/academy_ship_16px.png", identifier: "meta-academy" };
static NEW_OVERLAY_ABERRANT: IconOverlay = IconOverlay::Resource { resource: "res:/ui/texture/icons/itemoverlay/aberrant.png", identifier: "meta-aberrant" };
static NEW_OVERLAY_MODULE_HIGH: IconOverlay = IconOverlay::Resource { resource: "res:/ui/texture/icons/38_16_123.png", identifier: "module-high" };
static NEW_OVERLAY_MODULE_MEDIUM: IconOverlay = IconOverlay::Resource { resource: "res:/ui/texture/icons/38_16_122.png", identifier: "module-medium" };
static NEW_OVERLAY_MODULE_LOW: IconOverlay = IconOverlay::Resource { resource: "res:/ui/texture/icons/38_16_121.png", identifier: "module-low" };
static NEW_OVERLAY_MODULE_RIG: IconOverlay = IconOverlay::Resource { resource: "res:/ui/texture/icons/38_16_124.png", identifier: "module-rig" };
static NEW_OVERLAY_MODULE_SUBSYSTEM: IconOverlay = IconOverlay::Resource { resource: "res:/ui/texture/icons/38_16_42.png", identifier: "module-subsystem" };
static NEW_OVERLAY_CLONE_ALPHA: IconOverlay = IconOverlay::Bytes { image: include_bytes!("rsc/overlay/new/Alpha.png"), cachekey: "alpha-new", identifier: "clone-alpha" };
static NEW_OVERLAY_CLONE_OMEGA: IconOverlay = IconOverlay::Bytes { image: include_bytes!("rsc/overlay/new/Omega.png"), cachekey: "omega-new", identifier: "clone-omega" };

static OLD_OVERLAY_T1: IconOverlay = IconOverlay::Bytes { image: include_bytes!("rsc/overlay/old/Tech 1.png"), cachekey: "t1-old", identifier: "meta-t1" };
static OLD_OVERLAY_T2: IconOverlay = IconOverlay::Bytes { image: include_bytes!("rsc/overlay/old/Tech 2.png"), cachekey: "t2-old", identifier: "meta-t2" };
static OLD_OVERLAY_STORYLINE: IconOverlay = IconOverlay::Bytes { image: include_bytes!("rsc/overlay/old/Storyline.png"), cachekey: "storyline-old", identifier: "meta-storyline" };
static OLD_OVERLAY_FACTION: IconOverlay = IconOverlay::Bytes { image: include_bytes!("rsc/overlay/old/Faction.png"), cachekey: "faction-old", identifier: "meta-faction" };
static OLD_OVERLAY_OFFICER: IconOverlay = IconOverlay::Bytes { image: include_bytes!("rsc/overlay/old/Officer.png"), cachekey: "officer-old", identifier: "meta-officer" };
static OLD_OVERLAY_DEADSPACE: IconOverlay = IconOverlay::Bytes { image: include_bytes!("rsc/overlay/old/Deadspace.png"), cachekey: "deadspace-old", identifier: "meta-deadspace" };
static OLD_OVERLAY_T3: IconOverlay = IconOverlay::Bytes { image: include_bytes!("rsc/overlay/old/Tech 3.png"), cachekey: "t3-old", identifier: "meta-t3" };
static OLD_OVERLAY_ABYSSAL: IconOverlay = IconOverlay::Bytes { image: include_bytes!("rsc/overlay/old/Abyssal.png"), cachekey: "abyssal-old", identifier: "meta-abyssal" };
static OLD_OVERLAY_NES: IconOverlay = IconOverlay::Bytes { image: include_bytes!("rsc/overlay/old/NES.png"), cachekey: "nes-old", identifier: "meta-nes" };
static OLD_OVERLAY_TIMELIMITED: IconOverlay = IconOverlay::Bytes { image: include_bytes!("rsc/overlay/old/Time Limited.png"), cachekey: "timelimited-old", identifier: "meta-timelimited" };
static OLD_OVERLAY_STRUCTURE_FACTION: IconOverlay = IconOverlay::Bytes { image: include_bytes!("rsc/overlay/old/Structure Faction.png"), cachekey: "structurefaction-old", identifier: "meta-structure-faction" };
static OLD_OVERLAY_STRUCTURE_T2: IconOverlay = IconOverlay::Bytes { image: include_bytes!("rsc/overlay/old/Structure Tech 2.png"), cachekey: "structuret2-old", identifier: "meta-structure-t2" };
static OLD_OVERLAY_STRUCTURE_T1: IconOverlay = IconOverlay::Bytes { image: include_bytes!("rsc/overlay/old/Structure Tech 1.png"), cachekey: "structuret1-old", identifier: "meta-structure-t1" };
static OLD_OVERLAY_ACADEMY: IconOverlay = IconOverlay::Bytes { image: include_bytes!("rsc/overlay/old/Academy.png"), cachekey: "academy-old", identifier: "meta-academy" };
static OLD_OVERLAY_ABERRANT: IconOverlay = IconOverlay::Bytes { image: include_bytes!("rsc/overlay/old/Aberrant.png"), cachekey: "aberrant-old", identifier: "meta-aberrant" };
const OLD_OVERLAY_MODULE_HIGH: IconOverlay = IconOverlay::Bytes { image: include_bytes!("rsc/overlay/old/Slot-High.png"), cachekey: "slot-high-old", identifier: "module-high" };
const OLD_OVERLAY_MODULE_MEDIUM: IconOverlay = IconOverlay::Bytes { image: include_bytes!("rsc/overlay/old/Slot-Med.png"), cachekey: "slot-med-old", identifier: "module-medium" };
const OLD_OVERLAY_MODULE_LOW: IconOverlay = IconOverlay::Bytes { image: include_bytes!("rsc/overlay/old/Slot-Low.png"), cachekey: "slot-low-old", identifier: "module-low" };
const OLD_OVERLAY_MODULE_RIG: IconOverlay = IconOverlay::Bytes { image: include_bytes!("rsc/overlay/old/Slot-Rig.png"), cachekey: "slot-rig-old", identifier: "module-rig" };
const OLD_OVERLAY_MODULE_SUBSYSTEM: IconOverlay = IconOverlay::Bytes { image: include_bytes!("rsc/overlay/old/Slot-Subsystem.png"), cachekey: "slot-subsystem-old", identifier: "module-subsystem" };
const OLD_OVERLAY_CLONE_OMEGA: IconOverlay = IconOverlay::Bytes { image: include_bytes!("rsc/overlay/old/Omega.png"), cachekey: "omega-old", identifier: "clone-omega" };
const OLD_OVERLAY_CLONE_ALPHA: IconOverlay = IconOverlay::Bytes { image: include_bytes!("rsc/overlay/old/Alpha.png"), cachekey: "alpha-old", identifier: "clone-alpha" };

static NEW_OVERLAY_LIST: &[&IconOverlay] = &[
    &NEW_OVERLAY_T1,
    &NEW_OVERLAY_T2,
    &NEW_OVERLAY_STORYLINE,
    &NEW_OVERLAY_FACTION,
    &NEW_OVERLAY_OFFICER,
    &NEW_OVERLAY_DEADSPACE,
    &NEW_OVERLAY_T3,
    &NEW_OVERLAY_ABYSSAL,
    &NEW_OVERLAY_NES,
    &NEW_OVERLAY_TIMELIMITED,
    &NEW_OVERLAY_STRUCTURE_FACTION,
    &NEW_OVERLAY_STRUCTURE_T2,
    &NEW_OVERLAY_STRUCTURE_T1,
    &NEW_OVERLAY_ACADEMY,
    &NEW_OVERLAY_ABERRANT,
    &NEW_OVERLAY_MODULE_HIGH,
    &NEW_OVERLAY_MODULE_MEDIUM,
    &NEW_OVERLAY_MODULE_LOW,
    &NEW_OVERLAY_MODULE_RIG,
    &NEW_OVERLAY_MODULE_SUBSYSTEM,
    &NEW_OVERLAY_CLONE_ALPHA,
    &NEW_OVERLAY_CLONE_OMEGA
];
static OLD_OVERLAY_LIST: &[&IconOverlay] = &[
    &OLD_OVERLAY_T1,
    &OLD_OVERLAY_T2,
    &OLD_OVERLAY_STORYLINE,
    &OLD_OVERLAY_FACTION,
    &OLD_OVERLAY_OFFICER,
    &OLD_OVERLAY_DEADSPACE,
    &OLD_OVERLAY_T3,
    &OLD_OVERLAY_ABYSSAL,
    &OLD_OVERLAY_NES,
    &OLD_OVERLAY_TIMELIMITED,
    &OLD_OVERLAY_STRUCTURE_FACTION,
    &OLD_OVERLAY_STRUCTURE_T2,
    &OLD_OVERLAY_STRUCTURE_T1,
    &OLD_OVERLAY_ACADEMY,
    &OLD_OVERLAY_ABERRANT,
    &OLD_OVERLAY_MODULE_HIGH,
    &OLD_OVERLAY_MODULE_MEDIUM,
    &OLD_OVERLAY_MODULE_LOW,
    &OLD_OVERLAY_MODULE_RIG,
    &OLD_OVERLAY_MODULE_SUBSYSTEM,
    &OLD_OVERLAY_CLONE_ALPHA,
    &OLD_OVERLAY_CLONE_OMEGA
];

const BPO_PATCH: IconOverlay = IconOverlay::Bytes {
    image: include_bytes!("rsc/patch/BPO.png"),
    cachekey: "patch-BPO",
    identifier: "patch-BPO",
};
const BPC_PATCH: IconOverlay = IconOverlay::Bytes {
    image: include_bytes!("rsc/patch/BPC.png"),
    cachekey: "patch-BPC",
    identifier: "patch-BPC",
};

#[derive(Copy, Clone)]
pub enum IconOverlay {  // TODO: Cache parsed images
    None,
    Resource { resource: &'static str, identifier: &'static str },
    Bytes { image: &'static [u8], cachekey: &'static str, identifier: &'static str }
}

impl IconOverlay {
    pub fn retain_if(self, retain: bool) -> Self {
        if retain {
            self
        } else {
            Self::None
        }
    }

    pub fn load<C: SharedCache>(self, cache: &C) -> Result<Option<(&str, &'static DynamicImage)>, IconError> {
        match self {
            IconOverlay::None => Ok(None),
            IconOverlay::Resource { resource, identifier } => {
                let mut overlay_cache = OVERLAY_CACHE.lock().expect("Unexpected panic in overlay loading");
                if let Some(overlay) = overlay_cache.get(identifier) {
                    Ok(Some((cache.hash_of(resource)?, overlay)))
                } else {
                    // The overlays are small, reused often, and loaded once, so throw them onto the heap with Box::leak() for caching
                    let overlay = Box::leak(Box::new(
                        ImageReader::open(cache.path_of(resource)?)?.with_guessed_format()?.decode()?.resize_exact(16, 16, FilterType::Lanczos3)
                    ));
                    overlay_cache.insert(identifier, overlay);
                    Ok(Some((cache.hash_of(resource)?, overlay)))
                }
            }
            IconOverlay::Bytes { image: bytes, cachekey, identifier } => {
                let mut overlay_cache = OVERLAY_CACHE.lock().expect("Unexpected panic in overlay loading");

                if let Some(overlay) = overlay_cache.get(identifier) {
                    Ok(Some((cachekey, overlay)))
                } else {
                    // The overlays are small, reused often, and loaded once, so throw them onto the heap with Box::leak() for caching
                    let overlay = Box::leak(Box::new({
                        let mut reader = ImageReader::new(Cursor::new(bytes));
                        reader.set_format(ImageFormat::Png);
                        reader.decode()?.resize_exact(16, 16, FilterType::Lanczos3)
                    }));
                    overlay_cache.insert(identifier, overlay);
                    Ok(Some((cachekey, overlay)))
                }
            }
        }
    }

    pub fn identifier(&self) -> Option<&'static str> {
        match self {
            IconOverlay::None => None,
            IconOverlay::Resource { identifier, .. } => Some(identifier),
            IconOverlay::Bytes { identifier, .. } => Some(identifier),
        }
    }
}

pub fn get_techoverlay(metagroup_id: u32, use_old_style: bool) -> IconOverlay {
    if use_old_style {
        match metagroup_id {
            1 => IconOverlay::None,
            2 => OLD_OVERLAY_T2,
            3 => OLD_OVERLAY_STORYLINE,
            4 => OLD_OVERLAY_FACTION,
            5 => OLD_OVERLAY_OFFICER,
            6 => OLD_OVERLAY_DEADSPACE,
            14 => OLD_OVERLAY_T3,
            15 => OLD_OVERLAY_ABYSSAL,
            17 => OLD_OVERLAY_NES,
            19 => OLD_OVERLAY_TIMELIMITED,
            52 => OLD_OVERLAY_STRUCTURE_FACTION,
            53 => OLD_OVERLAY_STRUCTURE_T2,
            54 => OLD_OVERLAY_STRUCTURE_T1,
            62 => OLD_OVERLAY_ACADEMY,
            63 => OLD_OVERLAY_ABERRANT,
            _ => IconOverlay::None
        }
    } else {
        match metagroup_id {
            1 => IconOverlay::None,
            2 => NEW_OVERLAY_T2,
            3 => NEW_OVERLAY_STORYLINE,
            4 => NEW_OVERLAY_FACTION,
            5 => NEW_OVERLAY_OFFICER,
            6 => NEW_OVERLAY_DEADSPACE,
            14 => NEW_OVERLAY_T3,
            15 => NEW_OVERLAY_ABYSSAL,
            17 => NEW_OVERLAY_NES,
            19 => NEW_OVERLAY_TIMELIMITED,
            52 => NEW_OVERLAY_STRUCTURE_FACTION,
            53 => NEW_OVERLAY_STRUCTURE_T2,
            54 => NEW_OVERLAY_STRUCTURE_T1,
            62 => NEW_OVERLAY_ACADEMY,
            63 => NEW_OVERLAY_ABERRANT,
            _ => IconOverlay::None
        }
    }
}

pub fn get_moduleoverlay(module_slot: Option<ModuleSlot>, use_old_style: bool) -> IconOverlay {
    // Currently makes no difference, but handles future changes
    if use_old_style {
        match module_slot {
            None => IconOverlay::None,
            Some(ModuleSlot::High) => OLD_OVERLAY_MODULE_HIGH,
            Some(ModuleSlot::Medium) => OLD_OVERLAY_MODULE_MEDIUM,
            Some(ModuleSlot::Low) => OLD_OVERLAY_MODULE_LOW,
            Some(ModuleSlot::Rig) => OLD_OVERLAY_MODULE_RIG,
            Some(ModuleSlot::Subsystem) => OLD_OVERLAY_MODULE_SUBSYSTEM
        }
    } else {
        match module_slot {
            None => IconOverlay::None,
            Some(ModuleSlot::High) => NEW_OVERLAY_MODULE_HIGH,
            Some(ModuleSlot::Medium) => NEW_OVERLAY_MODULE_MEDIUM,
            Some(ModuleSlot::Low) => NEW_OVERLAY_MODULE_LOW,
            Some(ModuleSlot::Rig) => NEW_OVERLAY_MODULE_RIG,
            Some(ModuleSlot::Subsystem) => NEW_OVERLAY_MODULE_SUBSYSTEM
            // TODO: Possibly port over non-canonical service module overlay
        }
    }
}

pub fn get_cloneoverlay(requires_omega: Option<bool>, use_old_style: bool) -> IconOverlay {
    if use_old_style {
        match requires_omega {
            None => IconOverlay::None,
            Some(true) => OLD_OVERLAY_CLONE_OMEGA,
            Some(false) => OLD_OVERLAY_CLONE_ALPHA
        }
    } else {
        match requires_omega {
            None => IconOverlay::None,
            Some(true) => NEW_OVERLAY_CLONE_OMEGA,
            Some(false) => NEW_OVERLAY_CLONE_ALPHA
        }
    }
}

#[derive(Debug)]
pub enum IconError {
    Cache(CacheError),
    SDE(SDELoadError),
    IO(io::Error),
    Image(image::ImageError),
    String(String)
}

impl Display for IconError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            IconError::Cache(err) => Display::fmt(err, f),
            IconError::SDE(err) => Display::fmt(err, f),
            IconError::IO(err) => Display::fmt(err, f),
            IconError::Image(err) => Display::fmt(err, f),
            IconError::String(msg) => Display::fmt(msg, f),
        }
    }
}

impl Error for IconError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            IconError::Cache(err) => Some(err),
            IconError::SDE(err) => Some(err),
            IconError::IO(err) => Some(err),
            IconError::Image(err) => Some(err),
            IconError::String(_) => None
        }
    }
}

impl From<CacheError> for IconError {
    fn from(value: CacheError) -> Self {
        IconError::Cache(value)
    }
}

impl From<io::Error> for IconError {
    fn from(value: io::Error) -> Self {
        IconError::IO(value)
    }
}

impl From<image::ImageError> for IconError {
    fn from(value: image::ImageError) -> Self {
        IconError::Image(value)
    }
}

impl From<SDELoadError> for IconError {
    fn from(value: SDELoadError) -> Self {
        IconError::SDE(value)
    }
}

#[derive(Debug, Copy, Clone)]
pub enum ModuleSlot {
    High,
    Medium,
    Low,
    Rig,
    Subsystem
}

pub struct IconBuildData {
    types: HashMap<u32, TypeInfo>,
    icon_files: HashMap<u32, String>,
    graphics_folders: HashMap<u32, GraphicInfo>,
    skin_materials: HashMap<u32, u32>
}

impl IconBuildData {
    pub fn load(mut loader: SDELoader, icon_config: IconConfig) -> Result<IconBuildData, SDELoadError> {
        let group_categories = { loader.load_groups()?.map(|g_res| g_res.map(|g| (g.groupID, g.categoryID))).collect::<Result<HashMap<_, _>, _>>()? };

        Ok(Self {
            types: {
                fn load_renderable_types(loader: &mut SDELoader) -> Result<TypeList, SDELoadError> {
                    for res in loader.load_type_lists()? {
                        let type_list = res?;
                        if type_list.typeListID == 140 {
                            return Ok(type_list);
                        }
                    }
                    Err(SDELoadError::IntegrityError(format!("Could not find typelist #140: RenderableTypeIDs in Static Data Export {}", loader.version())))
                }

                let renderable_types = load_renderable_types(&mut loader)?;

                let mut type_metagroups = HashMap::<ids::TypeID, ids::MetaGroupID>::new();
                let mut types = HashMap::<ids::TypeID, TypeInfo>::new();
                for item_type in loader.load_types()? {
                    let item_type = item_type?;
                    let item_category = *group_categories.get(&item_type.groupID).ok_or_else(|| SDELoadError::IntegrityError(format!("Type without associated category? Type:{} Group:{}", item_type.typeID, item_type.groupID)))?;
                    if let Some(metagroup) = item_type.metaGroupID {
                        type_metagroups.insert(item_type.typeID, metagroup);
                    }

                    if item_type.graphicID.is_some() || item_type.iconID.is_some() || (1950..=1955).contains(&item_type.groupID) || item_type.groupID == 4040 {
                        types.insert(item_type.typeID, TypeInfo {
                            group_id: item_type.groupID,
                            category_id: item_category,
                            icon_id: item_type.iconID,
                            graphic_id: item_type.graphicID,
                            meta_group_id: item_type.metaGroupID,
                            is_renderable: renderable_types.contains(item_type.typeID, item_type.groupID, item_category),
                            module_slot: None,
                            omega_required: None,
                            blueprint_output_metagroup: None
                        });
                    }
                }

                for blueprint in loader.load_blueprints()? {
                    let blueprint = blueprint?;

                    if let Some(manufacturing) = blueprint.activities.manufacturing {
                        if let Some((output, _)) = manufacturing.products.first() {
                            if let (Some(bp_info), Some(output_group)) =(types.get_mut(&blueprint.blueprintTypeID), type_metagroups.get(output)) {
                                bp_info.blueprint_output_metagroup = Some(*output_group)
                            }
                        }
                    } else if let Some(reaction) = blueprint.activities.reaction {
                        if let Some((output, _)) = reaction.products.first() {
                            if let (Some(bp_info), Some(output_group)) =(types.get_mut(&blueprint.blueprintTypeID), type_metagroups.get(output)) {
                                bp_info.blueprint_output_metagroup = Some(*output_group)
                            }
                        }
                    }
                }

                if icon_config.clone_overlays || icon_config.module_overlays || icon_config.external_overlays {
                    let mut alpha_skills = HashMap::<ids::TypeID, values::SkillLevel>::new();
                    if icon_config.clone_overlays || icon_config.external_overlays {
                        for clone_grade in loader.load_clone_grades()? {
                            let clone_grade = clone_grade?;
                            alpha_skills.extend(clone_grade.skills);
                            break;  // All alpha clone grades are the same, skip after the first
                        }

                        for (skill, _) in &alpha_skills {
                            if let Some(item_type) = types.get_mut(skill) {
                                item_type.omega_required = Some(false)
                            }
                        }
                    }

                    for type_dogma in loader.load_type_dogma()? {
                        let type_dogma = type_dogma?;
                        if let Some(item_type) = types.get_mut(&type_dogma.typeID) {

                            if icon_config.module_overlays || icon_config.external_overlays {
                                if type_dogma.dogmaEffects.contains_key(&11) { item_type.module_slot = Some(ModuleSlot::Low); }
                                if type_dogma.dogmaEffects.contains_key(&13) { item_type.module_slot = Some(ModuleSlot::Medium); }
                                if type_dogma.dogmaEffects.contains_key(&12) { item_type.module_slot = Some(ModuleSlot::High); }
                                if type_dogma.dogmaEffects.contains_key(&2663) { item_type.module_slot = Some(ModuleSlot::Rig); }
                                if type_dogma.dogmaEffects.contains_key(&3772) { item_type.module_slot = Some(ModuleSlot::Subsystem); }
                            }

                            if icon_config.clone_overlays || icon_config.external_overlays {
                                // For skills, set requirement directly. For other items, determine requirement from required skills
                                if group_categories.get(&item_type.group_id) == Some(&16) {
                                    item_type.omega_required = Some(!alpha_skills.contains_key(&type_dogma.typeID))
                                } else {
                                    const SKILL_ATTRIBUTES: [ids::AttributeID; 6] = [182, 183, 184, 1285, 1289, 1290];
                                    const LEVEL_ATTRIBUTES: [ids::AttributeID; 6] = [277, 278, 279, 1286, 1287, 1288];

                                    let mut skill_required = false;
                                    let mut omega_required = false;

                                    for i in 0..6 {
                                        if let (Some(skill), Some(level)) = (type_dogma.dogmaAttributes.get(&SKILL_ATTRIBUTES[i]), type_dogma.dogmaAttributes.get(&LEVEL_ATTRIBUTES[i])) {
                                            skill_required = true;
                                            omega_required |= alpha_skills.get(&(*skill as ids::TypeID)).is_none_or(|alpha_level| *alpha_level < (*level as u8));
                                        }
                                    }

                                    if skill_required {
                                        item_type.omega_required = Some(omega_required)
                                    }
                                }
                            }
                        }
                    }
                }

                types
            },
            icon_files: { loader.load_icons()?.map(|i_res| i_res.map(|i| (i.iconID, i.iconFile))).collect::<Result<HashMap<_, _>, _>>()? },
            graphics_folders: {
                loader.load_graphics()?.map(|g_res| {
                    match g_res {
                        Ok(graphic) => Ok((graphic.graphicID, GraphicInfo { folder: graphic.iconFolder, hull: graphic.sofHullName })),
                        Err(err) => Err(err)
                    }
                })
                    .collect::<Result<HashMap<u32, GraphicInfo>, SDELoadError>>()?
            },
            skin_materials: {
                let license_skins = loader.load_skin_licenses()?.map(|l_res| l_res.map(|l| (l.typeID, l.skinID))).collect::<Result<HashMap<_, _>, _>>()?;
                let skin_materials = loader.load_skins()?.map(|s_res| s_res.map(|s| (s.skinID, s.skinMaterialID))).collect::<Result<HashMap<_, _>, _>>()?;

                let mut license_materials = HashMap::with_capacity(license_skins.len());
                for (license_id, skin_id) in license_skins {
                    if let Some(material_id) = skin_materials.get(&skin_id) {
                        license_materials.insert(license_id, *material_id);
                    }
                }
                license_materials
            }
        })
    }
}

fn composite_blueprint(background: &Path, overlay: &Path, icon: &Path, tech_icon: Option<&DynamicImage>, out: &Path) -> Result<(), IconError> {
    let mut background_image = ImageReader::open(background)?.with_guessed_format()?.decode()?.into_rgba8();
    let icon_image = ImageReader::open(icon)?.with_guessed_format()?.decode()?.resize_exact(64, 64, FilterType::Lanczos3);
    imageops::overlay(&mut background_image, &icon_image, 0, 0);
    let overlay_image = ImageReader::open(overlay)?.with_guessed_format()?.decode()?.into_rgba8();

    background_image.blend(&overlay_image, image_blend::pixelops::pixel_add, true, false).map_err(io::Error::other)?;

    if let Some(techoverlay) = tech_icon {
        imageops::overlay(&mut background_image, techoverlay, 0, 0);
    }

    background_image.save(out)?;
    Ok(())
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize)]
enum IconKind {
    #[serde(rename="icon")]
    Icon,
    #[serde(rename="bp")]
    Blueprint,
    #[serde(rename="bpc")]
    BlueprintCopy,
    #[serde(rename="reaction")]
    Reaction,
    #[serde(rename="relic")]
    Relic,
    #[serde(rename="render")]
    Render
}

impl IconKind {
    pub fn name(self) -> &'static str {
        match self {
            IconKind::Icon => "icon",
            IconKind::Blueprint => "bp",
            IconKind::BlueprintCopy => "bpc",
            IconKind::Reaction => "reaction",
            IconKind::Relic => "relic",
            IconKind::Render => "render"
        }
    }
}

impl Display for IconKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        <str as Display>::fmt(self.name(), f)
    }
}

#[derive(Debug)]
pub enum OutputMode<'a> {
    ServiceBundle { out: &'a Path },
    IEC { out: &'a Path },
    Web { out: &'a Path, copy_files: bool, hard_link: bool },
    UtilityIcons { out: &'a Path, regexes: Vec<(Regex, Option<String>)> },
    Checksum { out: Option<&'a Path> },
    AuxShipTreeRenders { out: &'a Path },
    AuxIcons { out: &'a Path },
    AuxImages { out: &'a Path, incl_character: bool }
}

impl<'a> OutputMode<'a> {
    pub fn needs_index_update(&self) -> bool {
        match self {
            OutputMode::ServiceBundle { .. } => true,
            OutputMode::IEC { .. } => true,
            OutputMode::Web { .. } => true,
            OutputMode::Checksum { .. } => true,
            OutputMode::UtilityIcons { .. } => false,
            OutputMode::AuxShipTreeRenders { .. } => false,
            OutputMode::AuxIcons { .. } => false,
            OutputMode::AuxImages { .. } => false,
        }
    }
}

fn parse_utility_line(mut line: &str) -> Option<Result<(Regex, Option<String>), IconError>> {
    line = line.split_once('#').map(|(line, _comment)| line).unwrap_or(line).trim();
    if line.len() == 0 { return None; }
    match line.split_once('=') {
        None => {
            match Regex::new(line) {
                Ok(pat) => Some(Ok((pat, None))),
                Err(err) => Some(Err(IconError::String(format!("invalid line in utility_list file: `{}` ({})", line, err))))
            }
        },
        Some((resource, rename)) => {
            match Regex::new(resource) {
                Ok(pat) => Some(Ok((pat, Some(rename.to_owned())))),
                Err(err) => Some(Err(IconError::String(format!("invalid line in utility_list file: `{}` ({})", line, err))))
            }
        }
    }
}

pub fn parse_utility_list(file: Option<&PathBuf>) -> Result<Vec<(Regex, Option<String>)>, IconError> {
    if let Some(file) = file {
        BufReader::new(File::open(file)?)
            .lines()
            .filter_map(|r| match r {
                Ok(line) => parse_utility_line(&line),
                Err(err) => Some(Err(IconError::from(err)))
            })
            .collect()
    } else {
        include_str!("./utilitylist.txt")
            .lines()
            .filter_map(parse_utility_line)
            .collect()
    }
}

pub fn build_icon_export<C: SharedCache, P: AsRef<Path>>(icon_config: IconConfig, output_modes: Vec<OutputMode>, skip_output_if_fresh: bool, no_purge: bool, data: &IconBuildData, cache: &C, icon_dir: P, force_rebuild: bool, mut silent_mode: bool) -> Result<(), IconError> {
    let log_file = crate::LOG_FILE.get();   // TODO: Put in a parameter
    silent_mode |= output_modes.iter().any(|mode| matches!(mode, OutputMode::Checksum { out: None }));  // If "Checksum to stdout" output mode is present, enforce silent mode

    #[allow(non_snake_case)]
    let DO_INDEX_UPDATE = output_modes.iter().any(OutputMode::needs_index_update);

    let icon_dir = icon_dir.as_ref();
    let mut old_index = HashSet::new();
    let index_path = icon_dir.join("cache.csv");

    if DO_INDEX_UPDATE {
        if !silent_mode { println!("Building icons..."); }
        if let Some(mut log) = log_file { writeln!(log, "Building icons...")?; }

        fs::create_dir_all(icon_dir)?;
        if fs::exists(&index_path)? {
            let mut buf = Vec::new();
            let mut reader = BufReader::new(File::open(&index_path)?);
            while reader.read_until(b'\x1E', &mut buf)? > 0 {
                let file = std::str::from_utf8(&buf).map_err(io::Error::other)?.trim_end_matches('\x1E');
                old_index.insert(file.to_string());
                buf.clear();
            };
        }
    }

    let mut service_metadata = BTreeMap::<u32, BTreeMap<IconKind, String>>::new();
    let mut overlay_metadata = BTreeMap::<u32, BTreeSet<&'static str>>::new();
    let mut new_index = HashSet::<String>::new();

    fn is_up_to_date(old_index: &HashSet<String>, new_index: &mut HashSet<String>, index_key: &str, force_rebuild: bool) -> bool {
        new_index.insert(index_key.to_string());
        old_index.contains(index_key) && !force_rebuild
    }

    let mut index_bytes = Vec::new();
    let skip_output;
    let to_remove;
    if DO_INDEX_UPDATE {
        for (type_id, type_info) in &data.types {
            // Skip types without iconID or graphicID as they have no icon, SKINs have custom logic
            if type_info.icon_id.is_none() && type_info.graphic_id.is_none() && type_info.category_id != 91 { continue; }

            if (type_info.category_id == 9) || (type_info.category_id == 34) {
                // Blueprint or reaction

                if let Some(folder) = type_info.graphic_id.and_then(|graphic_id| data.graphics_folders.get(&graphic_id)).and_then(|g| g.folder.as_ref()) && type_info.is_renderable {
                    let icon_resource_bp = format!("{}/{}_64_bp.png", folder.trim_end_matches('/'), type_info.graphic_id.unwrap());
                    let icon_resource_bpc = format!("{}/{}_64_bpc.png", folder.trim_end_matches('/'), type_info.graphic_id.unwrap());

                    if cache.has_resource(&*icon_resource_bp) {
                        let overlay_metagroup = ids::MetaGroupID::max(
                            type_info.meta_group_id.unwrap_or(1),
                            type_info.blueprint_output_metagroup.unwrap_or(1)
                        );

                        let mut techoverlay = get_techoverlay(overlay_metagroup, icon_config.use_old_overlays);
                        if let Some(overlay_identifier) = techoverlay.identifier() { overlay_metadata.entry(*type_id).or_default().insert(overlay_identifier); };

                        // Patch blueprints which may have an overlay burned into them
                        // Skip for types which don't need patching, i.e. metagroup 1 & no overlay
                        // Basilisk blueprint extends into the otherwise clipped corner and doesn't have an overlay to patch out, so skip patching for it
                        if icon_config.external_overlays && overlay_metagroup != 1 && *type_id != 11986 {
                            let index_key = hash::index_key("png", ("clean", cache.hash_of(&icon_resource_bp)?));
                            service_metadata.entry(*type_id).or_default().insert(IconKind::Icon, index_key.clone());
                            service_metadata.entry(*type_id).or_default().insert(IconKind::Blueprint, index_key.clone());
                            if !is_up_to_date(&old_index, &mut new_index, &index_key, force_rebuild) {
                                let mut image = ImageReader::open(&cache.path_of(&*icon_resource_bp)?)?.with_guessed_format()?.decode()?
                                    .resize_exact(64, 64, FilterType::Lanczos3);
                                imageops::overlay(&mut image, BPO_PATCH.load(cache)?.map(|(_, img)| img).expect("overlay patches are included in the binary, so loading must not fail"), 0, 0);
                                image.save(&icon_dir.join(index_key))?;
                            }

                            let index_key = hash::index_key("png", ("clean", cache.hash_of(&icon_resource_bpc)?));
                            service_metadata.entry(*type_id).or_default().insert(IconKind::BlueprintCopy, index_key.clone());
                            if !is_up_to_date(&old_index, &mut new_index, &index_key, force_rebuild) {
                                let mut image = ImageReader::open(&cache.path_of(&*icon_resource_bpc)?)?.with_guessed_format()?.decode()?
                                    .resize_exact(64, 64, FilterType::Lanczos3);
                                imageops::overlay(&mut image, BPC_PATCH.load(cache)?.map(|(_, img)| img).expect("overlay patches are included in the binary, so loading must not fail"), 0, 0);
                                image.save(&icon_dir.join(index_key))?;
                            }

                            continue;
                        }

                        techoverlay = techoverlay.retain_if(!icon_config.external_overlays);

                        if let Some((overlay_cache_key, techoverlay)) = techoverlay.load(cache)? {
                            let index_key = hash::index_key("png", (cache.hash_of(&icon_resource_bp)?, overlay_cache_key));
                            service_metadata.entry(*type_id).or_default().insert(IconKind::Icon, index_key.clone());
                            service_metadata.entry(*type_id).or_default().insert(IconKind::Blueprint, index_key.clone());
                            if !is_up_to_date(&old_index, &mut new_index, &index_key, force_rebuild) {
                                let mut image = ImageReader::open(&cache.path_of(&*icon_resource_bp)?)?.with_guessed_format()?.decode()?.resize_exact(64, 64, FilterType::Lanczos3);
                                imageops::overlay(&mut image, techoverlay, 0, 0);
                                image.save(&icon_dir.join(index_key))?;
                            }

                            if cache.has_resource(&*icon_resource_bpc) {
                                let index_key = hash::index_key("png", (cache.hash_of(&icon_resource_bpc)?, overlay_cache_key));
                                service_metadata.entry(*type_id).or_default().insert(IconKind::BlueprintCopy, index_key.clone());
                                if !is_up_to_date(&old_index, &mut new_index, &index_key, force_rebuild) {
                                    let mut image = ImageReader::open(&cache.path_of(&*icon_resource_bpc)?)?.with_guessed_format()?.decode()?
                                        .resize_exact(64, 64, FilterType::Lanczos3);
                                    imageops::overlay(&mut image, techoverlay, 0, 0);
                                    image.save(&icon_dir.join(index_key))?;
                                }
                            }
                            continue;
                        } else {
                            let index_key = hash::index_key("png", cache.hash_of(&icon_resource_bp)?);
                            service_metadata.entry(*type_id).or_default().insert(IconKind::Icon, index_key.clone());
                            service_metadata.entry(*type_id).or_default().insert(IconKind::Blueprint, index_key.clone());
                            if !is_up_to_date(&old_index, &mut new_index, &index_key, force_rebuild) {
                                let image = ImageReader::open(&cache.path_of(&*icon_resource_bp)?)?.with_guessed_format()?.decode()?
                                    .resize_exact(64, 64, FilterType::Lanczos3);
                                image.save(&icon_dir.join(index_key))?;
                            }

                            if cache.has_resource(&*icon_resource_bpc) {
                                let index_key = hash::index_key("png", cache.hash_of(&icon_resource_bpc)?);
                                service_metadata.entry(*type_id).or_default().insert(IconKind::BlueprintCopy, index_key.clone());
                                if !is_up_to_date(&old_index, &mut new_index, &index_key, force_rebuild) {
                                    let image = ImageReader::open(&cache.path_of(&*icon_resource_bpc)?)?.with_guessed_format()?.decode()?
                                        .resize_exact(64, 64, FilterType::Lanczos3);
                                    image.save(&icon_dir.join(index_key))?;
                                }
                            }
                            continue;
                        }
                    } else {
                        // Missing file
                        if !silent_mode { println!("\tERR: Missing icon for: {}", type_id); }
                        if let Some(mut log) = log_file { writeln!(log, "\tERR: Missing icon for: {}", type_id)?; }
                        continue;
                    }
                } else if let Some(icon) = type_info.icon_id { // If no graphics icon, try icon
                    let icon_resource = &*data.icon_files.get(&icon).ok_or(IconError::String(format!("unknown icon id: {}", icon)))?;
                    if cache.has_resource(&icon_resource) {
                        let techoverlay = get_techoverlay(type_info.meta_group_id.unwrap_or(1), icon_config.use_old_overlays);
                        if let Some(overlay_identifier) = techoverlay.identifier() { overlay_metadata.entry(*type_id).or_default().insert(overlay_identifier); };
                        let (techoverlay_cache, techoverlay_image) = techoverlay.retain_if(!icon_config.external_overlays).load(cache)?.unzip();

                        if type_info.category_id == 34 {
                            let index_key = hash::index_key("png", (
                                cache.hash_of(&icon_resource)?,
                                cache.hash_of("res:/ui/texture/icons/relic.png")?,
                                cache.hash_of("res:/ui/texture/icons/relic_overlay.png")?,
                                techoverlay_cache
                            ));

                            service_metadata.entry(*type_id).or_default().insert(IconKind::Icon, index_key.clone());
                            service_metadata.entry(*type_id).or_default().insert(IconKind::Relic, index_key.clone());
                            if !is_up_to_date(&old_index, &mut new_index, &index_key, force_rebuild) {
                                // Relic BG/overlay
                                composite_blueprint(
                                    &cache.path_of("res:/ui/texture/icons/relic.png")?,
                                    &cache.path_of("res:/ui/texture/icons/relic_overlay.png")?,
                                    &cache.path_of(icon_resource)?,
                                    techoverlay_image,
                                    &icon_dir.join(index_key)
                                )?;
                            }
                            continue;
                        } else if REACTION_GROUPS.contains(&type_info.group_id) {
                            let index_key = hash::index_key("png", (
                                cache.hash_of(&icon_resource)?,
                                cache.hash_of("res:/ui/texture/icons/reaction.png")?,
                                cache.hash_of("res:/ui/texture/icons/bpo_overlay.png")?,
                                techoverlay_cache
                            ));

                            service_metadata.entry(*type_id).or_default().insert(IconKind::Icon, index_key.clone());
                            service_metadata.entry(*type_id).or_default().insert(IconKind::Reaction, index_key.clone());
                            service_metadata.entry(*type_id).or_default().insert(IconKind::Blueprint, index_key.clone());   // Incorrect behaviour of the image service, included for compatibility
                            if !is_up_to_date(&old_index, &mut new_index, &index_key, force_rebuild) {
                                // Reaction BG/overlay
                                composite_blueprint(
                                    &cache.path_of("res:/ui/texture/icons/reaction.png")?,
                                    &cache.path_of("res:/ui/texture/icons/bpo_overlay.png")?,
                                    &cache.path_of(icon_resource)?,
                                    techoverlay_image,
                                    &icon_dir.join(index_key)
                                )?;
                            }
                            continue;
                        } else {
                            let index_key = hash::index_key("png", (
                                cache.hash_of(&icon_resource)?,
                                cache.hash_of("res:/ui/texture/icons/bpo.png")?,
                                cache.hash_of("res:/ui/texture/icons/bpo_overlay.png")?,
                                techoverlay_cache
                            ));

                            // BP & BPC BG/overlay
                            service_metadata.entry(*type_id).or_default().insert(IconKind::Icon, index_key.clone());
                            service_metadata.entry(*type_id).or_default().insert(IconKind::Blueprint, index_key.clone());
                            if !is_up_to_date(&old_index, &mut new_index, &index_key, force_rebuild) {
                                composite_blueprint(
                                    &cache.path_of("res:/ui/texture/icons/bpo.png")?,
                                    &cache.path_of("res:/ui/texture/icons/bpo_overlay.png")?,
                                    &cache.path_of(icon_resource)?,
                                    techoverlay_image,
                                    &icon_dir.join(index_key)
                                )?;
                            }

                            let index_key = hash::index_key("png", (
                                cache.hash_of(&icon_resource)?,
                                cache.hash_of("res:/ui/texture/icons/bpc.png")?,
                                cache.hash_of("res:/ui/texture/icons/bpc_overlay.png")?,
                                techoverlay_cache
                            ));
                            service_metadata.entry(*type_id).or_default().insert(IconKind::BlueprintCopy, index_key.clone());
                            if !is_up_to_date(&old_index, &mut new_index, &index_key, force_rebuild) {
                                composite_blueprint(
                                    &cache.path_of("res:/ui/texture/icons/bpc.png")?,
                                    &cache.path_of("res:/ui/texture/icons/bpc_overlay.png")?,
                                    &cache.path_of(icon_resource)?,
                                    techoverlay_image,
                                    &icon_dir.join(index_key)
                                )?;
                            }
                            continue;
                        }
                    } else {
                        // Skip missing icons, sometimes they're broken in-game.
                        if !silent_mode { println!("\tERR: Missing icon for: {}", type_id); }
                        if let Some(mut log) = log_file { writeln!(log, "\tERR: Missing icon for: {}", type_id)?; }
                        continue;
                    }
                } else {
                    continue; // No icon to be generated here
                }
            } else {
                // Regular item
                let techoverlay = get_techoverlay(type_info.meta_group_id.unwrap_or(1), icon_config.use_old_overlays);
                let moduleoverlay = get_moduleoverlay(type_info.module_slot, icon_config.use_old_overlays);
                let cloneoverlay = get_cloneoverlay(type_info.omega_required, icon_config.use_old_overlays);
                for overlay in [techoverlay, moduleoverlay, cloneoverlay] {
                    if let Some(overlay_identifier) = overlay.identifier() {
                        overlay_metadata.entry(*type_id).or_default().insert(overlay_identifier);
                    }
                }

                let graphic_info = type_info.graphic_id.and_then(|graphic_id| data.graphics_folders.get(&graphic_id));

                let mut icon_resource;
                if let Some(folder) = graphic_info.and_then(|g| g.folder.as_ref()) {
                    icon_resource = format!("{}/{}_64.png", folder.trim_end_matches('/'), type_info.graphic_id.unwrap());

                    // If no graphic, try icon
                    if !cache.has_resource(&*icon_resource) || !type_info.is_renderable {
                        if let Some(icon) = type_info.icon_id {
                            icon_resource = data.icon_files.get(&icon).ok_or(IconError::String(format!("unknown icon id: {}", icon)))?.clone();
                        } else {
                            continue;   // No icon
                        }
                    }

                    let render_resource = format!("{}/{}_512.jpg", folder.trim_end_matches('/'), type_info.graphic_id.unwrap());
                    if cache.has_resource(&*render_resource) {
                        let index_key = hash::index_key("jpg", cache.hash_of(&render_resource)?);
                        service_metadata.entry(*type_id).or_default().insert(IconKind::Render, index_key.clone());
                        if !is_up_to_date(&old_index, &mut new_index, &index_key, force_rebuild) {
                            let _ = fs::copy(cache.path_of(&*render_resource)?, icon_dir.join(index_key)).map_err(IconError::from)?;
                        }

                        if icon_config.external_overlays {
                            let index_key = hash::index_key("png", ("downscale", cache.hash_of(&render_resource)?));
                            if !is_up_to_date(&old_index, &mut new_index, &index_key, force_rebuild) {
                                ImageReader::open(&cache.path_of(&render_resource)?)?
                                    .with_guessed_format()?
                                    .decode()?
                                    .resize_exact(64, 64, FilterType::Lanczos3)
                                    .save(&icon_dir.join(&index_key))?;
                            }
                            service_metadata.entry(*type_id).or_default().insert(IconKind::Icon, index_key.clone());
                            continue;   // Special-case icon has been built, skip general icon logic.
                        }
                    }
                } else if let Some(icon) = type_info.icon_id {
                    icon_resource = data.icon_files.get(&icon).ok_or(IconError::String(format!("unknown icon id: {}", icon)))?.clone();
                } else if type_info.category_id == 91 {
                    // SKIN
                    if let Some(material_id) = data.skin_materials.get(type_id) {
                        icon_resource = format!("res:/ui/texture/classes/skins/icons/{}.png", material_id);
                    } else {
                        continue;   // Some skins are region-exclusive and do not have the resources available on the TQ client, so skip and treat as no-icon types
                    }
                } else {
                    continue; // No icon to be generated here
                }

                if !cache.has_resource(&icon_resource) {
                    if !silent_mode { println!("\tERR: Missing icon for: {}", type_id); }
                    if let Some(mut log) = log_file { writeln!(log, "\tERR: Missing icon for: {}", type_id)?; }
                    continue; // Skip missing icons, sometimes they're broken in-game.
                }

                let techoverlay = techoverlay.retain_if(!icon_config.external_overlays).load(cache)?;
                let moduleoverlay = moduleoverlay.retain_if(!icon_config.external_overlays).load(cache)?;
                let cloneoverlay = cloneoverlay.retain_if(!icon_config.external_overlays).load(cache)?;

                if let (None, None, None) = (&techoverlay, &moduleoverlay, &cloneoverlay) {
                    // These icons are still resized, and so are copied to the icon-cache folder
                    let index_key = hash::index_key("png", cache.hash_of(&icon_resource)?);
                    service_metadata.entry(*type_id).or_default().insert(IconKind::Icon, index_key.clone());

                    if !is_up_to_date(&old_index, &mut new_index, &index_key, force_rebuild) {
                        let image = ImageReader::open(&cache.path_of(&*icon_resource)?)?.with_guessed_format()?.decode()?.resize_exact(64, 64, FilterType::Lanczos3);
                        image.save(&icon_dir.join(index_key))?;
                    }
                    continue;
                } else {
                    let (techoverlay_cache, techoverlay_image) = techoverlay.unzip();
                    let (moduleoverlay_cache, moduleoverlay_image) = moduleoverlay.unzip();
                    let (cloneoverlay_cache, cloneoverlay_image) = cloneoverlay.unzip();

                    let index_key = hash::index_key("png", (
                        cache.hash_of(&*icon_resource)?,
                        techoverlay_cache,
                        moduleoverlay_cache,
                        cloneoverlay_cache
                    ));

                    service_metadata.entry(*type_id).or_default().insert(IconKind::Icon, index_key.clone());

                    if !is_up_to_date(&old_index, &mut new_index, &index_key, force_rebuild) {
                        let mut image = ImageReader::open(&cache.path_of(&icon_resource)?)?.with_guessed_format()?.decode()?.resize_exact(64, 64, FilterType::Lanczos3);
                        if let Some(techoverlay) = techoverlay_image {
                            imageops::overlay(&mut image, techoverlay, 0, 0);
                        }
                        if let Some(moduleoverlay) = moduleoverlay_image {
                            imageops::overlay(&mut image, moduleoverlay, 48, 48);
                        }
                        if let Some(cloneoverlay) = cloneoverlay_image {
                            imageops::overlay(&mut image, cloneoverlay, 48, 0);
                        }
                        image.save(&icon_dir.join(index_key))?;
                    }
                    continue;
                }
            }
        }

        let mut sort_index = new_index.iter().map(String::as_str).collect::<Vec<_>>();
        sort_index.sort();


        let mut first = true;
        for item in sort_index {
            if first {
                first = false;
            } else {
                index_bytes.extend(b"\x1E");
            }
            index_bytes.extend(item.as_bytes())
        }

        fs::write(index_path, &index_bytes)?;

        to_remove = old_index.iter().filter(|key| !new_index.contains(*key)).map(String::as_str).collect::<Vec<&str>>();
        let to_add = new_index.iter().filter(|key| !old_index.contains(*key)).map(String::as_str).collect::<Vec<&str>>();

        skip_output = to_add.len() == 0 && to_remove.len() == 0 && skip_output_if_fresh;
        if skip_output {
            if !silent_mode { println!("Icons fresh, skipping output..."); }
            if let Some(mut log) = log_file { writeln!(log, "Icons fresh, skipping output...")?; }
        } else {
            if !silent_mode { println!("Icons built, generating output..."); }
            if let Some(mut log) = log_file { writeln!(log, "Icons built, generating output...")?; }
        }
    } else {
        if !silent_mode { println!("Generating output..."); }
        if let Some(mut log) = log_file { writeln!(log, "Generating output...")?; }
        skip_output = true; // Unused, but set to true so any bugs will skip output with defective index
        to_remove = Vec::new();
    }

    if !silent_mode { println!("\tImage format: {}", icon_config.image_format.help_text()); }
    if let Some(mut log) = log_file { writeln!(log, "\tImage format: {}", icon_config.image_format.help_text())?; }

    for output_mode in output_modes {
        match output_mode {
            OutputMode::ServiceBundle { out } => {
                if skip_output {
                    if !silent_mode { println!("\tSKIPPED Service Bundle"); }
                    if let Some(mut log) = log_file { writeln!(log, "\tSKIPPED Service Bundle")?; }
                    continue;
                }

                if !silent_mode { println!("\tWriting Service Bundle to {:?}", out); }
                if let Some(mut log) = log_file { writeln!(log, "\tWriting Service Bundle to {:?}", out)?; }
                let mut writer = ZipWriter::new(File::create(out)?);

                let mut written = HashSet::new();
                for (type_id, metadata) in &mut service_metadata {
                    for (icon_kind, filename) in metadata {
                        let cache_file = match &icon_config.image_format {
                            IconImageFormat::Native => filename,
                            #[allow(unreachable_patterns)]  // Other patterns are enabled with cargo features
                            _ => {
                                let new_name = format!(
                                    "{}.{}",
                                    filename.rsplit_once('.').map(|(name, _ext)| name).unwrap_or(filename),
                                    icon_config.image_format.extension().expect("all non-'native' icon image formats have an extension set")
                                );
                                &*mem::replace(filename, new_name)
                            },
                        };
                        let cache_file = &*cache_file;  // Reborrow to switch to immutable borrow
                        if let Some(mut log) = log_file { writeln!(log, "\t\tType {} ({}) - {}", type_id, icon_kind, filename)?; }
                        if written.insert(&*filename) {
                            writer.start_file(&*filename, FileOptions::<()>::default().compression_method(CompressionMethod::Stored))
                                .map_err(|e| format!("err in {}: {}", filename, e))
                                .map_err(io::Error::other)?;

                            icon_config.image_format.write_cacheimage(icon_dir.join(&*cache_file), &mut writer)?;
                        }
                    }
                }

                writer.start_file("service_metadata.json", FileOptions::<()>::default()).map_err(io::Error::other)?;
                serde_json::to_writer_pretty(&mut writer, &service_metadata).map_err(io::Error::other)?;
                if icon_config.external_overlays {
                    writer.start_file("overlay_metadata.json", FileOptions::<()>::default()).map_err(io::Error::other)?;
                    serde_json::to_writer(&mut writer, &overlay_metadata).map_err(io::Error::other)?;

                    write_overlays(cache, &mut writer)?;
                }

                writer.finish().map_err(io::Error::other)?.flush()?;
            }
            OutputMode::IEC { out } => {
                if skip_output {
                    if !silent_mode { println!("\tSKIPPED IEC archive"); }
                    if let Some(mut log) = log_file { writeln!(log, "\tSKIPPED IEC archive")?; }
                    continue;
                }

                if !silent_mode { println!("\tWriting IEC archive to {:?}", out); }
                if let Some(mut log) = log_file { writeln!(log, "\tWriting IEC archive to {:?}", out)?; }
                let mut writer = ZipWriter::new(File::create(out)?);
                // Copy the icons IEC-style; Types with the same icon get duplicated files
                for (type_id, icons) in &service_metadata {
                    for (icon_kind, filename) in icons {
                        match icon_kind {
                            IconKind::Icon => {
                                let output_name = format!("{}_64.{}", type_id, icon_config.image_format.extension().unwrap_or("png"));
                                if let Some(mut log) = log_file { writeln!(log, "\t\tType {} ({}) - {} [{}]", type_id, icon_kind, output_name, filename)?; }
                                writer.start_file(&output_name, FileOptions::<()>::default().compression_method(CompressionMethod::Stored)).map_err(io::Error::other)?;

                                icon_config.image_format.write_cacheimage(icon_dir.join(filename), &mut writer)?;
                            }
                            IconKind::Blueprint | IconKind::Reaction | IconKind::Relic => { /* None, these are duplicated by IconKind::Icon */ }
                            IconKind::BlueprintCopy => {
                                let output_name = format!("{}_bpc_64.{}", type_id, icon_config.image_format.extension().unwrap_or("png"));
                                if let Some(mut log) = log_file { writeln!(log, "\t\tType {} ({}) - {} [{}]", type_id, icon_kind, output_name, filename)?; }
                                writer.start_file(&output_name, FileOptions::<()>::default().compression_method(CompressionMethod::Stored)).map_err(io::Error::other)?;

                                icon_config.image_format.write_cacheimage(icon_dir.join(filename), &mut writer)?;
                            }
                            IconKind::Render => {
                                let output_name = format!("{}_512.{}", type_id, icon_config.image_format.extension().unwrap_or("jpg"));
                                if let Some(mut log) = log_file { writeln!(log, "\t\tType {} ({}) - {} [{}]", type_id, icon_kind, output_name, filename)?; }
                                writer.start_file(&output_name, FileOptions::<()>::default().compression_method(CompressionMethod::Stored)).map_err(io::Error::other)?;

                                icon_config.image_format.write_cacheimage(icon_dir.join(filename), &mut writer)?;
                            }
                        }
                    }
                }
                if icon_config.external_overlays {
                    writer.start_file("overlay_metadata.json", FileOptions::<()>::default()).map_err(io::Error::other)?;
                    serde_json::to_writer(&mut writer, &overlay_metadata).map_err(io::Error::other)?;

                    write_overlays(cache, &mut writer)?;
                }
                writer.finish().map_err(io::Error::other)?.flush()?;
            }
            OutputMode::Web { out, copy_files, hard_link } => {
                if skip_output {
                    if !silent_mode { println!("\tSKIPPED building web folder"); }
                    if let Some(mut log) = log_file { writeln!(log, "\tSKIPPED building web folder")?; }
                    continue;
                }

                let mode_name = if copy_files { "COPYING" } else if hard_link { "HARD LINK" } else { "SOFT LINK" };
                if !silent_mode { println!("\tBuilding web folder to {:?} ({})", out, mode_name); }
                if let Some(mut log) = log_file { writeln!(log, "\tBuilding web folder to {:?} ({})", out, mode_name)?; }

                if icon_config.image_format != IconImageFormat::Native {
                    if !silent_mode { println!("\tImage format override set, ignored by webdir"); }
                    if let Some(mut log) = log_file { writeln!(log, "\tImage format override set, ignored by webdir")?; }
                }

                let mut created_files = HashMap::<String, String>::new();

                let index_path = out.join("index.json");
                let old_links = if fs::exists(&index_path)? {
                    serde_json::from_reader::<_, HashMap<String, String>>(File::open(&index_path)?).map_err(io::Error::other)?
                } else {
                    HashMap::new()
                };

                if icon_config.external_overlays {
                    serde_json::to_writer(&mut File::create(out.join("overlay_metadata.json"))?, &overlay_metadata).map_err(io::Error::other)?;

                    // TODO: Write overlays?
                }

                let mut kind_buf = Vec::<IconKind>::new();
                for (type_id, icons) in &service_metadata {
                    let json_name = format!("{}.json", type_id);
                    let json_filename = out.join(&json_name);
                    kind_buf.extend(icons.keys());
                    let json_content = serde_json::to_string(&kind_buf).map_err(io::Error::other)?;
                    kind_buf.clear();
                    if force_rebuild || old_links.get(&json_name) != Some(&json_content) {
                        fs::write(&json_filename, json_content.as_bytes())?;
                    }
                    created_files.insert(json_name, json_content);

                    for (icon_kind, filename) in icons {
                        let link_name = format!("{}_{}.{}",
                            type_id,
                            icon_kind.name(),
                            { icon_config.image_format.extension().unwrap_or(if IconKind::Render == *icon_kind { "jpg" } else { "png" }) }
                        );
                        let link_source = std::path::absolute(icon_dir.join(filename))?;
                        let link_file = std::path::absolute(out.join(&link_name))?;

                        if force_rebuild || old_links.get(&link_name) != Some(&filename) {
                            if let Some(mut log) = log_file { writeln!(log, "\t\t{} -> {}", &filename, &link_name)?; }
                            if copy_files {
                                fs::copy(link_source, link_file)?;
                            } else if hard_link {
                                if fs::exists(&link_file)? { fs::remove_file(&link_file)? };
                                fs::hard_link(link_source, link_file)?;
                            } else {
                                if fs::exists(&link_file)? { fs::remove_file(&link_file)? };
                                #[cfg(target_family = "windows")]
                                std::os::windows::fs::symlink_file(link_source, link_file)?;
                                #[cfg(target_family = "unix")]
                                std::os::unix::fs::symlink(link_source, link_file)?;
                                #[cfg(not(any(target_family = "windows", target_family = "unix")))]
                                compile_error!("Can't create symlink on OS that is neither windows nor unix :(")
                            }
                        } else {
                            if let Some(mut log) = log_file { writeln!(log, "\t\tSKIP: {}", &link_name)?; }
                        }
                        created_files.insert(link_name, filename.clone());
                    }
                }

                for entry in old_links.keys() {
                    if !created_files.contains_key(entry) {
                        if let Some(mut log) = log_file { writeln!(log, "\t\tRemoved: {}", &entry)?; }
                        match fs::remove_file(out.join(entry)) {
                            Ok(()) => Ok(()),
                            Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
                            res => res
                        }?;
                    }
                }
                serde_json::to_writer(File::create(&index_path)?, &created_files).map_err(io::Error::other)?;
            }
            OutputMode::UtilityIcons { out, regexes } => {
                if !silent_mode { println!("\tWriting Utility archive to {:?}", out); }
                if let Some(mut log) = log_file { writeln!(log, "\tWriting Utility archive to {:?}", out)?; }
                let mut writer = ZipWriter::new(File::create(out)?);

                for resource in cache.iter_resources() {
                    // Cut the amount of resources compared down from ~120k to ~40k
                    // Also this tool isn't intended for a general cache resource file extraction
                    if resource.ends_with("png") || resource.ends_with("jpg") {
                        for (regex, replace) in &regexes {
                            let output_name = if let Some(replace) = replace {
                                match regex.replace(resource, replace) {
                                    Cow::Borrowed(_) => continue,
                                    Cow::Owned(new) => {
                                        format!(
                                            "{}.{}",
                                            new.rsplit_once('.').map(|(name, _ext)| name).unwrap_or(&new),
                                            icon_config.image_format.extension().unwrap_or_else(|| resource.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("png"))
                                        )
                                    },
                                }
                            } else {
                                if regex.is_match(resource) {
                                    format!(
                                        "{}.{}",
                                        resource.rsplit_once('.').map(|(name, _ext)| name).unwrap_or(&resource),
                                        icon_config.image_format.extension().unwrap_or_else(|| resource.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("png"))
                                    )
                                } else {
                                    continue
                                }
                            };

                            match cache.path_of(&*resource) {
                                Ok(path) => {
                                    if let Some(mut log) = log_file { writeln!(log, "\t\t{} -> {}", resource, output_name)?; }

                                    writer.start_file(output_name, FileOptions::<()>::default().compression_method(CompressionMethod::Stored)).map_err(io::Error::other)?;
                                    icon_config.image_format.write_cacheimage(path, &mut writer)?;
                                }
                                Err(CacheError::ResourceNotFound(_)) => {
                                    // Shouldn't happen unless iter_resources has a bug
                                    if let Some(mut log) = log_file { writeln!(log, "\t\tMissing file: {}", resource)?; }
                                }
                                Err(err) => Err(err)?
                            }
                        }
                    }
                }

                writer.finish().map_err(io::Error::other)?.flush()?;
            }
            OutputMode::Checksum { out } => {
                // Checksum is never skipped
                assert!(DO_INDEX_UPDATE);
                let checksum = md5::compute(&index_bytes);
                if let Some(mut log) = log_file { writeln!(log, "Checksum:{:x}", checksum)?; }
                if let Some(outfile) = out {
                    if !silent_mode { println!("\tWriting checksum to {:?}", outfile); }
                    fs::write(outfile, format!("{:x}", checksum))?
                } else {
                    assert!(silent_mode);
                    print!("{:x}", md5::compute(&index_bytes))
                }
            },
            OutputMode::AuxShipTreeRenders { out } => {
                if !silent_mode { println!("\tWriting Auxiliary Ship Tree Render archive to {:?}", out); }
                if let Some(mut log) = log_file { writeln!(log, "\tWriting Auxiliary Ship Tree Render archive to {:?}", out)?; }
                let mut writer = ZipWriter::new(File::create(out)?);

                let mut buf = Cursor::new(Vec::<u8>::new());

                for (type_id, type_info) in &data.types {
                    if type_info.category_id != 6 { continue; } // Only handle player ships, maybe change this if there's ever demand for the other icons
                    if let Some(graphic_id) = type_info.graphic_id {
                        if let Some(graphic) = data.graphics_folders.get(&graphic_id) {
                            if let (Some(folder), Some(hull)) = (&graphic.folder, &graphic.hull) {
                                let resource = format!("{}/{}_isis.png", folder, hull);

                                if !cache.has_resource(&resource) { continue; }
                                let resource_path = cache.path_of(&resource)?;

                                let image = ImageReader::open(resource_path)?.with_guessed_format()?.decode()?.to_rgba8();
                                let (width, height) = image.dimensions();
                                let mut out = GrayAlphaImage::new(width, height);

                                for y in 0..height {
                                    for x in 0..width {
                                        let p = *image.get_pixel(x, y);
                                        // Copy RED channel onto alpha, and set Luma to 255
                                        out.put_pixel(x, y, LumaA([255, p[0]]));
                                    }
                                }

                                // Write to a buffer because img.write_to requires `Seek`
                                icon_config.image_format.write_dyimage(DynamicImage::ImageLumaA8(out), &mut buf, |img, buf| { img.write_to(buf, ImageFormat::Png)?; Ok(()) })?;

                                writer.start_file(format!("{}.{}", type_id, icon_config.image_format.extension().unwrap_or("png")), FileOptions::<()>::default().compression_method(CompressionMethod::Stored)).map_err(io::Error::other)?;
                                std::io::copy(&mut buf.get_ref().as_slice(), &mut writer)?;
                                buf.set_position(0);
                                buf.get_mut().clear();
                            }
                        }
                    }
                }
            }
            OutputMode::AuxIcons { out } => {
                if !silent_mode { println!("\tWriting Auxiliary Icon dump archive to {:?}", out); }
                if let Some(mut log) = log_file { writeln!(log, "\tWriting Auxiliary Icon dump archive to {:?}", out)?; }
                let mut writer = ZipWriter::new(File::create(out)?);
                for (icon_id, resource) in &data.icon_files {
                    let (_path, extension) = resource.rsplit_once('.')
                        .or_else(|| resource.rsplit_once('/'))
                        .unwrap_or(("", resource));

                    if !silent_mode { println!("\t\t{}: {}", icon_id, resource); };
                    if let Some(mut log) = log_file { writeln!(log, "\t\t{}: {}", icon_id, resource)?; }

                    writer.start_file(format!("{}.{}", icon_id, icon_config.image_format.extension().unwrap_or(extension)), FileOptions::<()>::default().compression_method(CompressionMethod::Stored)).map_err(io::Error::other)?;
                    icon_config.image_format.write_cacheimage(cache.path_of(resource)?, &mut writer)?;
                }
                writer.finish().map_err(io::Error::other)?.flush()?;
            }
            OutputMode::AuxImages { out, incl_character } => {
                if !silent_mode { println!("\tWriting Auxiliary All-Images dump archive to {:?}", out); }
                if let Some(mut log) = log_file { writeln!(log, "\tWriting Auxiliary All-Images dump archive to {:?}", out)?; }
                let mut writer = ZipWriter::new(File::create(out)?);

                let resource_valid = |resource: &&str| (resource.ends_with("png") || resource.ends_with("jpg")) && (incl_character || !resource.starts_with("res:/graphics/character/"));

                let res_count = cache.iter_resources().filter(resource_valid).count();
                for (n, resource) in cache.iter_resources().filter(resource_valid).enumerate() {
                    let (_resource_kind, filename) = resource.split_once(":/").unwrap_or(("", resource));

                    if !silent_mode { println!("\t\t[{}/{}] {}", n, res_count, resource); }
                    if let Some(mut log) = log_file { writeln!(log, "\t\t[{}/{}] {}", n, res_count, resource)?; }

                    let filename = if let Some(extension) = icon_config.image_format.extension() {
                        Cow::Owned(format!("{}.{}", filename.rsplit_once('.').map(|(name, _ext)| name).unwrap_or(filename), extension))
                    } else {
                        Cow::Borrowed(filename)
                    };

                    writer.start_file(filename, FileOptions::<()>::default().compression_method(CompressionMethod::Stored)).map_err(io::Error::other)?;
                    icon_config.image_format.write_cacheimage(cache.path_of(resource)?, &mut writer)?;
                }
                writer.finish().map_err(io::Error::other)?.flush()?;
            },
        }
    }

    if DO_INDEX_UPDATE {
        if !no_purge {
            if !silent_mode { println!("Cleaning up icon folder (Removing {} files)", to_remove.len()); }
            if let Some(mut log) = log_file { writeln!(log, "Cleaning up icon folder (Removing {} files)", to_remove.len())?; }
            for filename in &to_remove {
                fs::remove_file(icon_dir.join(filename))?;
            }
        }
    }

    Ok(())
}

fn write_overlays<C: SharedCache>(cache: &C, writer: &mut ZipWriter<File>) -> Result<(), IconError> {
    let mut buf = Cursor::new(Vec::new());
    for (prefix, list) in [("old", OLD_OVERLAY_LIST), ("new", NEW_OVERLAY_LIST)] {
        for overlay in list {
            writer.start_file(
                format!("overlay/{}/{}.png", prefix, overlay.identifier().expect("overlay-list overlays should always have an identifier")),
                FileOptions::<()>::default().compression_method(CompressionMethod::Stored)
            ).map_err(io::Error::other)?;

            if let Some((_, overlay_image)) = overlay.load(cache)? {
                buf.set_position(0);
                overlay_image.write_to(&mut buf, ImageFormat::Png)?;
                buf.set_position(0);
                io::copy(&mut buf.get_ref().as_slice(), writer)?;
            }
        }
    }
    Ok(())
}
