#![cfg_attr(feature="docs_export", evestaticdata_macro::doc_export)]

#[cfg(feature="docs_export")]
use evestaticdata_macro::doc_sde;

use crate::types::{ids, uuids, values};
use crate::util;
use indexmap::IndexMap;
use serde::de::{DeserializeOwned, SeqAccess, Unexpected, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::fs::File;
use std::hash::Hash;
use std::io;
use std::io::{BufRead, BufReader, Read, Seek};
use std::marker::PhantomData;
use util::units::EVEUnit;
use zip::ZipArchive;
use zip::result::ZipError;
use crate::types::ids::MissionID;
use crate::util::reflist::RefList;

/// Error indicating failure to load SDE
#[derive(Debug)]
pub enum SDELoadError {
    /// IO Error while reading from SDE
    IO(io::Error),
    /// An error occurred parsing the .zip file; Archive corrupt?
    Zip(ZipError),
    /// SDE zip file did not contain expected file, did the SDE format change?
    ArchiveFileNotFound(String),
    /// Parsing the JSON content failed, did the SDE schema change?
    ParseError { file: String, entry: usize, error: serde_json::Error},
    /// Data integrity problem, did the SDE schema change?
    IntegrityError(String)
}

impl Display for SDELoadError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SDELoadError::IO(err) => write!(f, "IO error: {}", err),
            SDELoadError::Zip(err) => write!(f, "Zip error: {}", err),
            SDELoadError::ArchiveFileNotFound(filename) => write!(f, "SDE did not contain expected file: `{}`", filename),
            SDELoadError::ParseError { file, entry, error } => write!(f, "Parse error in `{}` entry {}: {}", file, entry, error),
            SDELoadError::IntegrityError(err_description) => write!(f, "SDE data integrity error ({})", err_description)
        }
    }
}

impl Error for SDELoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            SDELoadError::IO(err) => Some(err),
            SDELoadError::Zip(err) => Some(err),
            SDELoadError::ArchiveFileNotFound(_) => None,
            SDELoadError::ParseError { error, .. } => Some(error),
            SDELoadError::IntegrityError(_) => None
        }
    }
}

impl From<io::Error> for SDELoadError {
    fn from(value: io::Error) -> Self {
        SDELoadError::IO(value)
    }
}

impl From<ZipError> for SDELoadError {
    fn from(value: ZipError) -> Self {
        SDELoadError::Zip(value)
    }
}

/// Helper trait for `deserialize_inline_entry_map`
trait InlineEntry<K> {
    fn key(&self) -> K;
}

/// Deserialize a json-array of [`InlineEntry`]-trait values into an IndexMap
fn deserialize_inline_entry_map<'de, K: Deserialize<'de> + Hash + Eq + Ord, V: Deserialize<'de> + InlineEntry<K>, D: Deserializer<'de>>(deserializer: D) -> Result<IndexMap<K, V>, D::Error> {
    struct EntryVisitor<K, V>(PhantomData<K>, PhantomData<V>);
    impl<'de, K: Deserialize<'de> + Hash + Eq + Ord, V: Deserialize<'de> + InlineEntry<K>> Visitor<'de> for EntryVisitor<K, V> {
        type Value = IndexMap<K, V>;

        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            formatter.write_str("a map encoded as array of flattened entries")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error> where A: SeqAccess<'de> {
            let size_hint = seq.size_hint();
            let mut map = size_hint.map(IndexMap::with_capacity).unwrap_or_else(IndexMap::new);
            while let Some(value) = seq.next_element::<V>()? {
                map.insert(InlineEntry::key(&value), value);
            }
            Ok(map)
        }
    }

    deserializer.deserialize_seq(EntryVisitor::<K, V>(PhantomData::default(), PhantomData::default()))
}

/// Deserialize a json-array of [`ExplicitMapEntry`] values into an IndexMap
fn deserialize_explicit_entry_map<'de, K: Deserialize<'de> + Hash + Eq + Ord, V: Deserialize<'de>, D: Deserializer<'de>>(deserializer: D) -> Result<IndexMap<K, V>, D::Error> {
    struct EntryVisitor<K, V>(PhantomData<K>, PhantomData<V>);
    impl<'de, K: Deserialize<'de> + Hash + Eq + Ord, V: Deserialize<'de>> Visitor<'de> for EntryVisitor<K, V> {
        type Value = IndexMap<K, V>;

        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            formatter.write_str("a map encoded as array of flattened entries")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error> where A: SeqAccess<'de> {
            let size_hint = seq.size_hint();
            let mut map = size_hint.map(IndexMap::with_capacity).unwrap_or_else(IndexMap::new);
            while let Some(value) = seq.next_element::<ExplicitMapEntry<K, V>>()? {
                map.insert(value._key, value._value);
            }
            Ok(map)
        }
    }

    deserializer.deserialize_seq(EntryVisitor::<K, V>(PhantomData::default(), PhantomData::default()))
}

// Helper macro for implement into-map collection
macro_rules! impl_map_collect {
    ($id:ty, $val:ty, $field:ident) => {
        impl FromIterator<$val> for IndexMap<$id, $val> {
            fn from_iter<T: IntoIterator<Item=$val>>(iter: T) -> Self {
                IndexMap::from_iter(iter.into_iter().map(|a| (a.$field, a)))
            }
        }
    };
    ($id:ty, $val:ty, $key:ty, fn $lambda:expr) => {
        impl FromIterator<$key> for IndexMap<$id, $val> {
            fn from_iter<T: IntoIterator<Item=$key>>(iter: T) -> Self {
                IndexMap::from_iter(iter.into_iter().map($lambda))
            }
        }
    }
}

// Generic types
/// Helper type for JSON maps that are encoded as arrays of objects
#[derive(Deserialize)]
struct ExplicitMapEntry<K, V> {
    _key: K,
    _value: V
}

/// Position of an object on the New Eden cluster map, units in metres.
///
/// See <https://developers.eveonline.com/docs/guides/map-data/> for detailed explanation
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="docs_export", doc_sde(common_type))]
pub struct MapPosition {
    /// +X, East/Right
    pub x: f64,
    /// +Y, Up
    pub y: f64,
    /// +Z, North/Forward
    pub z: f64
}

/// Position of an object within a [`SolarSystem`], units in metres.
///
/// See <https://developers.eveonline.com/docs/guides/map-data/> for detailed explanation
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="docs_export", doc_sde(common_type))]
pub struct CelestialPosition {
    /// +X, West/Left
    pub x: f64,
    /// +Y, Up
    pub y: f64,
    /// +Z, North/Forward
    pub z: f64
}

/// 2D-map position of an object, units in metres.
///
/// Up/down, Left/right directions depend on context, see <https://developers.eveonline.com/docs/guides/map-data/> for detailed explanation
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="docs_export", doc_sde(common_type))]
pub struct Position2D {
    /// +X, East/Right
    pub x: f64,
    /// +Y, North/Up
    pub y: f64
}

/// String with multiple language variants
///
/// English is always available. Usually, all other languages are also available
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(common_type))]
pub struct LocalizedString {
    /// English
    pub en: String,
    /// German
    pub de: Option<String>,
    /// Spanish
    pub es: Option<String>,
    /// French
    pub fr: Option<String>,
    /// Japanese
    pub ja: Option<String>,
    /// Korean
    pub ko: Option<String>,
    /// Russian
    pub ru: Option<String>,
    /// Chinese
    pub zh: Option<String>
}

impl LocalizedString {
    /// English string
    ///
    /// Getter for `en` field matching the `try_` methods
    pub fn en(&self) -> &str {
        &self.en
    }

    /// German string if available, else English string
    pub fn try_de(&self) -> &str {
        self.de.as_ref().unwrap_or(&self.en)
    }

    /// Spanish string if available, else English string
    pub fn try_es(&self) -> &str {
        self.es.as_ref().unwrap_or(&self.en)
    }

    /// French string if available, else English string
    pub fn try_fr(&self) -> &str {
        self.fr.as_ref().unwrap_or(&self.en)
    }

    /// Japanese string if available, else English string
    pub fn try_ja(&self) -> &str {
        self.ja.as_ref().unwrap_or(&self.en)
    }

    /// Korean string if available, else English string
    pub fn try_ko(&self) -> &str {
        self.ko.as_ref().unwrap_or(&self.en)
    }

    /// Russian string if available, else English string
    pub fn try_ru(&self) -> &str {
        self.ru.as_ref().unwrap_or(&self.en)
    }

    /// Chinese string if available, else English string
    pub fn try_zh(&self) -> &str {
        self.zh.as_ref().unwrap_or(&self.en)
    }
}

// SDE data types

/// Agent (Mission NPC) that is located in space, rather than docked in a station
///
/// Additional Agent information is contained in [`NpcCharacter`] data
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="accountingEntryTypes"))]
pub struct AccountingEntryType {
    #[serde(rename="_key")]
    pub accountingEntryTypeID: ids::AccountingEntryTypeID,
    pub internalName: String,
    pub name: LocalizedString,
    pub description: Option<LocalizedString>,
    pub journalMessage: Option<LocalizedString>
}

impl_map_collect!(ids::AccountingEntryTypeID, AccountingEntryType, accountingEntryTypeID);

/// The different kinds of agent
///
/// See <https://wiki.eveuniversity.org/Agent#Category> for information about the various kinds of Agent
#[derive(Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub enum AgentType {
    NonAgent,
    BasicAgent,
    TutorialAgent,
    ResearchAgent,
    CONCORDAgent,
    GenericStorylineMissionAgent,
    StorylineMissionAgent,
    EventMissionAgent,
    FactionalWarfareAgent,
    EpicArcAgent,
    AuraAgent,
    CareerAgent,
    HeraldryAgent
}

impl Display for AgentType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {AgentType::NonAgent => write!(f, "NonAgent"),
            AgentType::BasicAgent => write!(f, "BasicAgent"),
            AgentType::TutorialAgent => write!(f, "TutorialAgent"),
            AgentType::ResearchAgent => write!(f, "ResearchAgent"),
            AgentType::CONCORDAgent => write!(f, "CONCORDAgent"),
            AgentType::GenericStorylineMissionAgent => write!(f, "GenericStorylineMissionAgent"),
            AgentType::StorylineMissionAgent => write!(f, "StorylineMissionAgent"),
            AgentType::EventMissionAgent => write!(f, "EventMissionAgent"),
            AgentType::FactionalWarfareAgent => write!(f, "FactionalWarfareAgent"),
            AgentType::EpicArcAgent => write!(f, "EpicArcAgent"),
            AgentType::AuraAgent => write!(f, "AuraAgent"),
            AgentType::CareerAgent => write!(f, "CareerAgent"),
            AgentType::HeraldryAgent => write!(f, "HeraldryAgent"),
        }
    }
}

// Helper for deserializing
/// The different kinds of agent
///
/// See <https://wiki.eveuniversity.org/Agent#Category> for information about the various kinds of Agent
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="agentTypes"))]
#[cfg_attr(feature="docs_export", doc_sde(rename="AgentType"))]
struct AgentTypeEntry {
    /// Identifier for this AgentType
    #[serde(rename="_key")]
    agentTypeID: ids::AgentTypeID,
    /// Name for this AgentType
    name: AgentType
}

/// Agent (Mission NPC) that is located in space, rather than docked in a station
///
/// Additional Agent information is contained in [`NpcCharacter`] data
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="agentsInSpace"))]
pub struct AgentInSpace {
    /// CharacterID for this agent
    #[serde(rename="_key")]
    pub agentID: ids::CharacterID,
    /// 'Dungeon' within which the agent is located
    ///
    /// Data about dungeons is not available for EVE 3rd party developers
    pub dungeonID: ids::DungeonID,
    /// SolarSystem in which the Agent is located
    pub solarSystemID: ids::SolarSystemID,
    /// Spawnpoint for agent, no data available for EVE 3rd party developers
    pub spawnPointID: ids::SpawnPointID,
    /// TypeID of the agent's ship (Note: Agent Ships are not the same as the player-flyable ships, and have different TypeIDs)
    pub typeID: ids::TypeID
}

impl_map_collect!(ids::CharacterID, AgentInSpace, agentID);

/// Character Ancestry; Now-unused character creation element (Removed from player character creation 2021-03-02)
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="ancestries"))]
pub struct Ancestry {
    /// Identifier for this Ancestry
    #[serde(rename="_key")]
    pub ancestryID: ids::AncestryID,
    /// Bloodline this ancestry is a part of
    pub bloodlineID: ids::BloodlineID,
    /// Skill training attribute modifier
    pub charisma: i32,
    /// Skill training attribute modifier
    pub intelligence: i32,
    /// Skill training attribute modifier
    pub memory: i32,
    /// Skill training attribute modifier
    pub perception: i32,
    /// Skill training attribute modifier
    pub willpower: i32,
    /// Ancestry description as (previously) displayed in game client
    pub description: LocalizedString,
    /// Icon, optional
    pub iconID: Option<ids::IconID>,
    /// Ancestry name
    pub name: LocalizedString,
    /// Short English description
    pub shortDescription: Option<String>
}

impl_map_collect!(ids::AncestryID, Ancestry, ancestryID);

/// Effect applied when in proximity to an object
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="appliedProximityEffects"))]
pub struct AppliedProximityEffect {
    /// TypeID for the celestial object this effect is centered on
    #[serde(rename="_key")]
    pub objectTypeID: ids::TypeID,
    #[serde(deserialize_with="deserialize_explicit_entry_map")]
    pub dbuffs: IndexMap<ids::DynamicBuffID, f64>,
    pub radius: f64,
    pub delaySeconds: f64
}

impl_map_collect!(ids::TypeID, AppliedProximityEffect, objectTypeID);

/// Dungeon Archetype
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="archetypes"))]
pub struct Archetype {
    /// ID for this Archetype
    #[serde(rename="_key")]
    pub archetypeID: ids::DungeonArchetypeID,
    /// Title; Kind of sites in this archetype
    pub title: Option<LocalizedString>,
    /// Text description of this archetype
    pub description: LocalizedString
}

impl_map_collect!(ids::DungeonArchetypeID, Archetype, archetypeID);

/// Character Bloodline; Character creation element
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="bloodlines"))]
pub struct Bloodline {
    /// Identifier for this Bloodline
    #[serde(rename="_key")]
    pub bloodlineID: ids::BloodlineID,
    /// Default NPC Corporation for characters with this Bloodline
    pub corporationID: ids::CorporationID,
    /// Bloodline description, as shown in game client
    pub description: LocalizedString,
    /// Icon, optional
    pub iconID: Option<ids::IconID>,
    /// Bloodline name
    pub name: LocalizedString,
    /// Character race this bloodline is a part of
    pub raceID: ids::RaceID,
    /// Skill training attribute modifier
    pub charisma: i32,
    /// Skill training attribute modifier
    pub intelligence: i32,
    /// Skill training attribute modifier
    pub memory: i32,
    /// Skill training attribute modifier
    pub perception: i32,
    /// Skill training attribute modifier
    pub willpower: i32,
}

impl_map_collect!(ids::BloodlineID, Bloodline, bloodlineID);

/// Industry Blueprint. Also describes Reaction Formulae and the Sleeper Relics used in T3 production
///
/// Note: The SDE provides Blueprint Copy and Blueprint Original data as 'merged' into a single entry for the Blueprint's typeID.
/// 'Copying' & 'Research Time/Material' activities are not usable with BPCs, 'Invention' activity is not usable with BPOs.
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="blueprints"))]
pub struct Blueprint {
    /// Key; Blueprint TypeID. Duplicate of explicit `blueprintTypeID` field in entry. This library current favours using the explicit field, this may change.
    /// In the event this is de-duplicated by removing the entry field this field will be renamed to [`Blueprint::blueprintTypeID`] to retain backwards compatibility.
    #[serde(rename="_key")]
    #[allow(unused)]
    blueprintTypeID_key: ids::TypeID,
    /// TypeID of this blueprint. BP Originals and BP Copies share the same TypeID.
    pub blueprintTypeID: ids::TypeID,
    /// The maximum amount of job runs that a single blueprint copy can have
    ///
    /// This is not the limit of repeats that can be scheduled in a single manufacturing/copying/etc job.
    pub maxProductionLimit: i32,
    /// Activities available for this blueprint type
    pub activities: BlueprintActivities
}

/// Blueprint activities for a [`Blueprint`]
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct BlueprintActivities {
    /// Blueprint copying activity. When present on blueprint types, only applicable to blueprint *originals*
    pub copying: Option<BPActivity>,
    /// Manufacturing activity
    pub manufacturing: Option<BPActivity>,
    /// Time Efficiency Research activity. When present on blueprint types, only applicable to blueprint *originals*
    pub research_time: Option<BPActivity>,
    /// Material Efficiency Research activity. When present on blueprint types, only applicable to blueprint *originals*
    pub research_material: Option<BPActivity>,
    /// Invention activity. Applicable to Sleeper Relics and blueprint *copies* but not blueprint originals.
    pub invention: Option<BPActivity>,
    /// Reaction activity
    pub reaction: Option<BPActivity>,
}

impl IntoIterator for BlueprintActivities {
    type Item = (ids::IndustryActivityID, BPActivity);
    type IntoIter = std::iter::FilterMap<std::array::IntoIter<Option<(ids::IndustryActivityID, BPActivity)>, 6>, fn(Option<(ids::IndustryActivityID, BPActivity)>) -> Option<(ids::IndustryActivityID, BPActivity)>>;

    fn into_iter(self) -> Self::IntoIter {
        [
            self.copying.map(|a| (5, a)),
            self.manufacturing.map(|a| (1, a)),
            self.research_time.map(|a| (3, a)),
            self.research_material.map(|a| (4, a)),
            self.invention.map(|a| (8, a)),
            self.reaction.map(|a| (9, a)),
        ].into_iter()
            .filter_map(|o| o)
    }
}


/// A single [`Blueprint`] activity
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(external_type))]
pub struct BPActivity {
    /// Materials and quantity required for one run of this activity
    #[cfg_attr(feature="docs_export", doc_sde(alias_type="Vec<BPProduct>"))]
    #[serde(default)]
    #[serde(deserialize_with="deserialize_activity_materials")]
    pub materials: IndexMap<ids::TypeID, u32>,
    /// Products, quantity, and optional probability for one run of this activity.
    /// Only one product type is allowed per run of this activity; When multiple types of products are available, one must be selected by the player when setting up the industry job
    #[cfg_attr(feature="docs_export", doc_sde(alias_type="Vec<BPProduct>"))]
    #[serde(default)]
    #[serde(deserialize_with="deserialize_activity_products")]
    pub products: IndexMap<ids::TypeID, (u32, Option<f64>)>,
    /// Skills required to set up a run of this activity
    #[cfg_attr(feature="docs_export", doc_sde(alias_type="Vec<BPSkill>"))]
    #[serde(default)]
    #[serde(deserialize_with="deserialize_activity_skills")]
    pub skills: IndexMap<ids::TypeID, values::SkillLevel>,
    /// Time required for one run of this activity, in seconds
    pub time: u32
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
struct BPMaterial {
    typeID: ids::TypeID,
    quantity: u32
}
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
struct BPProduct {
    typeID: ids::TypeID,
    quantity: u32,
    probability: Option<f64>
}
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct BPSkill {
    typeID: ids::TypeID,
    level: values::SkillLevel,
}

fn deserialize_activity_materials<'de, D: Deserializer<'de>>(deserializer: D) -> Result<IndexMap<ids::TypeID, u32>, D::Error> {
    pub struct MaterialVisitor;
    impl<'de> Visitor<'de> for MaterialVisitor {
        type Value = IndexMap<ids::TypeID, u32>;

        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            formatter.write_str("array of blueprint materials (typeID & quantity)")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error> where A: SeqAccess<'de> {
            let size_hint = seq.size_hint();
            let mut map = size_hint.map(IndexMap::with_capacity).unwrap_or_else(IndexMap::new);
            while let Some(value) = seq.next_element::<BPMaterial>()? {
                map.insert(value.typeID, value.quantity);
            }
            Ok(map)
        }
    }

    deserializer.deserialize_seq(MaterialVisitor)
}
fn deserialize_activity_products<'de, D: Deserializer<'de>>(deserializer: D) -> Result<IndexMap<ids::TypeID, (u32, Option<f64>)>, D::Error> {
    pub struct ProductVisitor;
    impl<'de> Visitor<'de> for ProductVisitor {
        type Value = IndexMap<ids::TypeID, (u32, Option<f64>)>;

        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            formatter.write_str("array of blueprint products (typeID, quantity & probability)")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error> where A: SeqAccess<'de> {
            let size_hint = seq.size_hint();
            let mut map = size_hint.map(IndexMap::with_capacity).unwrap_or_else(IndexMap::new);
            while let Some(value) = seq.next_element::<BPProduct>()? {
                map.insert(value.typeID, (value.quantity, value.probability));
            }
            Ok(map)
        }
    }

    deserializer.deserialize_seq(ProductVisitor)
}
fn deserialize_activity_skills<'de, D: Deserializer<'de>>(deserializer: D) -> Result<IndexMap<ids::TypeID, values::SkillLevel>, D::Error> {
    pub struct SkillVisitor;
    impl<'de> Visitor<'de> for SkillVisitor {
        type Value = IndexMap<ids::TypeID, values::SkillLevel>;

        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            formatter.write_str("array of blueprint skills (typeID & level)")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error> where A: SeqAccess<'de> {
            let size_hint = seq.size_hint();
            let mut map = size_hint.map(IndexMap::with_capacity).unwrap_or_else(IndexMap::new);
            while let Some(value) = seq.next_element::<BPSkill>()? {
                map.insert(value.typeID, value.level);
            }
            Ok(map)
        }
    }

    deserializer.deserialize_seq(SkillVisitor)
}

impl_map_collect!(ids::TypeID, Blueprint, blueprintTypeID);


/// Item Type 'Category'; Collection of [Groups](Group)
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="categories"))]
pub struct Category {
    /// ID for this category
    #[serde(rename="_key")]
    pub categoryID: ids::TypeID,
    /// Name of this category
    pub name: LocalizedString,
    /// 'Published' status; If false, not visible to players in the game client
    pub published: bool,
    /// Icon, optional
    pub iconID: Option<ids::IconID>
}

impl_map_collect!(ids::CategoryID, Category, categoryID);


/// Ship Mastery Certificate
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="certificates"))]
pub struct Certificate {
    /// ID for this certificate
    #[serde(rename="_key")]
    pub certificateID: ids::CertificateID,
    /// Skill [`Group`] this Certificate is for
    pub groupID: ids::GroupID,
    /// Certificate name
    pub name: LocalizedString,
    /// Certificate description
    pub description: LocalizedString,
    /// Ships this certificate is recommended for
    #[serde(default)]
    pub recommendedFor: Vec<ids::TypeID>,
    /// Skill levels for this certificate
    #[serde(rename="skillTypes")]
    #[serde(deserialize_with="deserialize_inline_entry_map")]
    pub skillLevels: IndexMap<ids::TypeID, CertificateSkillLevels>
}

/// Skill levels required for a certificate level
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct CertificateSkillLevels {
    /// Skill this 'levels' data is for
    #[serde(rename="_key")]
    pub skillTypeID: ids::TypeID,
    /// Skill level required for 'basic' certificate
    pub basic: values::SkillLevel,
    /// Skill level required for 'standard' certificate
    pub standard: values::SkillLevel,
    /// Skill level required for 'improved' certificate
    pub improved: values::SkillLevel,
    /// Skill level required for 'advanced' certificate
    pub advanced: values::SkillLevel,
    /// Skill level required for 'elite' certificate
    pub elite: values::SkillLevel,
}

impl InlineEntry<ids::TypeID> for CertificateSkillLevels {
    fn key(&self) -> ids::TypeID {
        self.skillTypeID
    }
}

impl_map_collect!(ids::CertificateID, Certificate, certificateID);

/// Character skill training Attribute
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="characterAttributes"))]
pub struct CharacterAttribute {
    /// ID for this character attribute
    #[serde(rename="_key")]
    pub characterAttributeID: ids::CharacterAttributeID,
    /// Name, as displayed on the character sheet
    pub name: LocalizedString,
    /// Description, in English
    pub description: String,
    /// Icon for attribute
    pub iconID: ids::IconID,
    /// Notes, in English
    pub notes: String,
    /// Short description, in English
    pub shortDescription: String
}

impl_map_collect!(ids::CharacterAttributeID, CharacterAttribute, characterAttributeID);

/// Character title, cosmetic text tag earned through achievements
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="characterAttributes"))]
pub struct CharacterTitle {
    /// ID for this character attribute
    #[serde(rename="_key")]
    pub characterTitleID: uuids::CharacterTitleID,
    /// Title text, as displayed on the character sheet
    pub name: LocalizedString
}

impl_map_collect!(uuids::CharacterTitleID, CharacterTitle, characterTitleID);

/// Information about Alpha clones
/// Currently there is one entry for each of the 4 races' Alpha Clones, but the entries are the same; Each character race is allowed to train the same skills, including ships of the other races
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="cloneGrades"))]
pub struct CloneGrade {
    /// ID for this clone grade
    #[serde(rename="_key")]
    pub cloneGradeID: ids::CloneGradeID,
    /// Name (Not displayed in-game)
    pub name: String,
    /// Skills that may be trained by this clone grade
    #[serde(deserialize_with="deserialize_clonegrade_skills")]
    #[cfg_attr(feature="docs_export", doc_sde(alias_type="Vec<CloneSkill>"))]
    pub skills: IndexMap<ids::TypeID, values::SkillLevel>
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
struct CloneSkill {
    /// Skill typeID
    typeID: ids::TypeID,
    /// Maximum skill level that may be trained
    level: values::SkillLevel,
}
fn deserialize_clonegrade_skills<'de, D: Deserializer<'de>>(deserializer: D) -> Result<IndexMap<ids::TypeID, values::SkillLevel>, D::Error> {
    pub struct SkillVisitor;
    impl<'de> Visitor<'de> for SkillVisitor {
        type Value = IndexMap<ids::TypeID, values::SkillLevel>;

        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            formatter.write_str("array of clonegrade skills (typeID & level)")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error> where A: SeqAccess<'de> {
            let size_hint = seq.size_hint();
            let mut map = size_hint.map(IndexMap::with_capacity).unwrap_or_else(IndexMap::new);
            while let Some(value) = seq.next_element::<CloneSkill>()? {
                map.insert(value.typeID, value.level);
            }
            Ok(map)
        }
    }

    deserializer.deserialize_seq(SkillVisitor)
}

impl_map_collect!(ids::CloneGradeID, CloneGrade, cloneGradeID);

/// Information about ore/gas/ice compression
/// Ore compression has a 1:1 input output ratio, N units of 'oreTypeID' yield N units of 'compressedTypeID'
/// Volume ratio is provided by `compressedType.volume / oreType.volume`
/// For ore and ice, compression is lossless. Gas must be decompressed before use, where some losses are had. (Depending on skills & facility used)
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="compressibleTypes"))]
pub struct CompressibleType {
    /// Ore (input) typeID
    #[serde(rename="_key")]
    pub oreTypeID: ids::TypeID,
    /// Compressed ore (output) typeID
    pub compressedTypeID: ids::TypeID
}

impl_map_collect!(ids::TypeID, ids::TypeID, CompressibleType, fn |c| (c.oreTypeID, c.compressedTypeID));

/// Contraband status information for a [`Type`]
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="contrabandTypes"))]
pub struct ContrabandType {
    /// Type for which this Contraband information applies
    #[serde(rename="_key")]
    pub typeID: ids::TypeID,
    /// Per-faction contraband info; An entry means the Type is contraband in the given faction
    #[serde(deserialize_with="deserialize_inline_entry_map")]
    pub factions: IndexMap<ids::FactionID, ContrabandFactionInfo>
}

/// Per-faction Contraband information
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct ContrabandFactionInfo {
    /// Faction for which this info applies
    #[serde(rename="_key")]
    pub factionID: ids::FactionID,
    /// Minimum solarsystem security in which NPC customs agents will attack if this type of contraband is carried
    ///
    /// Mechanic appears to be disabled, with this value always set greater than the maximum security level of 1.0
    pub attackMinSec: f64,
    /// Minimum solarsystem security in which NPC customs confiscate this type of contraband
    pub confiscateMinSec: f64,
    /// Fine (multiplier * item value carried, e.g. 4.5 = 450% of the contraband's value) to be paid if caught
    pub fineByValue: f64,
    /// Faction standing loss if caught carrying this contraband
    pub standingLoss: f64
}

impl InlineEntry<ids::FactionID> for ContrabandFactionInfo {
    fn key(&self) -> ids::FactionID {
        self.factionID
    }
}

impl_map_collect!(ids::TypeID, ContrabandType, typeID);

/// Resources required for Player-owned-Starbase Control Tower operation
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="controlTowerResources"))]
pub struct ControlTowerResources {
    /// TypeID of the Control Tower type this information applies to
    #[serde(rename="_key")]
    pub towerTypeID: ids::TypeID,
    /// Resources required for the operation of this Control Tower
    pub resources: Vec<ControlTowerResourceInfo>
}

/// Resources required for Player-owned-Starbase Control Tower operation
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct ControlTowerResourceInfo {
    /// Purpose for which this resource is required. (Either Online operation or Reinforcement)
    pub purpose: ResourcePurpose,
    /// Quantity required per hour of operation
    pub quantity: u32,
    /// Type of the required resource
    pub resourceTypeID: ids::TypeID,
    /// If set, this resource is only required if operating in the specified Faction's space
    pub factionID: Option<ids::FactionID>,
    /// If set, this resource is only required if operating above the specified security level
    pub minSecurityLevel: Option<f64>
}

#[repr(u8)]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
#[derive(serde_repr::Serialize_repr, serde_repr::Deserialize_repr, Copy, Clone, PartialEq, Eq, Debug)]
pub enum ResourcePurpose {
    /// Resource required for keeping a Control Tower online
    Online = 1,
    /// Legacy value, unused
    Power = 2,
    /// Legacy value, unused
    CPU = 3,
    /// Resource required for Control Tower reinforcement
    Reinforce = 4
}

impl ResourcePurpose {
    pub fn name(&self) -> &'static str {
        match self {
            ResourcePurpose::Online => "Online",
            ResourcePurpose::Power => "Power",
            ResourcePurpose::CPU => "CPU",
            ResourcePurpose::Reinforce => "Reinforce"
        }
    }
}

impl Display for ResourcePurpose {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self.name(), f)
    }
}

impl_map_collect!(ids::TypeID, ControlTowerResources, towerTypeID);

/// NPC Station Activity/"Specialization"
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="corporationActivities"))]
pub struct CorporationActivity {
    /// ID for this activity
    #[serde(rename="_key")]
    pub corporationActivityID: ids::CorporationActivityID,
    /// Name for this activity
    pub name: LocalizedString
}

impl_map_collect!(ids::CorporationActivityID, CorporationActivity, corporationActivityID);


/// Grouping of corporation permission roles
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="corporationRoleGroups"))]
pub struct CorporationRoleGroup {
    /// ID for this role group
    #[serde(rename="_key")]
    pub corporationRoleGroupID: ids::CorporationRoleGroupID,
    pub name: LocalizedString,
    pub appliesTo: CorporationRoleGroupAppliesTo,
    pub appliesToGrantable: CorporationRoleGroupAppliesToGrantable,
    pub isDivisional: bool,
    pub isLocational: bool,
}

#[derive(Debug, Deserialize)]
#[allow(non_camel_case_types)]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub enum CorporationRoleGroupAppliesTo {
    roles,
    rolesAtHQ,
    rolesAtBase,
    rolesAtOther
}

#[derive(Debug, Deserialize)]
#[allow(non_camel_case_types)]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub enum CorporationRoleGroupAppliesToGrantable {
    grantableRoles,
    grantableRolesAtHQ,
    grantableRolesAtBase,
    grantableRolesAtOther
}

impl_map_collect!(ids::CorporationRoleGroupID, CorporationRoleGroup, corporationRoleGroupID);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="corporationRoles"))]
pub struct CorporationRole {
    /// ID for this role
    #[serde(rename="_key")]
    pub corporationRoleID: ids::CorporationRoleID,
    pub name: LocalizedString,
    pub shortName: String,
    pub description: LocalizedString,
    #[serde(default)]
    pub roleGroupIDs: Vec<ids::CorporationRoleGroupID>
}

impl_map_collect!(ids::CorporationRoleID, CorporationRole, corporationRoleID);

/// 'Dynamic Buff'; e.g. Command Burst bonus effects
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="dbuffCollections"))]
pub struct DynamicBuff {
    /// ID for this warfare buff. Referenced by attributes on Command Burst charges
    #[serde(rename="_key")]
    pub DynamicBuffID: ids::DynamicBuffID,
    /// Aggregate mode for multiple buffs; Whether the maximum or minimum value is selected when multiple buffs of different strengths are applied to a ship
    pub aggregateMode: DynamicBuffAggregateMode,
    /// Developer description, in English
    pub developerDescription: String,
    /// Display name, as shown in tooltip in-game
    pub displayName: Option<LocalizedString>,
    /// Attributes whose effects are applied as Item Modifiers
    #[serde(default)]
    #[serde(deserialize_with="deserialize_dynamicbuff_item_modifiers")]
    pub itemModifiers: Vec<ids::AttributeID>,
    /// Attributes whose effects are applied as Location Group Modifiers
    #[serde(default)]
    pub locationGroupModifiers: Vec<DynamicBuffLocationGroupModifier>,
    /// Attributes whose effects are applied as Location Modifiers
    #[serde(default)]
    #[serde(deserialize_with="deserialize_dynamicbuff_location_modifiers")]
    pub locationModifiers: Vec<ids::AttributeID>,
    /// Attributes whose effects are applied as Location with-required-skill Modifiers
    #[serde(default)]
    pub locationRequiredSkillModifiers: Vec<DynamicBuffLocationRequiredSkillModifier>,
    /// Operation applied by modifiers
    pub operationName: DynamicBuffOperation,
    /// How the effect value is displayed in-game
    pub showOutputValueInUI: DynamicBuffUIMode
}

fn deserialize_dynamicbuff_item_modifiers<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<ids::AttributeID>, D::Error> {
    struct SeqVisitor;
    impl<'de> Visitor<'de> for SeqVisitor {
        type Value = Vec<ids::AttributeID>;

        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            formatter.write_str("array of dynamicbuff item modifier attributes")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error> where A: SeqAccess<'de> {
            #[derive(Debug, Deserialize)]
            #[allow(non_snake_case)]
            #[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
            struct DynamicBuffItemModifier {
                pub dogmaAttributeID: ids::AttributeID
            }

            let size_hint = seq.size_hint();
            let mut vec = size_hint.map(Vec::with_capacity).unwrap_or_else(Vec::new);
            while let Some(value) = seq.next_element::<DynamicBuffItemModifier>()? {
                vec.push(value.dogmaAttributeID)
            }
            Ok(vec)
        }
    }

    deserializer.deserialize_seq(SeqVisitor)
}

fn deserialize_dynamicbuff_location_modifiers<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<ids::AttributeID>, D::Error> {
    struct SeqVisitor;
    impl<'de> Visitor<'de> for SeqVisitor {
        type Value = Vec<ids::AttributeID>;

        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            formatter.write_str("array of dynamicBuff location modifier attributes")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error> where A: SeqAccess<'de> {
            #[derive(Debug, Deserialize)]
            #[allow(non_snake_case)]
            #[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
            struct DynamicBuffLocationModifier {
                dogmaAttributeID: ids::AttributeID
            }

            let size_hint = seq.size_hint();
            let mut vec = size_hint.map(Vec::with_capacity).unwrap_or_else(Vec::new);
            while let Some(value) = seq.next_element::<DynamicBuffLocationModifier>()? {
                vec.push(value.dogmaAttributeID)
            }
            Ok(vec)
        }
    }

    deserializer.deserialize_seq(SeqVisitor)
}

/// Aggregate mode for warfare buff effect stacking
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub enum DynamicBuffAggregateMode {
    /// If multiple buffs stack, the maximum value is selected
    Maximum,
    /// If multiple buffs stack, the minimum value is selected
    Minimum
}

/// Dogma operation for warfare buff
///
/// Subject to change
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub enum DynamicBuffOperation {
    // Dogma is weird and complicated, so no individual docs on these
    PostMul, PostPercent, ModAdd, PreAssignment, PostAssignment
}

/// Warfare buff display mode
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub enum DynamicBuffUIMode {
    /// Buff amount is not shown
    Hide,
    /// Buff amount is shown as-is, e.g. `10 -> "10%", -10 -> "-10%"`
    ShowNormal,
    /// Buff amount is shown inverted, e.g. `10 -> "-10%", -10 -> "10%"`
    ShowInverted
}

/// Attribute whose effects are applied as Location Group Modifier
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct DynamicBuffLocationGroupModifier {
    /// Attribute source for effect
    pub dogmaAttributeID: ids::AttributeID,
    /// Applicable group
    pub groupID: ids::GroupID
}

/// Attributes whose effects are applied as Location with-required-skill Modifiers
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct DynamicBuffLocationRequiredSkillModifier {
    /// Attribute source for effect
    pub dogmaAttributeID: ids::AttributeID,
    /// Skill required by applicable types
    pub skillID: ids::TypeID
}

impl_map_collect!(ids::DynamicBuffID, DynamicBuff, DynamicBuffID);


/// Attribute Category, grouping of [`Attribute`]
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="dogmaAttributeCategories"))]
pub struct AttributeCategory {
    /// ID for this category
    #[serde(rename="_key")]
    pub attributeCategoryID: ids::AttributeCategoryID,
    /// Category name, in English
    pub name: String,
    /// Description, in English
    pub description: Option<String>
}

impl_map_collect!(ids::AttributeCategoryID, AttributeCategory, attributeCategoryID);

/// Dogma Attribute, describing properties for [`Type`]s. Such as HP, maximum velocity, and other item stats
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="dogmaAttributes"))]
pub struct Attribute {
    /// ID for this attribute
    #[serde(rename="_key")]
    pub attributeID: ids::AttributeID,
    /// [`AttributeCategory`] for this attribute
    pub attributeCategoryID: Option<ids::AttributeCategoryID>,
    /// ???
    pub chargeRechargeTimeID: Option<u32>,
    /// ???
    pub dataType: i32,
    /// Default implied value if an attribute is not explicitly given for a type
    pub defaultValue: f64,
    /// "Developer" description in English
    pub description: Option<String>,
    /// In-game name, as displayed in item stats
    pub displayName: Option<LocalizedString>,
    /// If true, display this attribute when it's value is `0.0`. If false, attribute is hidden when the value is `0.0`
    pub displayWhenZero: bool,
    /// If set to true, higher values are considered better than lower values. Inverted for false. Used for e.g. determining which module have a better attribute value than another module
    pub highIsGood: bool,
    /// Icon for attribute, as disabled in in-game stats
    pub iconID: Option<ids::IconID>,
    /// Attribute specifying the maximum value for this attribute
    pub maxAttributeID: Option<ids::AttributeID>,
    /// Attribute specifying the minimum value for this attribute
    pub minAttributeID: Option<ids::AttributeID>,
    /// "Developer" name in English
    pub name: String,
    /// 'Published' status; If false, not visible to players in the game client
    pub published: bool,
    /// If false, this attribute is subject to stacking penalties
    pub stackable: bool,
    /// Tooltip tile, as displayed when hovering over an attribute in-game
    pub tooltipTitle: Option<LocalizedString>,
    /// Tooltip description, as displayed when hovering over an attribute in-game
    pub tooltipDescription: Option<LocalizedString>,
    /// Unit for this attribute's values
    #[cfg_attr(feature="docs_export", doc_sde(alias_type="Option<ids::UnitID>"))]
    pub unitID: Option<EVEUnit>,
}

impl_map_collect!(ids::AttributeID, Attribute, attributeID);

/// Dogma Effect, describing interactions of [`Attribute`]s
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="dogmaEffects"))]
pub struct Effect {
    /// ID for this effect
    #[serde(rename="_key")]
    pub effectID: ids::EffectID,
    /// Category of this effect
    pub effectCategoryID: ids::EffectCategoryID,
    /// Effect name
    pub name: String,
    /// Description of the effect, fitting hardpoint & slot effects have their description shown ingame
    pub description: Option<LocalizedString>,
    /// (Unused) If set to true, auto-repeat of this module is disabled
    pub disallowAutoRepeat: bool,
    /// Display name for effect, as shown in-game
    pub displayName: Option<LocalizedString>,
    /// Graphic related ID. Not documented here.
    pub guid: Option<String>,
    /// Effect icon
    pub iconID: Option<ids::IconID>,
    /// Whether effect counts as assistance in combat (such as for criminal/suspect timers)
    pub isAssistance: bool,
    /// Whether effect counts as attacking in combat (such as for criminal/suspect timers)
    pub isOffensive: bool,
    /// Whether effect can be active during warp
    pub isWarpSafe: bool,
    /// Whether effect is set as visible in the client. (Unusual and unintuitive for effects)
    pub published: bool,
    /// (Unused) Sound effect name
    pub sfxName: Option<String>,
    /// Unintuitive dogma wizardry. Not documented here.
    pub distribution: Option<i32>,
    /// Unintuitive dogma wizardry. Not documented here.
    pub electronicChance: bool,
    /// Unintuitive dogma wizardry. Not documented here.
    pub dischargeAttributeID: Option<ids::AttributeID>,
    /// Unintuitive dogma wizardry. Not documented here.
    pub durationAttributeID: Option<ids::AttributeID>,
    /// Unintuitive dogma wizardry. Not documented here.
    pub falloffAttributeID: Option<ids::AttributeID>,
    /// Unintuitive dogma wizardry. Not documented here.
    pub fittingUsageChanceAttributeID: Option<ids::AttributeID>,
    /// Unintuitive dogma wizardry. Not documented here.
    pub npcActivationChanceAttributeID: Option<ids::AttributeID>,
    /// Unintuitive dogma wizardry. Not documented here.
    pub npcUsageChanceAttributeID: Option<ids::AttributeID>,
    /// Unintuitive dogma wizardry. Not documented here.
    pub rangeAttributeID: Option<ids::AttributeID>,
    /// Unintuitive dogma wizardry. Not documented here.
    pub trackingSpeedAttributeID: Option<ids::AttributeID>,
    /// Unintuitive dogma wizardry. Not documented here.
    pub resistanceAttributeID: Option<ids::AttributeID>,
    /// Unintuitive dogma wizardry. Not documented here.
    pub rangeChance: bool,
    /// Unintuitive dogma wizardry. Not documented here.
    pub propulsionChance: bool,
    /// Unintuitive dogma wizardry. Not documented here.
    #[serde(default)]
    pub modifierInfo: Vec<ModifierInfo>,
}

/// Dogma effect "modifier", describing mathematical operations of an effect
///
/// Unintuitive dogma wizardry. Not documented here.
#[derive(Debug, Serialize, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct ModifierInfo {
    pub domain: String,
    pub func: String,
    pub operation: Option<i32>,
    pub modifiedAttributeID: Option<ids::AttributeID>,
    pub modifyingAttributeID: Option<ids::AttributeID>,
    pub groupID: Option<ids::GroupID>,
    pub effectID: Option<ids::EffectID>,
    pub skillTypeID: Option<ids::TypeID>
}

impl_map_collect!(ids::EffectID, Effect, effectID);

/// Unit of measurement used in EVE Online, see [`EVEUnit`] for details
/// For formatting values with units, use [`EVEUnit::format`]
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="dogmaUnits"))]
pub struct DogmaUnit {
    /// UnitID for this unit, encoded as an [`EVEUnit`] enum
    ///
    /// EVEUnit provides formatting functions
    #[serde(rename="_key")]
    pub unitID: EVEUnit,
    /// Name of this unit, a unit or it's associated measure.
    pub name: String,
    /// Description of this unit, either the full name of the unit ("Kilogram per cubic meter") or a short description of the unit and it's purpose
    pub description: Option<LocalizedString>,
    /// Displayed text snippet, usually the short symbol for the unit
    pub displayName: Option<LocalizedString>,
}

impl_map_collect!(EVEUnit, DogmaUnit, unitID);

/// Dungeon, Mission/Anomaly instance
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="dungeons"))]
pub struct Dungeon {
    /// ID for this dungeon
    #[serde(rename="_key")]
    pub dungeonID: ids::DungeonID,
    /// Archetype this dungeon belongs to
    pub archetypeID: ids::DungeonArchetypeID,
    /// Name for this dungeon, as shown in-game
    pub name: LocalizedString,
    /// Description for this dungeon, shown in pop-up on entry(?)
    pub description: Option<LocalizedString>,
    /// Description for this dungeon, shown in pop-up on entry(?)
    pub gameplayDescription: Option<LocalizedString>,
    /// Ships allowed to enter this dungeon (Usually locked out by an acceleration gate)
    pub allowedShipsList: Option<Vec<ids::TypeListID>>,
    /// Associated faction
    pub factionID: Option<ids::FactionID>
}

impl_map_collect!(ids::DungeonID, Dungeon, dungeonID);


/// Dynamic attributes for a [`Type`], used for Mutaplasmids.
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="dynamicItemAttributes"))]
pub struct DynamicItemAttributes {
    /// Mutaplasmid typeID
    #[serde(rename="_key")]
    pub mutaplasmidTypeID: ids::TypeID,
    /// Attributes modified by this mutaplasmid
    ///
    /// Upon application of the mutaplasmid, a random roll is made between `max` and `min` to generate the value multiplier
    #[serde(deserialize_with="deserialize_inline_entry_map")]
    pub attributeIDs: IndexMap<ids::AttributeID, DynamicAttributeInfo>,
    /// "IOMapping"; Describes which types the mutaplasmid can be applied to, and the resulting output type.
    pub inputOutputMapping: Vec<DynamicItemAttributesIOMapping>
}

/// Info about a single dynamic attribute
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct DynamicAttributeInfo {
    /// AttributeID that is modified by mutaplasmid
    #[serde(rename="_key")]
    pub attributeID: ids::AttributeID,
    /// If true, a higher value for this attribute is considered better (e.g. HP bonus), if false a lower value is "better" (e.g. CPU/PG usage)
    ///
    /// [`Attribute`]s also have their own `highIsGood` field
    pub highIsGood: Option<bool>,   // TODO: Add default value? TODO: Check if this ever deviates from the attribute's value
    /// Maximum multiplier for attribute value
    pub max: f64,
    /// Minimum multiplier for attribute value
    pub min: f64
}

impl InlineEntry<ids::AttributeID> for DynamicAttributeInfo {
    fn key(&self) -> ids::AttributeID {
        self.attributeID
    }
}

/// Mutaplasmid IOMapping
///
/// Describes which types the mutaplasmid can be applied to, and the resulting output type.
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct DynamicItemAttributesIOMapping {
    /// List of typeIDs the parent mutaplasmid can be applied to
    pub applicableTypes: Vec<ids::TypeID>,
    /// Output type of modified module
    pub resultingType: ids::TypeID
}

impl_map_collect!(ids::TypeID, DynamicItemAttributes, mutaplasmidTypeID);

/// Epic Arc
///
/// Replayable Mission "arc"/storyline
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="epicArcs"))]
pub struct EpicArc {
    /// Epic Arc ID
    #[serde(rename="_key")]
    pub epicArcID: ids::EpicArcID,
    /// Time before this Epic Arc may be restarted if failed or completed, in hours
    pub arcRestartInterval: u32,
    /// Associated faction
    pub factionID: Option<ids::FactionID>,
    /// Background badge art, no longer shown ingame?
    ///
    /// The icons as shown in The Agency are under Cache Resource folder `res:/ui/texture/classes/agency/icons/contenttypes/...`
    pub iconID: ids::IconID,
    /// Name (Note: Players often refer to the epic arcs by their faction)
    pub name: LocalizedString,
    /// Mission chain in this arc
    #[serde(deserialize_with="deserialize_inline_entry_map")]
    pub missions: IndexMap<ids::MissionID, EpicArcMission>
}

impl_map_collect!(ids::EpicArcID, EpicArc, epicArcID);

/// Epic Arc specific information for a [`Mission`]
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct EpicArcMission {
    /// Mission ID
    #[serde(rename="_key")]
    pub missionID: ids::MissionID,
    /// Agent for this mission
    pub agentID: ids::CharacterID,
    /// Next mission if this current mission is failed
    pub failMissionID: Option<ids::MissionID>,
    /// Next mission options if this current mission is successful
    ///
    /// Players may select one mission out of these options to continue with
    #[serde(default)]
    pub nextMissions: Vec<ids::MissionID>
}

impl InlineEntry<ids::MissionID> for EpicArcMission {
    fn key(&self) -> MissionID {
        self.missionID
    }
}

/// Temporary skill unlock package
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="expertSystems"))]
pub struct ExpertSystem {
    /// Expert System inventory type
    #[serde(rename="_key")]
    pub typeID: ids::TypeID,
    #[serde(default)]
    pub associatedShipTypes: Vec<ids::TypeID>,
    pub durationDays: u32,
    pub hidden: bool,
    pub internalName: String,
    pub retired: bool,
    #[serde(deserialize_with="deserialize_expertsystem_skills")]
    pub skillsGranted: IndexMap<ids::TypeID, values::SkillLevel>
}

impl_map_collect!(ids::TypeID, ExpertSystem, typeID);

fn deserialize_expertsystem_skills<'de, D: Deserializer<'de>>(deserializer: D) -> Result<IndexMap<ids::TypeID, values::SkillLevel>, D::Error> {
    struct MapVisitor;
    impl<'de> Visitor<'de> for MapVisitor {
        type Value = IndexMap<ids::TypeID, values::SkillLevel>;

        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            formatter.write_str("array of expert system skill levels")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error> where A: SeqAccess<'de> {
            #[derive(Debug, Deserialize)]
            #[allow(non_snake_case)]
            #[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
            struct ExpertSystemSkillLevel {
                level: values::SkillLevel,
                typeID: ids::TypeID
            }

            let size_hint = seq.size_hint();
            let mut map = size_hint.map(IndexMap::with_capacity).unwrap_or_else(IndexMap::new);
            while let Some(value) = seq.next_element::<ExpertSystemSkillLevel>()? {
                map.insert(value.typeID, value.level);
            }
            Ok(map)
        }
    }

    deserializer.deserialize_seq(MapVisitor)
}

/// The major and minor NPC factions
///
/// e.g. Caldari/Minmatar/Amarr/Gallente but also CONCORD, ORE, and SOE
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="factions"))]
pub struct Faction {
    /// FactionID
    #[serde(rename="_key")]
    pub factionID: ids::FactionID,
    /// CorporationID for the "main" corporation of this faction. The Navy corporation for major factions.
    pub corporationID: Option<ids::CorporationID>,
    /// Faction name
    pub name: LocalizedString,
    /// Faction description
    pub description: LocalizedString,
    /// Shorter description
    pub shortDescription: Option<LocalizedString>,
    /// Logo filename
    pub flatLogo: Option<String>,
    /// Logomark filename
    pub flatLogoWithName: Option<String>,
    /// Logo iconID
    pub iconID: ids::IconID,    // TODO: Verify if this is equivalent to `flatLogo`
    /// [`CharacterRace`]'s included in this faction
    pub memberRaces: Vec<ids::RaceID>,
    /// CorporationID of the factional warfare corporation, if this faction participates in fwar.
    pub militiaCorporationID: Option<ids::CorporationID>,
    /// Unknown
    pub sizeFactor: f64,    // TODO: Figure out what this is
    /// "Capital"/home system of this faction
    pub solarSystemID: ids::SolarSystemID,
    /// Whether this faction uses a unique name; Currently always true, and the [`name`] is always present
    pub uniqueName: bool
}

impl_map_collect!(ids::FactionID, Faction, factionID);


#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="fighterAbilities"))]
pub struct FighterAbility {
    #[serde(rename="_key")]
    pub fighterAbilityID: ids::FighterAbilityID,
    pub displayName: LocalizedString,
    pub tooltipText: Option<LocalizedString>,
    pub disallowInHighSec: bool,
    pub disallowInLowSec: bool,
    pub iconID: ids::IconID,
    pub targetMode: FighterAbilityTargetMode,
    pub turretGraphicID: Option<ids::GraphicID>
}

impl_map_collect!(ids::FighterAbilityID, FighterAbility, fighterAbilityID);

#[derive(Debug, Deserialize)]
#[allow(non_camel_case_types)]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub enum FighterAbilityTargetMode {
    itemTargeted,
    pointTargeted,
    untargeted
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="fighterAbilitiesByType"))]
pub struct FighterAbilitiesByType {
    #[serde(rename="_key")]
    pub typeID: ids::TypeID,
    pub abilitySlot0: FighterAbilityInfo,
    pub abilitySlot1: FighterAbilityInfo,
    pub abilitySlot2: Option<FighterAbilityInfo>,
}

impl_map_collect!(ids::TypeID, FighterAbilitiesByType, typeID);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct FighterAbilityInfo {
    pub abilityID: ids::FighterAbilityID,
    pub cooldownSeconds: Option<u32>,
    pub charges: Option<FighterAbilityCharges>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct FighterAbilityCharges {
    pub chargeCount: u32,
    pub rearmTimeSeconds: u32
}

/// Freelance job schema, describes the possible kinds of freelance job
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="freelanceJobSchemas"))]
pub struct FreelanceJobSchema {
    /// Type of job this schema describes
    #[serde(rename="_key")]
    pub job_type: JobType,
    /// Job title/name
    pub title: LocalizedString,
    /// Description of this job type, shown in-game when hovering over the job type selection
    pub description: LocalizedString,
    /// Search filter tags for this job schema,
    pub contentTags: Vec<String>,
    /// Multiplier of contribution payout
    ///
    /// Used for 'insurance' job type to determine % of ship value that will be reimbursed
    pub contributionMultiplier: Option<ContributionMultiplier>,
    /// Description text for progress; e.g. "HP repaired" or "Items delivered"
    pub progressDescription: LocalizedString,
    /// Description text for reward; e.g. "Reward per HP boosted" or "Reward per item delivered"
    pub rewardDescription: LocalizedString,
    /// Description text for total job goal; e.g. "Total hit points to be boosted" or "Number of items to be delivered"
    pub targetDescription: LocalizedString,
    /// Icon for job
    pub iconID: String, // TODO: Document how these new icons work  res:/ui/texture/eveicon/system_icons/delivery_16px.png
    /// Maximum contributions allowed per character
    pub maxContributionsPerParticipant: ContributionInfo,
    /// Maximum progress per single contribution
    ///
    /// Used for insurance, where this determines the maximum payout for a single ship loss
    pub maxProgressPerContribution: Option<ContributionInfo>,
    /// Parameters (other options)
    #[serde(deserialize_with="deserialize_inline_entry_map")]
    pub parameters: IndexMap<String, JobSchemaParameter>
}

/// Job type for a freelance AIR opportunities job
#[derive(Debug, Deserialize, Eq, PartialEq, Hash, Copy, Clone)]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub enum JobType {
    /// Boost another (specified) player's shield HP
    BoostShield,
    /// Capture FW complex
    CaptureFWComplex,
    /// Damage player ships
    DamageShip,
    /// Defend FW complex
    DefendFWComplex,
    /// Deliver (provide) item
    DeliverItem,
    /// Destroy player ships
    KillCapsuleer,
    /// Destroy NPC ships
    KillNPC,
    /// Mine (but not hand over) ore
    MineOre,
    /// Repair another (specified) player's armour HP
    RepairArmor,
    /// Ship insurance; Pays out on own ship's destruction
    ///
    /// Automatic form of player-run "Ship Replacement Programs"
    ShipInsurance
}

/// Multiplier of contribution payout
///
/// Used for 'insurance' job type to determine % of ship value that will be reimbursed
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct ContributionMultiplier {
    /// Title of this option in the job creation menu
    pub title: LocalizedString,
    /// Description of this option in the job creation menu
    ///
    /// Shown in-game when hovering over the (i) info button
    pub description: LocalizedString,
    /// Placeholder text if no value is set in the input
    pub unsetDescription: LocalizedString,
    /// Icon for this option
    pub iconID: String,
    /// Default value set in this option
    pub defaultValue: f64,
    /// Maximum value allowed for this option
    pub maxValue: f64,
    /// Minimum value allowed for this option
    pub minValue: f64,
}

/// Standard contribution option
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct ContributionInfo {
    /// Title of this option in the job creation menu
    pub title: LocalizedString,
    /// Description of this option in the job creation menu
    ///
    /// Shown in-game when hovering over the (i) info button
    pub description: LocalizedString,
    /// Placeholder text if no value is set in the input
    pub unsetDescription: LocalizedString,
    /// Icon for this option
    pub iconID: String
}

/// Parameters (other options)
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct JobSchemaParameter {
    /// Parameter name
    ///
    /// Parameter type is stored in one of the fields below
    #[serde(rename="_key")]
    pub parameter_type: String,
    /// Boolean-type parameter
    pub boolean: Option<JobSchemaParameterBoolean>,
    /// Delivery-type parameter
    pub itemDelivery: Option<JobSchemaParameterItemDelivery>,
    /// Matcher-type parameter
    ///
    /// Restricts in which places
    pub matcher: Option<JobSchemaParameterMatcher>,
}

impl InlineEntry<String> for JobSchemaParameter {
    fn key(&self) -> String {
        self.parameter_type.clone()
    }
}

/// Title and description for a boolean job schema parameter
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct JobSchemaParameterBooleanOption {
    pub title: LocalizedString,
    pub description: LocalizedString,
}

/// Boolean-type parameter
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct JobSchemaParameterBoolean {
    pub choiceLabel: LocalizedString,
    pub default: bool,
    pub description: LocalizedString,
    pub iconID: String,
    pub optionFalse: JobSchemaParameterBooleanOption,
    pub optionTrue: JobSchemaParameterBooleanOption,
    pub title: LocalizedString
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct JobSchemaParameterItemDeliveryLocation {
    pub acceptedValueTypes: Vec<String>,
    pub description: LocalizedString,
    pub iconID: String,
    pub maxEntries: f64,
    pub title: LocalizedString,
    pub unsetDescription: LocalizedString
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct JobSchemaParameterItemDeliveryInventoryType {
    pub acceptedValueTypes: Vec<String>,
    pub description: LocalizedString,
    pub iconID: String,
    pub title: LocalizedString,
    pub unsetDescription: LocalizedString
}

/// Delivery-type parameter
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct JobSchemaParameterItemDelivery {
    pub deliveryLocation: JobSchemaParameterItemDeliveryLocation,
    pub description: LocalizedString,
    pub iconID: String,
    pub inventoryType: JobSchemaParameterItemDeliveryInventoryType,
    pub title: LocalizedString
}

/// Matcher-type parameter
///
/// Restricts in which places
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct JobSchemaParameterMatcher {
    pub acceptedValueTypes: Vec<String>,    // TODO: Turn into enumset?
    pub description: LocalizedString,
    pub iconID: String,
    pub maxEntries: f64,
    pub optional: bool,
    pub title: LocalizedString,
    #[serde(rename="type")]
    pub matcher_type: String,
    pub unsetDescription: LocalizedString
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="graphics"))]
pub struct GraphicMaterialSet {
    /// ID for this material set
    #[serde(rename="_key")]
    pub materialSetID: ids::MaterialSetID,
    pub description: String,
    pub colorHull: Option<ARGB>,
    pub colorPrimary: Option<ARGB>,
    pub colorSecondary: Option<ARGB>,
    pub colorWindow: Option<ARGB>,
    pub custommaterial1: Option<String>,
    pub custommaterial2: Option<String>,
    pub material1: Option<String>,
    pub material2: Option<String>,
    pub material3: Option<String>,
    pub material4: Option<String>,
    pub resPathInsert: Option<String>,
    pub sofFactionName: Option<String>,
    pub sofPatternName: Option<String>,
    pub sofRaceHint: Option<String>,
}

impl_map_collect!(ids::MaterialSetID, GraphicMaterialSet, materialSetID);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct ARGB {
    /// Alpha channel, in range 0-1
    pub a: f64,
    /// Red channel, in range 0-1
    pub r: f64,
    /// Green channel, in range 0-1
    pub g: f64,
    /// Blue channel, in range 0-1
    pub b: f64,
}

/// 3D Graphics information, such as metadata for models+textures
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="graphics"))]
pub struct Graphic {
    /// ID for this graphic
    #[serde(rename="_key")]
    pub graphicID: ids::GraphicID,
    /// For graphics consisting of a single file, SharedCache resource
    pub graphicFile: Option<values::CacheResource>,
    /// Folder for icons (e.g. for ships)
    pub iconFolder: Option<String>,
    /// Faction of this object, used for base colour
    pub sofFactionName: Option<String>,
    /// Hull name for a ship, drone, etc. Determines model
    pub sofHullName: Option<String>,
    /// ???
    #[serde(default)]
    pub sofLayout: Vec<String>,
    /// Matersial set for model
    pub sofMaterialSetID: Option<ids::MaterialSetID>,
    /// Faction of this object, used for various effect colours
    pub sofRaceName: Option<String>,
}

impl_map_collect!(ids::GraphicID, Graphic, graphicID);


/// Item-type Group
///
/// Each [`Type`] is part of a parent Group
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="groups"))]
pub struct Group {
    /// ID for this group
    #[serde(rename="_key")]
    pub groupID: ids::GroupID,
    /// Whether types in this group are anchorable (e.g. Deployables, player structures, etc)
    pub anchorable: bool,
    /// Whether types in this group are always anchored (e.g. Stations, which do not exist in unanchored form)
    pub anchored: bool,
    /// Parent category of this group
    pub categoryID: ids::CategoryID,
    /// Whether this type can be fitted to ship in "non-singleton" ("repackaged"/stackable) form, true for types like ammo
    pub fittableNonSingleton: bool,
    /// Icon of this group
    pub iconID: Option<ids::IconID>,
    /// Name of this group
    pub name: LocalizedString,
    /// Whether this group is visible to players in-game
    pub published: bool,
    /// ???
    pub useBasePrice: bool,
}

impl_map_collect!(ids::GroupID, Group, groupID);


/// Icon and images
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="icons"))]
pub struct Icon {
    /// ID for this icon
    #[serde(rename="_key")]
    pub iconID: ids::IconID,
    /// Cache resource for this icon
    pub iconFile: values::CacheResource
}

impl_map_collect!(ids::IconID, Icon, iconID);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="industryActivities"))]
pub struct IndustryActivity {
    #[serde(rename="_key")]
    pub industryActivityID: ids::IndustryActivityID,
    pub name: String,
    pub description: String
}

impl_map_collect!(ids::IndustryActivityID, IndustryActivity, industryActivityID);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="industryActivities"))]
pub struct IndustryAssemblyLine {
    #[serde(rename="_key")]
    pub industryAssemblyLineID: ids::IndustryAssemblyLineID,
    pub name: String,
    pub description: Option<String>,
    pub activityID: ids::IndustryActivityID,
    pub baseCostMultiplier: Option<f64>,
    pub baseMaterialMultiplier: f64,
    pub baseTimeMultiplier: f64,
    pub detailsPerCategory: Option<Vec<IndustryAssemblyLineDetailsPerCategory>>,
    pub detailsPerGroup: Option<Vec<IndustryAssemblyLineDetailsPerGroup>>,
    pub detailsPerTypeList: Option<Vec<IndustryAssemblyLineDetailsPerTypeList>>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct IndustryAssemblyLineDetailsPerCategory {
    pub categoryID: ids::CategoryID,
    pub costMultiplier: Option<f64>,
    pub materialMultiplier: f64,
    pub timeMultiplier: f64,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct IndustryAssemblyLineDetailsPerGroup {
    pub groupID: ids::GroupID,
    pub costMultiplier: Option<f64>,
    pub materialMultiplier: f64,
    pub timeMultiplier: f64,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct IndustryAssemblyLineDetailsPerTypeList {
    pub typeListID: ids::TypeListID,
    pub costMultiplier: Option<f64>,
    pub materialMultiplier: f64,
    pub timeMultiplier: f64,
}

impl_map_collect!(ids::IndustryAssemblyLineID, IndustryAssemblyLine, industryAssemblyLineID);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="industryInstallationTypes"))]
pub struct IndustryInstallationType {
    #[serde(rename="_key")]
    pub typeID: ids::TypeID,
    #[serde(deserialize_with="deserialize_industry_installation_type_assemblylines")]
    pub assemblyLines: Vec<ids::IndustryAssemblyLineID>
}

impl_map_collect!(ids::TypeID, IndustryInstallationType, typeID);

fn deserialize_industry_installation_type_assemblylines<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<ids::IndustryAssemblyLineID>, D::Error> {
    struct SeqVisitor;
    impl<'de> Visitor<'de> for SeqVisitor {
        type Value = Vec<ids::IndustryAssemblyLineID>;

        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            formatter.write_str("array of industry assemblyline IDs")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error> where A: SeqAccess<'de> {
            #[derive(Debug, Deserialize)]
            #[allow(non_snake_case)]
            #[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
            struct AssemblyLineInfo {
                assemblyLineID: ids::IndustryAssemblyLineID
            }

            let size_hint = seq.size_hint();
            let mut vec = size_hint.map(Vec::with_capacity).unwrap_or_else(Vec::new);
            while let Some(value) = seq.next_element::<AssemblyLineInfo>()? {
                vec.push(value.assemblyLineID)
            }
            Ok(vec)
        }
    }

    deserializer.deserialize_seq(SeqVisitor)
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="industryModifierSources"))]
pub struct IndustryModifierSource {
    #[serde(rename="_key")]
    pub typeID: ids::TypeID,
    pub copying: Option<IndustryModifier>,
    pub invention: Option<IndustryModifier>,
    pub manufacturing: Option<IndustryModifier>,
    pub reaction: Option<IndustryModifier>,
    pub researchMaterial: Option<IndustryModifier>,
    pub researchTime: Option<IndustryModifier>,
}

impl_map_collect!(ids::TypeID, IndustryModifierSource, typeID);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct IndustryModifier {
    #[serde(default)]
    pub cost: Vec<IndustryModifierAttribute>,
    #[serde(default)]
    pub time: Vec<IndustryModifierAttribute>,
    #[serde(default)]
    pub material: Vec<IndustryModifierAttribute>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct IndustryModifierAttribute {
    pub dogmaAttributeID: ids::AttributeID,
    pub filterID: Option<ids::IndustryFilterID>
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="industryTargetFilters"))]
pub struct IndustryTargetFilter {
    #[serde(rename="_key")]
    pub filterID: ids::IndustryFilterID,
    pub name: String,
    #[serde(default)]
    pub categoryIDs: Vec<ids::CategoryID>,
    #[serde(default)]
    pub groupIDs: Vec<ids::GroupID>,
}

impl_map_collect!(ids::IndustryFilterID, IndustryTargetFilter, filterID);

/// Landmark in the game world
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="landmarks"))]
pub struct Landmark {
    /// ID for this landmark
    #[serde(rename="_key")]
    pub landmarkID: ids::LandmarkID,
    /// Name of the landmark, shown in-game on the map and on navigation beacons
    pub name: LocalizedString,
    /// Description of this landmark, shown in-game on the map or as show-info on the object.
    pub description: LocalizedString,
    /// Icon for this landmark
    pub iconID: Option<ids::IconID>,
    /// Location of this landmark, currently always a solarsystemID
    pub locationID: Option<ids::LocationID>,
    /// Position of the landmark relative to the cluster map origin
    pub position: MapPosition
}

impl_map_collect!(ids::LandmarkID, Landmark, landmarkID);


#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="linkWithShip"))]
pub struct LinkWithShip {
    /// TypeID of the linking object
    #[serde(rename = "_key")]
    pub typeID: ids::TypeID,
    pub applyPvpFlag: bool,
    pub canRelink: bool,
    #[serde(deserialize_with="deserialize_explicit_entry_map")]
    pub dbuffs: IndexMap<ids::DynamicBuffID, f64>,
    /// Activation cost, known as 'Complex Encryption Qubits' in-game
    pub characterEnergyCost: Option<f64>,
    /// Duration in seconds until Link completes
    ///
    /// Total effect duration is `linkDuration` + `dbuffPostLinkDuration`
    pub linkDuration: u32,
    /// Duration in seconds that `dbuffs` remain in effect after Link completes
    ///
    /// Total effect duration is `linkDuration` + `dbuffPostLinkDuration`
    pub dbuffPostLinkDuration: u32,
    /// Unused
    pub generateCynoInhibitor: bool,
    /// ??? (Used for skyhook theft, but not the full `linkDuration` is retained?)
    pub keepDbuffDurationOnLinkBreak: bool,
    pub linkEffectGraphicIDOverride: ids::GraphicID,
    pub linkableShipTypeListID: ids::TypeListID,
    /// Link range in metres
    pub maxLinkRange: f64,
    pub omegaOnly: bool,
    /// Cost to Solarsystem 'Interference'
    pub solarsystemInterferenceCost: Option<f64> // TODO: Document how system interference numbers work
}

impl_map_collect!(ids::TypeID, LinkWithShip, typeID);


/// Asteroid belt
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="mapAsteroidBelts"))]
pub struct AsteroidBelt {
    /// ID for this asteroid belt, unique for each belt in the game
    #[serde(rename="_key")]
    pub asteroidBeltID: ids::AsteroidBeltID,
    /// Unique name of this asteroid belt. If absent, uses generated name. See [`AsteroidBelt::name()`]
    pub uniqueName: Option<LocalizedString>,
    /// Celestial ID of the planet or moon this belt orbits
    pub orbitID: ids::ItemID,
    /// Index of the planet this asteroid belt orbits, starting at 1 for the first planet
    pub celestialIndex: u32,
    /// Index this asteroid belt within the planet, starting at 1. As shown in the name of "[planet] - Asteroid Belt N" in-game.
    ///
    /// Counts separately from moons and stations. There can be both a moon with orbitindex N and an asteroidbelt with orbitindex N around the same planet
    pub orbitIndex: u32,
    /// In-system position of this asteroid belt
    pub position: CelestialPosition,
    /// Radius of this asteroid belt
    ///
    /// Generally set to `1` for asteroid belts, has no correlation to actual on-grid size of the asteroid belt
    pub radius: Option<f64>,
    /// [`SolarSystem`] this asteroidbelt is in
    pub solarSystemID: ids::SolarSystemID,
    /// Additional celestial information for this asteroidbelt
    pub statistics: Option<AsteroidBeltStatistics>,
    /// TypeID of this asteroidbelt. Currently, always type `15 - Asteroid Belt`
    pub typeID: ids::TypeID,
}

impl AsteroidBelt {
    /// Return or generate the name of this asteroidbelt
    pub fn name<'a, F: FnOnce(ids::ItemID) -> &'a LocalizedString>(&self, celestial_name: F) -> LocalizedString {
        if let Some(name) = &self.uniqueName {
            name.clone()
        } else {
            let planet_name = celestial_name(self.orbitID);

            LocalizedString {
                en: format!("{} - Asteroid Belt {}", planet_name.en, self.orbitIndex),
                de: planet_name.de.as_ref().map(|planet_name| format!("{} - Asteroid Belt {}", planet_name, self.orbitIndex)),
                es: planet_name.es.as_ref().map(|planet_name| format!("{} - Cinturón de asteroides {}", planet_name, self.orbitIndex)),
                fr: planet_name.fr.as_ref().map(|planet_name| format!("{} - Ceinture d'astéroïdes {}", planet_name, self.orbitIndex)),
                ja: planet_name.ja.as_ref().map(|planet_name| format!("{} - アステロイドベルト {}", planet_name, self.orbitIndex)),
                ko: planet_name.ko.as_ref().map(|planet_name| format!("{} - 소행성 벨트 {}", planet_name, self.orbitIndex)),
                ru: planet_name.ru.as_ref().map(|planet_name| format!("{} - Asteroid Belt {}", planet_name, self.orbitIndex)),
                zh: planet_name.zh.as_ref().map(|planet_name| format!("{} - 小行星带 {}", planet_name, self.orbitIndex)),
            }
        }
    }
}

/// Additional celestial information for an asteroidbelt
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct AsteroidBeltStatistics {
    pub density: f64,
    pub eccentricity: f64,
    pub escapeVelocity: f64,
    pub locked: bool,
    pub massGas: Option<f64>,
    pub massDust: f64,
    pub orbitPeriod: f64,
    pub orbitRadius: f64,
    pub rotationRate: f64,
    pub spectralClass: String,
    pub surfaceGravity: f64,
    pub temperature: f64
}

impl_map_collect!(ids::AsteroidBeltID, AsteroidBelt, asteroidBeltID);


/// Constellation of solarsystems
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="mapConstellations"))]
pub struct Constellation {
    /// ID for this constellation
    #[serde(rename="_key")]
    pub constellationID: ids::ConstellationID,
    /// [`Region`] this constellation belongs to
    pub regionID: ids::RegionID,
    /// Position of this constellation (approximate but not exact center), relative to the map origin
    pub position: MapPosition,
    /// Name of this constellation
    pub name: LocalizedString,
    /// Solarsystems in this constellation
    pub solarSystemIDs: Vec<ids::SolarSystemID>,
    /// Faction holding this constellation
    ///
    /// May be overridden by factionID values in [`SolarSystem`]
    /// If `None`, Faction holdings should be inherited from [`Region::factionID`]
    pub factionID: Option<ids::FactionID>,
    /// Wormhole class for this constellation
    ///
    /// May be overridden by wormholeClassID values in [`SolarSystem`]
    /// If `None`, WH class should be inherited from [`Region::wormholeClassID`]
    pub wormholeClassID: Option<ids::WormholeClassID>
}

impl_map_collect!(ids::ConstellationID, Constellation, constellationID);

/// Moon
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="mapMoons"))]
pub struct Moon {
    /// ID for this moon
    #[serde(rename="_key")]
    pub moonID: ids::MoonID,
    /// Unique name of this moon. If absent, uses generated name. See [`Moon::name()`]
    pub uniqueName: Option<LocalizedString>,
    /// [`NpcStation`]s orbiting this moon
    #[serde(default)]
    pub npcStationIDs: Vec<ids::StationID>,
    /// CelestialID (PlanetID) of the planet this moon orbits
    pub orbitID: ids::ItemID,
    /// Index of the planet this moon orbits, starting at 1 for the first planet
    pub celestialIndex: u32,
    /// Index this moon within the planet, starting at 1. As shown in the name of "[planet] - Moon N" in-game.
    ///
    /// Counts separately from asteroidbelts and stations. There can be both a moon with orbitindex N and an asteroidbelt with orbitindex N around the same planet
    pub orbitIndex: u32,
    /// In-system position of this moon
    pub position: CelestialPosition,
    /// Moon radius
    pub radius: f64,
    /// Solarsystem this moon is located in
    pub solarSystemID: ids::SolarSystemID,
    /// TypeID for moon object
    pub typeID: ids::TypeID,
    /// Additional celestial information for this moon
    pub statistics: Option<MoonStatistics>,
    /// Moon 3D model information
    pub attributes: MoonAttributes,
}

impl Moon {
    /// Return or generate the name of this moon
    pub fn name<'a, E, F: FnOnce(ids::ItemID) -> Result<&'a LocalizedString, E>>(&self, celestial_name: F) -> Result<LocalizedString, E> {
        if let Some(name) = &self.uniqueName {
            Ok(name.clone())
        } else {
            let planet_name = celestial_name(self.orbitID)?;

            Ok(LocalizedString {
                en: format!("{} - Moon {}", planet_name.en, self.orbitIndex),
                de: planet_name.de.as_ref().map(|planet_name| format!("{} - Moon {}", planet_name, self.orbitIndex)),
                es: planet_name.es.as_ref().map(|planet_name| format!("{} - Luna {}", planet_name, self.orbitIndex)),
                fr: planet_name.fr.as_ref().map(|planet_name| format!("{} - Lune {}", planet_name, self.orbitIndex)),
                ja: planet_name.ja.as_ref().map(|planet_name| format!("{} - 衛星 {}", planet_name, self.orbitIndex)),
                ko: planet_name.ko.as_ref().map(|planet_name| format!("{} - 위성 {}", planet_name, self.orbitIndex)),
                ru: planet_name.ru.as_ref().map(|planet_name| format!("{} - Moon {}", planet_name, self.orbitIndex)),
                zh: planet_name.zh.as_ref().map(|planet_name| format!("{} - 卫星 {}", planet_name, self.orbitIndex)),
            })
        }
    }
}

/// Additional celestial information for a moon
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct MoonStatistics {
    pub density: f64,
    pub eccentricity: f64,
    pub escapeVelocity: f64,
    pub locked: bool,
    pub massDust: f64,
    pub massGas: Option<f64>,
    pub pressure: f64,
    pub orbitPeriod: f64,
    pub orbitRadius: f64,
    pub rotationRate: f64,
    pub spectralClass: String,
    pub surfaceGravity: f64,
    pub temperature: f64
}

/// Moon 3D model information
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct MoonAttributes {
    pub heightMap1: u32,
    pub heightMap2: u32,
    pub shaderPreset: u32
}

impl_map_collect!(ids::MoonID, Moon, moonID);


/// Planet
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="mapPlanet"))]
pub struct Planet {
    /// ID for this planet
    #[serde(rename="_key")]
    pub planetID: ids::PlanetID,
    /// Unique name of this planet. If absent, uses generated name. See [`Planet::name()`]
    pub uniqueName: Option<LocalizedString>,
    /// [`AsteroidBelt`]s orbiting this planet
    #[serde(default)]
    pub asteroidBeltIDs: Vec<ids::AsteroidBeltID>,
    /// NPC stations orbiting this planet
    #[serde(default)]
    pub npcStationIDs: Vec<ids::StationID>,
    /// [`Moon`]s orbiting this planet
    #[serde(default)]
    pub moonIDs: Vec<ids::MoonID>,
    /// Planet 3D model information
    pub attributes: PlanetAttributes,
    /// Index of this planet, starting at 1 for the first planet
    pub celestialIndex: u32,
    /// CelestialID (StarID) of the star this moon orbits
    pub orbitID: Option<ids::ItemID>,
    /// In-system position of this planet
    pub position: CelestialPosition,
    /// Planet radius
    pub radius: f64,
    /// Solarsystem this planet is located in
    pub solarSystemID: ids::SolarSystemID,
    /// Additional celestial information for this planet
    pub statistics: PlanetStatistics,
    /// TypeID for planet object
    pub typeID: ids::TypeID,
}

impl Planet {
    /// Return or generate the name of this planet
    pub fn name<'a, E, F: FnOnce(ids::SolarSystemID) -> Result<&'a LocalizedString, E>>(&self, system_name: F) -> Result<LocalizedString, E> {
        if let Some(name) = &self.uniqueName {
            Ok(name.clone())
        } else {
            let star_name = system_name(self.solarSystemID)?;

            let number = match self.celestialIndex {
                1 => "I", 2 => "II", 3 => "III", 4 => "IV", 5 => "V", 6 => "VI", 7 => "VII", 8 => "VIII", 9 => "IX",
                10 => "X", 11 => "XI", 12 => "XII", 13 => "XIII", 14 => "XIV", 15 => "XV", 16 => "XVI", 17 => "XVII", 18 => "XVIII", 19 => "XIX",
                20 => "XX", 21 => "XXI", 22 => "XXII", 23 => "XXIII", 24 => "XXIV", 25 => "XXV", 26 => "XXVI", 27 => "XXVII", 28 => "XXVIII", 29 => "XXIX",
                _ => {
                    debug_assert!(false, "Planet celestialIndex out of range!");
                    &format!("{}", self.celestialIndex)
                }
            };

            Ok(LocalizedString {
                en: format!("{} - Moon {}", star_name.en, number),
                de: star_name.de.as_ref().map(|star_name| format!("{} {}", star_name, number)),
                es: star_name.es.as_ref().map(|star_name| format!("{} {}", star_name, number)),
                fr: star_name.fr.as_ref().map(|star_name| format!("{} {}", star_name, number)),
                ja: star_name.ja.as_ref().map(|star_name| format!("{} {}", star_name, number)),
                ko: star_name.ko.as_ref().map(|star_name| format!("{} {}", star_name, number)),
                ru: star_name.ru.as_ref().map(|star_name| format!("{} {}", star_name, number)),
                zh: star_name.zh.as_ref().map(|star_name| format!("{} {}", star_name, number)),
            })
        }
    }
}

/// Additional celestial information for a planet
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct PlanetStatistics {
    pub density: f64,
    pub eccentricity: f64,
    pub escapeVelocity: f64,
    pub locked: bool,
    pub massDust: f64,
    pub massGas: Option<f64>,
    pub pressure: f64,
    pub orbitPeriod: Option<f64>,
    pub orbitRadius: Option<f64>,
    pub rotationRate: f64,
    pub spectralClass: String,
    pub surfaceGravity: Option<f64>,
    pub temperature: f64
}

/// Planet 3D model information
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct PlanetAttributes {
    pub heightMap1: u32,
    pub heightMap2: u32,
    pub population: bool,
    pub shaderPreset: u32
}

impl_map_collect!(ids::PlanetID, Planet, planetID);


/// Region of constellations
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="mapRegions"))]
pub struct Region {
    /// ID for this region
    #[serde(rename="_key")]
    pub regionID: ids::RegionID,
    /// Name of this region
    pub name: LocalizedString,
    /// Description for this region, shown in-game in the showinfo menu
    pub description: Option<LocalizedString>,
    /// Constellations in this region
    pub constellationIDs: Vec<ids::ConstellationID>,
    /// Faction holding this region
    ///
    /// May be overridden by factionID values in [`Constellation`] or [`SolarSystem`]
    pub factionID: Option<ids::FactionID>,
    /// Background skybox nebula for this region
    pub nebulaID: u32,    // TODO: Assign ID type
    /// Position of this region (approximate but not exact center), relative to the map origin
    pub position: MapPosition,
    /// Wormhole class of this region
    ///
    /// May be overridden by wormholeClassID values in [`Constellation`] or [`SolarSystem`]
    pub wormholeClassID: Option<ids::WormholeClassID>
}

impl_map_collect!(ids::RegionID, Region, regionID);


/// Wormhole effect "2nd star" (Red Giant, Magnetar, etc)
///
/// Consists of both a star object and an effect beacon. The star object is the same for all wormholes of the same type, while the effect beacon differs with the class and type of the wormhole.
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="mapSecondarySuns"))]
pub struct SecondarySun {
    /// CelestialID for the star object
    #[serde(rename="_key")]
    pub celestialID: ids::ItemID,
    /// TypeID of the effect beacon
    pub effectBeaconTypeID: ids::TypeID,
    /// TypeID of the star object
    pub typeID: ids::TypeID,
    /// Position of the star object
    pub position: CelestialPosition,
    /// Solarsystem this effect applies to
    pub solarSystemID: ids::SolarSystemID,
}

impl FromIterator<SecondarySun> for IndexMap<ids::SolarSystemID, SecondarySun> {
    fn from_iter<T: IntoIterator<Item=SecondarySun>>(iter: T) -> Self {
        IndexMap::from_iter(iter.into_iter().map(|s| (s.solarSystemID, s)))
    }
}


/// Solarsystem, a single in-game star system
///
/// Terminology note: "SolarSystem" is the term for star systems within EVE Online third party development. Players usually use the term "system"
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="mapSolarSystems"))]
pub struct SolarSystem {
    /// ID for this solarsystem
    #[serde(rename="_key")]
    pub solarSystemID: ids::SolarSystemID,
    /// Constellation this solarsystem is a part of
    pub constellationID: ids::ConstellationID,
    /// Region this solarsystem is a part of
    pub regionID: ids::RegionID,
    /// Item-type [`Category`]-ies that cannot be anchored in this system
    #[serde(default)]
    pub disallowedAnchorCategories: Vec<ids::CategoryID>,
    /// Item-type [`Groups`]s that cannot be anchored in this system
    #[serde(default)]
    pub disallowedAnchorGroups: Vec<ids::GroupID>,
    /// Faction holding this solarsystem
    ///
    /// If `None`, Faction holdings should be inherited from [`Constellation::factionID`] and [`Region::factionID`]
    pub factionID: Option<ids::FactionID>,
    /// Solarsystem name
    pub name: LocalizedString,
    /// [`Planet`]s in this system
    #[serde(default)]
    pub planetIDs: Vec<ids::PlanetID>,
    /// [`Stargate`]s in this system
    #[serde(default)]
    pub stargateIDs: Vec<ids::StargateID>,
    /// (3D) position of this solarsystem
    ///
    /// See https://developers.eveonline.com/docs/guides/map-data/
    pub position: MapPosition,
    /// Position of this solarsystem on the 2D map. Only the k-space systems have a 2d position
    ///
    /// See https://developers.eveonline.com/docs/guides/map-data/
    pub position2D: Option<Position2D>,
    /// Solarsystem radius
    ///
    /// Caution: Does not reflect actual in-system size
    pub radius: f64,
    pub securityClass: Option<String>,
    /// Numerical security status,
    pub securityStatus: f64,
    /// [`Star`] of this solarsystem; Only certain special event or instance-host systems do not have a star. All regular k-space and wh-space systems have stars.
    pub starID: Option<ids::StarID>,
    /// Star luminosity
    pub luminosity: Option<f64>,
    /// Additional visual effect for this system
    pub visualEffect: Option<String>,

    /// Whether this solarsystem counts as a border system
    pub border: Option<bool>,
    /// Whether this solarsystem counts as a corridor system
    pub corridor: Option<bool>,
    /// Whether this solarsystem counts as a fringe system
    pub fringe: Option<bool>,
    /// Whether this solarsystem counts as a hub system
    pub hub: Option<bool>,
    /// Whether this solarsystem counts as an international system
    pub international: Option<bool>,
    /// Whether this solarsystem counts as an regional system
    pub regional: Option<bool>,

    /// Wormhole class for this solarsystem
    ///
    /// If `None`, WH class should be inherited from [`Constellation::wormholeClassID`] and [`Region::wormholeClassID`]
    pub wormholeClassID: Option<ids::WormholeClassID>,
}

impl SolarSystem {
    /// True if this solarsystem has "highsec" security status
    pub fn is_highsec(&self) -> bool {
        self.securityStatus >= 0.45
    }

    /// True if this solarsystem has "lowsec" security status
    pub fn is_lowsec(&self) -> bool {
        self.securityStatus > 0.0 && !self.is_highsec()
    }

    /// True if this solarsystem has "nullsec" (both sovnull, npc-null, and wormhole/abyssal space) security status
    pub fn is_nullsec(&self) -> bool {
        self.securityStatus <= 0.0
    }


    /// Returns rounded security status, as displayed ingame
    ///
    /// # Arguments
    ///
    /// * `force_sign`: If true, include a `+` sign for positive security status
    ///
    /// returns: String
    pub fn security_text(&self, force_sign: bool) -> String {
        #[allow(unused_parens)] // compiler bug; https://github.com/rust-lang/rust/issues/120737
        let (negative, n, decimal) = if matches!(self.securityStatus, (..=0.0 | 0.05..)) {
            let n = (self.securityStatus * 10.0).round() as i8;
            (self.securityStatus < 0.0, i8::unsigned_abs(n / 10), i8::unsigned_abs(n % 10))
        } else {
            debug_assert!(self.securityStatus.is_finite() && !self.securityStatus.is_nan());
            (false, 0, 1)
        };
        if negative {
            format!("-{}.{}", n, decimal)
        } else if force_sign {
            format!("+{}.{}", n, decimal)
        } else {
            format!("{}.{}", n, decimal)
        }
    }

    /// Returns rounded security status, as displayed ingame
    pub fn security_rounded(&self) -> f64 {
        #[allow(unused_parens)] // compiler bug; https://github.com/rust-lang/rust/issues/120737
        if matches!(self.securityStatus, (..=0.0 | 0.05..)) {
            (self.securityStatus * 10.0).round() / 10.0
        } else {
            0.1
        }
    }
}

impl_map_collect!(ids::SolarSystemID, SolarSystem, solarSystemID);


/// Stargate connecting systems
///
/// Each connection has two entries, one for the stargate in each system.
///
/// Does not include player-built "Ansiblex" jump bridges
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="mapStargates"))]
pub struct Stargate {
    /// ID for this stargate
    #[serde(rename="_key")]
    pub stargateID: ids::StargateID,
    /// Solarsystem this stargate is in
    pub solarSystemID: ids::SolarSystemID,
    /// Destination of stargate
    ///
    /// Contains both the paired `StargateID` and destination `SolarSystemID`
    pub destination: StargateDestination,
    /// In-system position of this stargate
    pub position: CelestialPosition,
    /// TypeID for stargate object
    pub typeID: ids::TypeID
}

impl Stargate {
    /// Return or generate the name of this stargate
    pub fn name<'a, E, F: FnOnce(ids::SolarSystemID) -> Result<&'a LocalizedString, E>>(&self, system_name: F) -> Result<LocalizedString, E> {
        let dest_name = system_name(self.destination.solarSystemID)?;
        Ok(LocalizedString {
            en: format!("Stargate ({})", dest_name.en),
            de: Some(format!("Stargate ({})", dest_name.try_de())),
            es: Some(format!("Portal estelar ({})", dest_name.try_es())),
            fr: Some(format!("Portail stellaire ({})", dest_name.try_fr())),
            ja: Some(format!("スターゲート ({})", dest_name.try_ja())),
            ko: Some(format!("스타게이트 ({})", dest_name.try_ko())),
            ru: Some(format!("Stargate ({})", dest_name.try_ru())),
            zh: Some(format!("星门 ({})", dest_name.try_zh())),
        })
    }
}

/// Destination for a stargate, both the paired stargate and destination solarsystem
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct StargateDestination {
    pub solarSystemID: ids::SolarSystemID,
    pub stargateID: ids::StargateID
}

impl_map_collect!(ids::StargateID, Stargate, stargateID);


/// Star
///
/// most but not all systems have a central star
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="mapStars"))]
pub struct Star {
    /// ID for this star
    #[serde(rename="_key")]
    pub starID: ids::StarID,
    /// Star radius
    pub radius: f64,
    /// Solarsystem this star is in
    pub solarSystemID: ids::SolarSystemID,
    /// Additional celestial information for this star
    pub statistics: StarStatistics,
    /// TypeID of this star
    pub typeID: ids::TypeID
}

/// Additional celestial information for a star
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct StarStatistics {
    pub age: f64,
    pub life: f64,
    pub luminosity: f64,
    pub spectralClass: String,
    pub temperature: f64
}

impl_map_collect!(ids::StarID, Star, starID);


/// Market group
///
/// Market groups form a hierarchical tree, with child-groups having their "parentGroupID" field set to the marketGroupID of their parent.
///
/// All items on the market have a market group
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="marketGroups"))]
pub struct MarketGroup {
    /// ID for this market group
    #[serde(rename="_key")]
    pub marketGroupID: ids::MarketGroupID,
    /// Name, as shown in the in-game market screen
    pub name: LocalizedString,
    /// Description, shown in-game when hovering over the group name in the market screen
    pub description: Option<LocalizedString>,
    /// Icon, as shown in the in-game market screen
    pub iconID: Option<ids::IconID>,
    /// Whether this market group is assigned to item [`Type`]s or is merely a parent group of other market groups
    pub hasTypes: bool,
    /// Parent market group
    pub parentGroupID: Option<ids::MarketGroupID>
}

impl_map_collect!(ids::MarketGroupID, MarketGroup, marketGroupID);


/// Ship mastery info
#[derive(Debug)]
#[allow(non_snake_case)]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="masteries"))]
#[cfg_attr(feature="docs_export", doc_sde(override=r###"
# Ship mastery information
Mastery:
    # Ship typeID this mastery information applies to
    !!key shipID: TypeID (integer)
    levels: {
        # Mastery level, with level 1 at `0` and level 5 at `4`
        # Level "0" mastery in game are the base required skills for the ship, obtained through the "required Skill" type Attributes
        integer:
            # List of required Certificates for this level
            [CertificateID (integer)]
    }
"###))]
pub struct MasteryInfo {
    /// Certificates required to reach level 1 mastery
    ///
    /// Certificates must be at 'basic' level
    pub lvl1: Vec<ids::CertificateID>,
    /// Certificates required to reach level 2 mastery
    ///
    /// Certificates must be at 'standard' level
    pub lvl2: Vec<ids::CertificateID>,
    /// Certificates required to reach level 3 mastery
    ///
    /// Certificates must be at 'improved' level
    pub lvl3: Vec<ids::CertificateID>,
    /// Certificates required to reach level 4 mastery
    ///
    /// Certificates must be at 'advanced' level
    pub lvl4: Vec<ids::CertificateID>,
    /// Certificates required to reach level 5 mastery
    ///
    /// Certificates must be at 'elite' level
    pub lvl5: Vec<ids::CertificateID>
}

impl<'de> Deserialize<'de> for MasteryInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> where D: Deserializer<'de> {
        struct MasteryVisitor;
        impl<'de> Visitor<'de> for MasteryVisitor {
            type Value = MasteryInfo;

            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                formatter.write_str("array of mastery levels")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error> where A: SeqAccess<'de> {
                use serde::de::Error;
                let mut levels = MasteryInfo {
                    lvl1: Vec::new(),
                    lvl2: Vec::new(),
                    lvl3: Vec::new(),
                    lvl4: Vec::new(),
                    lvl5: Vec::new(),
                };

                while let Some(ExplicitMapEntry { _key, _value }) = seq.next_element::<ExplicitMapEntry<u8, Vec<ids::CertificateID>>>()? {
                    match _key {
                        0 => levels.lvl1 = _value,
                        1 => levels.lvl2 = _value,
                        2 => levels.lvl3 = _value,
                        3 => levels.lvl4 = _value,
                        4 => levels.lvl5 = _value,
                        _ => return Err(A::Error::invalid_value(Unexpected::Other("mastery level"), &"mastery level in range 0..=4"))
                    }
                }
                Ok(levels)
            }
        }

        deserializer.deserialize_seq(MasteryVisitor)
    }
}

/// Mercendary den event
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="mercenaryTacticalOperations"))]
pub struct MercenaryTacticalOperation {
    /// ID for this operation
    #[serde(rename="_key")]
    pub operation_id: ids::DungeonID,
    /// Anarchy score modifier if this event is successfully completed
    pub anarchyImpact: i32,
    /// Development score modifier if this event is successfully completed
    pub developmentImpact: i32,
    /// Infomorph production modifier if this event is successfully completed
    pub infomorphBonus: i32,
    /// [`Dungeon`] to which this event applies
    pub dungeonID: ids::DungeonID,
    /// Operation name
    pub name: LocalizedString,
    /// Operation description
    pub description: LocalizedString
}

impl_map_collect!(ids::DungeonID, MercenaryTacticalOperation, operation_id);

/// Metagroup or "tech tier" for [`Type`]s
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="metaGroup"))]
pub struct MetaGroup {
    /// ID for this metagroup
    #[serde(rename="_key")]
    pub metaGroupID: ids::MetaGroupID,
    /// Name (As shown in-game when hovering over the tech-tier icon)
    pub name: LocalizedString,
    /// Description
    pub description: Option<LocalizedString>,
    /// Associated colour
    pub color: Option<MetaGroupColor>,
    /// IconID for the corner icon displayed on type icons
    pub iconID: Option<ids::IconID>,
    /// Icon suffix
    pub iconSuffix: Option<String>,
}

/// Colour for metagroup
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct MetaGroupColor {
    /// Red channel, in range 0-1
    pub r: f64,
    /// Green channel, in range 0-1
    pub g: f64,
    /// Blue channel, in range 0-1
    pub b: f64,
}

impl_map_collect!(ids::MetaGroupID, MetaGroup, metaGroupID);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="metenoxMoonDrill"))]
pub struct MetenoxMoonDrill {   // TODO: Maybe just make singleton?, TODO: This still doesn't specify what reagent is used T.T
    /// Drill typeID
    #[serde(rename="_key")]
    pub typeID: ids::TypeID,
    /// Cycle time in seconds
    pub miningCycleTime: u32,
    pub miningEfficiency: f64,
    pub reagentsConsumedPerCycle: u32,
}

impl_map_collect!(ids::TypeID, MetenoxMoonDrill, typeID);

/// Military Campaign
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="militaryCampaigns"))]
pub struct MilitaryCampaign {
    // ID for this Campaign
    #[serde(rename="_key")]
    pub militaryCampaignID: uuids::MilitaryCampaignID,
    pub title: LocalizedString,
    pub subtitle: LocalizedString,
    pub issuer: MilitaryCampaignIssuer, // TODO: Flatten
    /// ???
    pub targetProgress: u32,
    pub annotations: MilitaryCampaignAnnotations
}

impl_map_collect!(uuids::MilitaryCampaignID, MilitaryCampaign, militaryCampaignID);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct MilitaryCampaignIssuer {
    pub factionID: ids::FactionID
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct MilitaryCampaignAnnotations {
    pub aoCampaignCardButtonImage: String,  // TODO: Apply SharedCache Resource type alias
    pub backgroundVideoLoop: String,
    pub briefingBackground: String,
    pub briefingForeground: String,
    pub briefingMiddleground: String,
    pub dashboardAmbientBackground: String,
    pub dashboardBackground: String,
    pub dashboardForeground: String,
    pub dashboardMiddleground: String,
    pub foregroundVideoIntro: String,
    pub foregroundVideoOutro: String,
    pub foregroundVideoLoop: String,
    pub middlegroundVideoIntro: String,
    pub middlegroundVideoLoop: String,
    pub middlegroundVideoOutro: String,

    pub briefingGoalDescription: LocalizedString,
    pub briefingHeader: LocalizedString,
    pub briefingSuccessDescription: LocalizedString,
    pub briefingSuccessHeader: LocalizedString,
    pub briefingFailureDescription: LocalizedString,
    pub briefingFailureHeader: LocalizedString,
    pub briefingFinalWords: LocalizedString,
    pub finishedCampaignEnded: LocalizedString,
    pub finishedResolutionStateFailure: LocalizedString,
    pub finishedFailureDescription: LocalizedString,
    pub finishedResolutionStateSuccess: LocalizedString,
    pub finishedSuccessDescription: LocalizedString,

    pub campaignSet: String,
    pub mapFocusEntityID: ids::ItemID,
    pub mapHeader: LocalizedString,
    pub mapSection1Paragraph: LocalizedString,
    pub mapSection1Title: LocalizedString,
    pub mapSection2Paragraph: LocalizedString,
    pub mapSection2Title: LocalizedString,
    pub mapSection3Paragraph: LocalizedString,
    pub mapSection3Title: LocalizedString,
    pub mapSubheader: LocalizedString,
    pub mapTitle: LocalizedString,

    pub presentingCharacterName: LocalizedString,
    pub presentingCharacterSubtitle: LocalizedString,
    pub presentingCharacterTexturePath: String,
    pub race: MilitaryCampaignAnnotationRace,
    pub themePack: String,
    pub towCampaignCardButtonImage: String
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub enum MilitaryCampaignAnnotationRace {
    #[serde(rename="amarr")] Amarr,
    #[serde(rename="minmatar")] Minmatar,
    #[serde(rename="gallente")] Gallente,
    #[serde(rename="caldari")] Caldari,
}

/// Military Campaign Objective
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="militaryCampaignObjectives"))]
pub struct MilitaryCampaignObjective {
    // ID for this Campaign
    #[serde(rename="_key")]
    pub militaryCampaignObjectiveID: uuids::MilitaryCampaignObjectiveID,
    pub campaignID: uuids::MilitaryCampaignID,
    pub careerPath: String,
    pub contentTags: Vec<String>,
    pub issuer: MilitaryCampaignObjectiveIssuer,    // TODO: Merge with campaign issuer?
    pub maxProgressPerParticipant: u32,
    pub presentingCharacterID: ids::CharacterID,
    pub contributionMethodConfiguration: MilitaryCampaignContributionMethod,
    pub rewards: MilitaryCampaignObjectiveRewards,
    pub subtitle: LocalizedString,
    pub title: LocalizedString,
    pub targetProgress: u32,
    pub annotations: Option<MilitaryCampaignObjectiveAnnotations>
}

impl_map_collect!(uuids::MilitaryCampaignObjectiveID, MilitaryCampaignObjective, militaryCampaignObjectiveID);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct MilitaryCampaignObjectiveAnnotations {
    pub requiredEnlistmentWithFactionID: ids::FactionID,
    pub restrictionTooltip: LocalizedString,
    pub warning1: LocalizedString,
    pub warning2: Option<LocalizedString>
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct MilitaryCampaignContributionMethod {
    pub name: String,   // TODO: Enum
    pub parameters: Vec<MilitaryCampaignContributionMethodParameter>
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct MilitaryCampaignContributionMethodParameter {
    pub key: String,
    pub matcher: MilitaryCampaignContributionMethodParameterMatcher
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct MilitaryCampaignContributionMethodParameterMatcher {
    pub values: Vec<MilitaryCampaignContributionMethodParameterMatcherValue>
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct MilitaryCampaignContributionMethodParameterMatcherValue {    // TODO: Fold these, this is rather silly.
    pub valueType: String,
    #[serde(default)]
    pub values: Vec<String>
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct MilitaryCampaignObjectiveIssuer {
    pub corporationID : ids::CorporationID
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct MilitaryCampaignObjectiveRewards {
    pub isk: MilitaryCampaignObjectiveRewardQuantity,
    pub lp: MilitaryCampaignObjectiveRewardQuantity,
    pub standing: MilitaryCampaignObjectiveRewardStanding
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct MilitaryCampaignObjectiveRewardQuantity {
    pub amountPerInterval: u32,
    pub issuer: MilitaryCampaignObjectiveIssuer,
    pub progressInterval: u32,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct MilitaryCampaignObjectiveRewardStanding {
    pub gainPercentPerInterval: f64,
    pub issuer: MilitaryCampaignObjectiveStandingIssuer,
    pub progressInterval: u32
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct MilitaryCampaignObjectiveStandingIssuer {
    pub factionID: ids::FactionID
}

/// NPC Mission
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="missions"))]
pub struct Mission {
    /// MissionID for this mission
    #[serde(rename="_key")]
    pub missionID: ids::MissionID,
    pub agentTypeID: Option<ids::AgentTypeID>,
    pub corporationID: Option<ids::CorporationID>,
    pub name: LocalizedString,
    pub expirationTime: Option<u32>,    // TODO: Unit
    pub courierMission: Option<MissionCourier>,
    #[serde(default)]
    #[serde(deserialize_with="deserialize_explicit_entry_map")]
    pub extraStandings: IndexMap<ids::FactionID, f64>,
    pub factionID: Option<ids::FactionID>,
    pub hasStandingRewards: bool,
    pub initialAgentGiftQuantity: Option<u32>,
    pub initialAgentGiftTypeID: Option<ids::TypeID>,
    pub killMission: Option<MissionKill>,
    #[serde(default)]
    #[serde(deserialize_with="deserialize_mission_message_map")]
    pub messages: IndexMap<String, LocalizedString>,
    pub missionRewards: Option<MissionReward>
}

impl_map_collect!(ids::MissionID, Mission, missionID);

/// Deserialize a json-array of [`InlineEntry`]-trait values into an IndexMap
fn deserialize_mission_message_map<'de, D: Deserializer<'de>>(deserializer: D) -> Result<IndexMap<String, LocalizedString>, D::Error> {
    #[derive(Debug, Clone, Deserialize)]
    #[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
    #[cfg_attr(feature="docs_export", doc_sde(common_type))]
    pub struct LocalizedMessage {
        pub _key: String,
        /// English
        pub en: String,
        /// German
        pub de: Option<String>,
        /// Spanish
        pub es: Option<String>,
        /// French
        pub fr: Option<String>,
        /// Japanese
        pub ja: Option<String>,
        /// Korean
        pub ko: Option<String>,
        /// Russian
        pub ru: Option<String>,
        /// Chinese
        pub zh: Option<String>
    }

    struct EntryVisitor(PhantomData<String>, PhantomData<LocalizedMessage>);
    impl<'de> Visitor<'de> for EntryVisitor {
        type Value = IndexMap<String, LocalizedString>;

        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            formatter.write_str("a map encoded as array of flattened entries")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error> where A: SeqAccess<'de> {
            let size_hint = seq.size_hint();
            let mut map = size_hint.map(IndexMap::with_capacity).unwrap_or_else(IndexMap::new);
            while let Some(value) = seq.next_element::<LocalizedMessage>()? {
                map.insert(value._key, LocalizedString {
                    en: value.en,
                    de: value.de,
                    es: value.es,
                    fr: value.fr,
                    ja: value.ja,
                    ko: value.ko,
                    ru: value.ru,
                    zh: value.zh,
                });
            }
            Ok(map)
        }
    }

    deserializer.deserialize_seq(EntryVisitor(PhantomData::default(), PhantomData::default()))
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct MissionReward {
    pub reward: Option<MissionRewardType>,
    pub bonusReward: Option<MissionRewardType>,
    pub bonusTimeInterval: Option<u32>
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct MissionRewardType {
    pub rewardQuantity: Option<u32>,
    pub rewardTypeID: Option<ids::TypeID>
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct MissionCourier {
    pub objectiveQuantity: u32,
    pub objectiveSingleton: bool,
    pub objectiveTypeID: ids::TypeID
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct MissionKill {
    pub dropItemInMissionContainer: Option<u32>,
    pub dungeonID: Option<ids::DungeonID>,
    pub objectiveQuantity: Option<u32>,
    pub objectiveTypeID: Option<ids::TypeID>
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="notificationTypes"))]
pub struct NotificationType {
    /// CharacterID for this NPC
    #[serde(rename = "_key")]
    pub notificationTypeID: ids::NotificationTypeID,
    pub displayName: Option<LocalizedString>,
    pub internalName: String
}

impl_map_collect!(ids::NotificationTypeID, NotificationType, notificationTypeID);


/// NPC character
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="npcCharacters"))]
pub struct NpcCharacter {
    /// CharacterID for this NPC
    #[serde(rename="_key")]
    pub characterID: ids::CharacterID,
    /// Character name
    pub name: LocalizedString,
    /// NPC/Character "bio"
    pub description: Option<String>,
    /// NPC race
    pub raceID: ids::RaceID,
    /// Bloodline for this npc
    pub bloodlineID: ids::BloodlineID,
    /// Ancestry for this npc (Optional)
    pub ancestryID: Option<ids::AncestryID>,
    /// Corporation this npc is in
    pub corporationID: ids::CorporationID,
    /// NPC gender
    pub gender: NpcCharacterGender,
    /// Character career
    pub careerID: Option<ids::CareerID>,
    /// True if this NPC is a CEO
    pub ceo: bool,
    /// NPC location
    pub locationID: Option<ids::LocationID>,
    /// NPC school
    pub schoolID: Option<ids::SchoolID>,
    /// Skills this NPC has
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_npc_skill")]
    pub skills: Vec<ids::TypeID>,
    /// NPC speciality
    pub specialityID: Option<ids::SpecialtyID>,
    /// NPC corporation start date
    pub startDate: Option<String>,
    /// True if this NPC has a non-generated name
    ///
    /// No specific meaning as NPCs always have their names specified in the `name` field
    pub uniqueName: bool,
    /// Additional agent-specific information if this character is an agent
    pub agent: Option<NpcCharacterAgent>,
}

fn deserialize_npc_skill<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<ids::TypeID>, D::Error> {
    struct EntryVisitor;
    impl<'de> Visitor<'de> for EntryVisitor {
        type Value = Vec<ids::TypeID>;

        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            formatter.write_str("list of objects with skill `typeID` field")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error> where A: SeqAccess<'de> {
            #[derive(Debug, Deserialize)]
            #[allow(non_snake_case)]
            #[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
            struct NpcCharacterSkill {
                typeID: ids::TypeID
            }

            let size_hint = seq.size_hint();
            let mut buf = size_hint.map(Vec::with_capacity).unwrap_or_else(Vec::new);
            while let Some(value) = seq.next_element::<NpcCharacterSkill>()? {
                buf.push(value.typeID);
            }
            Ok(buf)
        }
    }

    deserializer.deserialize_seq(EntryVisitor)
}

/// Additional agent-specific information for an NPC character
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct NpcCharacterAgent {
    /// Agent type
    pub agentTypeID: ids::AgentTypeID,
    /// Agent's corporation division
    pub divisionID: ids::DivisionID,
    /// Whether this agent is a locator agent
    pub isLocator: bool,
    /// Agent level
    pub level: i32,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
#[serde(from="bool")]
pub enum NpcCharacterGender {
    Male,
    Female
}

impl From<bool> for NpcCharacterGender {
    fn from(value: bool) -> Self {
        // Gender in the SDE is encoded as a bool, true = male
        if value {
            NpcCharacterGender::Male
        } else {
            NpcCharacterGender::Female
        }
    }
}

impl_map_collect!(ids::CharacterID, NpcCharacter, characterID);

/// Division of an NPC corporation
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="npcCorporationDivisions"))]
pub struct CorporationDivision {
    /// ID for this division
    #[serde(rename="_key")]
    pub divisionID: ids::DivisionID,
    /// Division name, localized
    pub name: LocalizedString,
    /// Division name (English, unsuitable for end-user display)
    pub displayName: Option<String>,
    /// Division description
    pub description: Option<LocalizedString>,
    /// Short internal-use name, in english
    pub internalName: String,
    /// Job description for division leader
    pub leaderTypeName: LocalizedString,
}

impl_map_collect!(ids::DivisionID, CorporationDivision, divisionID);

/// NPC corporation
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="npcCorporations"))]
pub struct NpcCorporation {
    /// ID for this corporation
    #[serde(rename="_key")]
    pub corporationID: ids::CorporationID,
    /// Corporation name
    pub name: LocalizedString,
    /// Corporation ticker
    pub tickerName: String,
    /// Corporation description/bio
    pub description: Option<LocalizedString>,
    /// Corporation icon
    pub iconID: Option<ids::IconID>,
    /// Whether this corporation has been deleted (inaccessible in-game)
    pub deleted: bool,
    /// Corporation CEO
    pub ceoID: Option<ids::CharacterID>,
    /// Character races allowed in this corporation
    ///
    /// Not applicable as players cannot join NPC corporations other than the school/default one
    pub allowedMemberRaces: Option<Vec<ids::RaceID>>,
    /// Corporation trades
    #[serde(default)]
    #[serde(deserialize_with="deserialize_explicit_entry_map")]
    pub corporationTrades: IndexMap<ids::TypeID, f64>,  // TODO: Document how these values work
    /// Divisions of this corporation
    #[serde(default)]
    #[serde(deserialize_with="deserialize_inline_entry_map")]
    pub divisions: IndexMap<ids::DivisionID, NpcCorporationDivision>,
    /// Faction this corporation is a part of
    pub factionID: Option<ids::FactionID>,
    /// Primary enemy for Fwar corporations, or "lore" competitor for other corporations
    pub enemyID: Option<ids::CorporationID>,
    /// Primary ally for Fwar corporations, or "lore" business partner for other corporations
    pub friendID: Option<ids::CorporationID>,
    /// ???
    #[serde(default)]
    #[serde(deserialize_with="deserialize_explicit_entry_map")]
    pub exchangeRates: IndexMap<ids::CorporationID, f64>,
    /// ???
    pub extent: String, // TODO: Enum
    /// ???
    pub size: String,   // TODO: Enum
    /// ???
    pub sizeFactor: Option<f64>,
    /// ??? Used only for CCP admin corporations
    pub hasPlayerPersonnelManager: bool,
    /// ???
    pub initialPrice: f64,
    /// Current shareholders (Other NPC corps, lore information?)
    #[serde(default)]
    #[serde(deserialize_with="deserialize_explicit_entry_map")]
    pub investors: IndexMap<ids::CorporationID, i32>,
    /// Loyalty point trades offered by this company
    ///
    /// The table numbers are currently meaningless to third party developers. Corporation LP trades can be obtained through ESI based on the corporationID
    /// https://developers.eveonline.com/api-explorer#/operations/GetLoyaltyStoresCorporationIdOffers
    #[serde(default)]
    pub lpOfferTables: Vec<u32>,    // TODO: Assign ID type
    /// main [`CorporationActivity`]
    pub mainActivityID: Option<ids::CorporationActivityID>,
    /// secondary [`CorporationActivity`]
    pub secondaryActivityID: Option<ids::CorporationActivityID>,
    /// Corporation member limit
    pub memberLimit: i32,
    /// Minimum security
    pub minSecurity: f64,
    /// Minimum standing to join (unused)
    pub minimumJoinStanding: f64,
    /// Corporation's raceID
    pub raceID: Option<ids::RaceID>,
    /// Issued shares of this corporation (NPC shares are unused)
    pub shares: u64,
    /// Home system of this corporation
    pub solarSystemID: Option<ids::SolarSystemID>,
    /// Home station of this corporation
    pub stationID: Option<ids::StationID>,
    /// Corporation tax rate (primarily relevant for the starter NPC corps that players join)
    pub taxRate: f64,
    /// Whether this corporation uses a unique or generated name, irrelevant as corporations always have a name specified
    pub uniqueName: bool,

    /// Whether joining this corporation sends the character termination message; Deleted characters are added to the 'Doomheim' corporation
    ///
    /// Also true for several other NPC corporations, it is best to rely on the Doomheim corpID instead (`1000001`)
    pub sendCharTerminationMessage: bool,
}

/// Division of an NPC corporation
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct NpcCorporationDivision {
    /// [`CorporationDivision`] for this division
    #[serde(rename="_key")]
    pub divisionID: ids::DivisionID,
    /// Division number within the corporation, starting at 1
    pub divisionNumber: i32,
    /// Leader of this division
    pub leaderID: ids::CharacterID,
    /// ???
    pub size: i32
}

impl InlineEntry<ids::DivisionID> for NpcCorporationDivision {
    fn key(&self) -> ids::DivisionID {
        self.divisionID
    }
}

impl_map_collect!(ids::CorporationID, NpcCorporation, corporationID);

/// NPC station, not to be confused with player built Citadels/Upwell Structures, or now-removed "Outposts" & "Conquerable Stations"
///
/// For station services, see [`StationOperation::services`]
///
/// For station descriptions, see [`StationOperation::description`]
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="npcStations"))]
pub struct NpcStation {
    /// ID for this station
    #[serde(rename="_key")]
    pub stationID: ids::StationID,
    /// Station owner
    pub ownerID: ids::CorporationID,
    /// Solarsystem this station is in
    pub solarSystemID: ids::SolarSystemID,
    /// Station object typeID (determines model & undocks)
    pub typeID: ids::TypeID,
    /// Whether this station includes the operation in the name. Use [`NpcStation::name()`] to acquire name
    pub useOperationName: bool,
    /// CelestialID of the orbited moon
    pub orbitID: ids::ItemID,
    /// Index the orbited moon within the planet, starting at 1. As shown in the name of "[planet] - Moon N" in-game.
    ///
    /// Equal to the [`Moon`]'s own `orbitIndex`
    pub orbitIndex: Option<u32>,
    /// Index of the orbited planet, starting at 1 for the first planet
    pub celestialIndex: Option<u32>,
    /// Station's [`StationOperation`];
    pub operationID: ids::StationOperationID,
    /// Station position within solarsystem
    pub position: CelestialPosition,
    /// Station reprocessing efficiency
    pub reprocessingEfficiency: f64,
    /// ??? This is always invFlag 4; "Hangar"
    pub reprocessingHangarFlag: i32,
    /// Station reprocessing tax (unused?)
    pub reprocessingStationsTake: f64,
}

impl NpcStation {
    /// Return or generate the name of this station
    pub fn name<'a, 'b, 'c, E, M, O, C>(&self, celestial_name: M, operation_name: O, corporation_name: C) -> Result<LocalizedString, E>
    where
        M: FnOnce(ids::ItemID) -> Result<&'a LocalizedString, E>,
        O: FnOnce(ids::StationOperationID) -> Result<&'b LocalizedString, E>,
        C: FnOnce(ids::CorporationID) -> Result<&'c LocalizedString, E>
    {
        if self.useOperationName {
            let moon_name = celestial_name(self.orbitID)?;
            let corp_name = corporation_name(self.ownerID)?;
            let operation_name = operation_name(self.operationID)?;

            Ok(LocalizedString {
                en: format!("{} - {} {}", moon_name.en, corp_name.en, operation_name.en),
                de: Some(format!("{} - {} {}", moon_name.try_de(), corp_name.try_de(), operation_name.try_de())),
                es: Some(format!("{} - {} {}", moon_name.try_es(), corp_name.try_es(), operation_name.try_es())),
                fr: Some(format!("{} - {} {}", moon_name.try_fr(), corp_name.try_fr(), operation_name.try_fr())),
                ja: Some(format!("{} - {} {}", moon_name.try_ja(), corp_name.try_ja(), operation_name.try_ja())),
                ko: Some(format!("{} - {} {}", moon_name.try_ko(), corp_name.try_ko(), operation_name.try_ko())),
                ru: Some(format!("{} - {} {}", moon_name.try_ru(), corp_name.try_ru(), operation_name.try_ru())),
                zh: Some(format!("{} - {} {}", moon_name.try_zh(), corp_name.try_zh(), operation_name.try_zh())),
            })
        } else {
            let moon_name = celestial_name(self.orbitID)?;
            let corp_name = corporation_name(self.ownerID)?;

            Ok(LocalizedString {
                en: format!("{} - {}", moon_name.en, corp_name.en),
                de: Some(format!("{} - {}", moon_name.try_de(), corp_name.try_de())),
                es: Some(format!("{} - {}", moon_name.try_es(), corp_name.try_es())),
                fr: Some(format!("{} - {}", moon_name.try_fr(), corp_name.try_fr())),
                ja: Some(format!("{} - {}", moon_name.try_ja(), corp_name.try_ja())),
                ko: Some(format!("{} - {}", moon_name.try_ko(), corp_name.try_ko())),
                ru: Some(format!("{} - {}", moon_name.try_ru(), corp_name.try_ru())),
                zh: Some(format!("{} - {}", moon_name.try_zh(), corp_name.try_zh())),
            })
        }
    }
}

impl_map_collect!(ids::StationID, NpcStation, stationID);

/// Planet sov resource
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="planetResources"))]
pub struct PlanetResource {
    /// PlanetID to which this resource info applies
    #[serde(rename="_key")]
    pub planet_id: ids::PlanetID,
    /// Amount of power this planet provides if a Skyhook is installed
    pub power: Option<i32>, // TODO: Maybe flatten with 0?
    /// Amount of workforce this planet provides if a Skyhook is installed
    pub workforce: Option<i32>, // Ditto, Maybe flatten with 0?
    /// Reagent this planet provides if a Skyhook is installed
    pub reagent: Option<PlanetReagent>
}

/// Planet sov resource reagent info
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct PlanetReagent {
    /// Reagent typeID
    pub type_id: ids::TypeID,
    /// Amount of reagent provided per cycle
    pub amount_per_cycle: i32,
    /// Cycle period in seconds
    pub cycle_period: i32,
    /// Secured capacity; Amount of reagent that is stored in the "secured" bay
    ///
    /// The per-cycle yield is split 50/50 between the secured and unsecured bays
    pub secured_capacity: i64,
    /// Unsecured capacity; Amount of reagent that is stored in the "unsecured" bay and can be stolen
    ///
    /// The per-cycle yield is split 50/50 between the secured and unsecured bays
    pub unsecured_capacity: i64,
}

impl_map_collect!(ids::PlanetID, PlanetResource, planet_id);

/// Planetary industry schematic
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="planetSchematics"))]
pub struct PlanetSchematic {
    /// ID for this schematic
    #[serde(rename="_key")]
    pub schematicID: ids::PlanetSchematicID,
    /// Schematic name
    pub name: LocalizedString,
    /// Cycle time in seconds
    pub cycleTime: u32,
    /// Planetary industry facilities at which this schematic may be used
    ///
    /// (Named "pins" for their visual similarity to a push-pin/tack put into the planet)
    pub pins: Vec<ids::TypeID>,
    /// Input-output types
    #[serde(deserialize_with="deserialize_inline_entry_map")]
    pub types: IndexMap<ids::TypeID, PlanetSchematicType>   // This _really_ should be parsed into separate input-output mappings, but that is hard to implement with serde
}

/// Input-output type for planetary interaction
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct PlanetSchematicType {
    /// TypeID
    #[serde(rename="_key")]
    pub typeID: ids::TypeID,
    /// True if this type is an input to the [`PlanetarySchematic`]
    pub isInput: bool,
    /// Quantity required for one [`PlanetarySchematic`] cycle
    pub quantity: u32
}

impl InlineEntry<ids::TypeID> for PlanetSchematicType {
    fn key(&self) -> ids::TypeID {
        self.typeID
    }
}

impl_map_collect!(ids::PlanetSchematicID, PlanetSchematic, schematicID);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="proximityTrap"))]
pub struct ProximityTrap {
    /// Trap object
    #[serde(rename = "_key")]
    pub typeID: ids::TypeID,
    pub dbuffDuration: u32,
    #[serde(default)]
    #[serde(deserialize_with="deserialize_explicit_entry_map")]
    pub dbuffs: IndexMap<ids::DynamicBuffID, f64>,
    pub forceDecloakDuration: Option<u32>,
    pub resetDelay: Option<u32>,
    pub showPerimeterLights: bool,
    pub triggerDelay: u32,
    pub triggerFilterTypeListID: ids::TypeListID,
    pub triggerRange: f64
}

impl_map_collect!(ids::TypeID, ProximityTrap, typeID);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="races"))]
pub struct CharacterRace {
    /// ID for this character race
    #[serde(rename = "_key")]
    pub raceID: ids::RaceID,
    /// Race name
    pub name: LocalizedString,
    /// Description, shown in character-creation
    pub description: Option<LocalizedString>,
    /// Race icon
    pub iconID: Option<ids::IconID>,
    /// "Rookie Ship" / Corvette for player characters of this race
    pub shipTypeID: Option<ids::TypeID>,
    /// "Default" skills all players characters of this race already have upon starting the game
    #[serde(default)]
    #[serde(deserialize_with="deserialize_explicit_entry_map")]
    pub skills: IndexMap<ids::TypeID, values::SkillLevel>
}

impl_map_collect!(ids::RaceID, CharacterRace, raceID);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="schoolMap"))]
pub struct SchoolMap {
    #[serde(rename="_key")]
    pub mappingID: u32, // TODO: Better name
    pub schoolID: ids::SchoolID,
    pub solarSystemID: ids::SolarSystemID
}

impl_map_collect!(ids::SchoolID, ids::SolarSystemID, SchoolMap, fn |m| (m.schoolID, m.solarSystemID));

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="schools"))]
pub struct School {
    #[serde(rename="_key")]
    pub schoolID: ids::SchoolID,
    #[serde(default)]
    pub careerAgents: Vec<ids::CharacterID>,
    pub careerID: ids::CareerID,
    pub characterDescription: Option<LocalizedString>,
    pub corporationID: ids::CorporationID,
    pub description: Option<LocalizedString>,
    pub iconID: Option<ids::IconID>,
    pub isStarterSpaceSchool: Option<bool>, // TODO: Default?
    pub name: LocalizedString,
    pub raceID: ids::RaceID,
    #[serde(default)]
    pub startingStations: Vec<ids::StationID>,
    pub title: Option<LocalizedString>
}

impl_map_collect!(ids::SchoolID, School, schoolID);


/// Ship Tree Element (Used weapon type, used tank type, etc)
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="shipTreeElements"))]
pub struct ShipTreeElement {
    #[serde(rename="_key")]
    pub shipTreeElementID: ids::ShipTreeElementID,
    pub name: LocalizedString,
    pub description: LocalizedString,
    pub icon: String
}

impl_map_collect!(ids::ShipTreeElementID, ShipTreeElement, shipTreeElementID);

/// Ship Tree Faction
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="shipTreeFactions"))]
pub struct ShipTreeFaction {
    #[serde(rename="_key")]
    pub factionID: ids::FactionID,
    pub description: LocalizedString,
    pub icon: String,
    #[serde(deserialize_with="deserialize_explicit_entry_map")]
    pub elements: IndexMap<u32, ids::ShipTreeElementID>
}

impl_map_collect!(ids::FactionID, ShipTreeFaction, factionID);

/// Ship Tree Group (Ship class)
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="shipTreeGroups"))]
pub struct ShipTreeGroup {
    #[serde(rename="_key")]
    pub shipTreeGroupID: ids::ShipTreeGroupID,
    pub name: LocalizedString,
    pub description: Option<LocalizedString>,
    pub icon: String,
    pub iconLarge: String,
    pub iconSmall: String,
    pub iconSmallNPC: String,
    #[serde(default)]
    #[serde(deserialize_with="deserialize_explicit_entry_map")]
    pub elements: IndexMap<u32, ids::ShipTreeElementID>,
    #[serde(default)]
    #[serde(deserialize_with="deserialize_inline_entry_map")]
    pub preReqSkills: IndexMap<ids::FactionID, ShipTreeGroupSkills>
}

impl_map_collect!(ids::ShipTreeGroupID, ShipTreeGroup, shipTreeGroupID);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct ShipTreeGroupSkills {
    #[serde(rename="_key")]
    pub factionID: ids::FactionID,
    #[serde(deserialize_with="deserialize_inline_entry_map")]
    pub skills: IndexMap<ids::TypeID, ShipTreeGroupSkillInfo>
}

impl InlineEntry<ids::FactionID> for ShipTreeGroupSkills {
    fn key(&self) -> ids::FactionID {
        self.factionID
    }
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct ShipTreeGroupSkillInfo {
    #[serde(rename="_key")]
    pub typeID: ids::TypeID,
    pub display: bool,
    pub level: values::SkillLevel
}

impl InlineEntry<ids::TypeID> for ShipTreeGroupSkillInfo {
    fn key(&self) -> ids::TypeID {
        self.typeID
    }
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="skillPlans"))]
pub struct SkillPlan {
    pub _key: u32,  // TODO: Document
    pub careerPathID: Option<ids::CareerPathID>,
    pub name: LocalizedString,
    pub description: LocalizedString,
    pub factionID: Option<ids::FactionID>,
    pub internalName: String,
    pub npcCorporationDivision: Option<ids::DivisionID>,
    #[serde(deserialize_with = "deserialize_skillplan_milestones")]
    pub milestones: IndexMap<ids::TypeID, Option<values::SkillLevel>>,
    #[serde(deserialize_with = "deserialize_skillplan_levels")]
    pub skillRequirements: IndexMap<ids::TypeID, values::SkillLevel>,
}

fn deserialize_skillplan_levels<'de, D: Deserializer<'de>>(deserializer: D) -> Result<IndexMap<ids::TypeID, values::SkillLevel>, D::Error> {
    struct EntryVisitor;
    impl<'de> Visitor<'de> for EntryVisitor {
        type Value = IndexMap<ids::TypeID, values::SkillLevel>;

        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            formatter.write_str("array of skill level objects")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error> where A: SeqAccess<'de> {
            #[derive(Debug, Deserialize)]
            #[allow(non_snake_case)]
            #[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
            struct SkillLevel {
                pub typeID: ids::TypeID,
                pub level: values::SkillLevel
            }

            let size_hint = seq.size_hint();
            let mut map = size_hint.map(IndexMap::with_capacity).unwrap_or_else(IndexMap::new);
            while let Some(value) = seq.next_element::<SkillLevel>()? {
                map.insert(value.typeID, value.level);
            }
            Ok(map)
        }
    }

    deserializer.deserialize_seq(EntryVisitor)
}

fn deserialize_skillplan_milestones<'de, D: Deserializer<'de>>(deserializer: D) -> Result<IndexMap<ids::TypeID, Option<values::SkillLevel>>, D::Error> {
    struct EntryVisitor;
    impl<'de> Visitor<'de> for EntryVisitor {
        type Value = IndexMap<ids::TypeID, Option<values::SkillLevel>>;

        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            formatter.write_str("array of skill milestone objects")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error> where A: SeqAccess<'de> {
            #[derive(Debug, Deserialize)]
            #[allow(non_snake_case)]
            #[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
            struct SkillLevel {
                pub typeID: ids::TypeID,
                pub level: Option<values::SkillLevel>
            }

            let size_hint = seq.size_hint();
            let mut map = size_hint.map(IndexMap::with_capacity).unwrap_or_else(IndexMap::new);
            while let Some(value) = seq.next_element::<SkillLevel>()? {
                map.insert(value.typeID, value.level);
            }
            Ok(map)
        }
    }

    deserializer.deserialize_seq(EntryVisitor)
}

impl_map_collect!(u32, SkillPlan, _key);

// SKINR


/// SKINR Component Category (currently: Material, Pattern, Metallic [material]
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="skinrComponentCategories"))]
pub struct SKINRComponentCategory {
    #[serde(rename="_key")]
    pub componentCategoryID: ids::SKINRComponentCategoryID,
    pub name: String
}

impl_map_collect!(ids::SKINRComponentCategoryID, SKINRComponentCategory, componentCategoryID);


#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="skinrComponentPointValues"))]
pub struct SKINRComponentPointValues {
    pub _key: ids::SKINRComponentCategoryID,  // TODO: Document
    #[serde(rename="_value")]
    #[serde(deserialize_with="deserialize_explicit_entry_map")]
    pub points: IndexMap<ids::SKINRComponentCategoryID, u32>
}

impl_map_collect!(ids::SKINRComponentCategoryID, SKINRComponentPointValues, _key);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="skinrComponentRarities"))]
pub struct SKINRComponentRarity {
    pub _key: u32,  // TODO: Document
    pub name: LocalizedString,
    pub rank: u32
}

impl_map_collect!(u32, SKINRComponentRarity, _key);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="skinrComponents"))]
pub struct SKINRComponent {
    #[serde(rename = "_key")]
    pub componentID: ids::SKINRComponentID,
    pub category: ids::SKINRComponentCategoryID,
    #[serde(deserialize_with="deserialize_component_associated_typeids")]
    pub associatedTypeIds: IndexMap<ids::TypeID, i32>,
    pub finish: SKINRFinish,
    pub iconFile: String,
    pub name: LocalizedString,
    pub projectionTypeU: String,
    pub projectionTypeV: String,
    pub published: bool,
    pub rarity: ids::SKINRComponentRarity,
    pub resourceFile: String,
    pub sequenceBinder: SKINRSequenceBinder
}

fn deserialize_component_associated_typeids<'de, D: Deserializer<'de>>(deserializer: D) -> Result<IndexMap<ids::TypeID, i32>, D::Error> {
    struct EntryVisitor;
    impl<'de> Visitor<'de> for EntryVisitor {
        type Value = IndexMap<ids::TypeID, i32>;

        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            formatter.write_str("array of skill milestone objects")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error> where A: SeqAccess<'de> {
            #[derive(Debug, Deserialize)]
            #[allow(non_snake_case)]
            #[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
            struct Entry {
                pub typeID: ids::TypeID,
                pub licenseUsesGranted : i32
            }

            let size_hint = seq.size_hint();
            let mut map = size_hint.map(IndexMap::with_capacity).unwrap_or_else(IndexMap::new);
            while let Some(value) = seq.next_element::<Entry>()? {
                map.insert(value.typeID, value.licenseUsesGranted);
            }
            Ok(map)
        }
    }

    deserializer.deserialize_seq(EntryVisitor)
}

impl_map_collect!(ids::SKINRComponentID, SKINRComponent, componentID);


#[derive(Debug, Deserialize, Eq, PartialEq)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub enum SKINRFinish {
    Matte, Satin, Gloss
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct SKINRSequenceBinder {
    pub itemTypeID: ids::TypeID,
    pub count: u32
}


#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="skinrSlotCategories"))]
pub struct SKINRSlotCategory {
    #[serde(rename="_key")]
    pub slotCategoryID: ids::SKINRSlotCategoryID,
    pub name: String
}

impl_map_collect!(ids::SKINRSlotCategoryID, SKINRSlotCategory, slotCategoryID);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="skinrSlotConfigurations"))]
pub struct SKINRSlotConfiguration {
    pub _key: u32,  // TODO: Document
    pub allowAllShips: Option<bool>,
    pub config: Option<Vec<u32>>,
    pub name: String,
    pub priority: u32,
    pub ships: Option<Vec<ids::TypeID>>
}

impl_map_collect!(u32, SKINRSlotConfiguration, _key);


#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="skinrSlotNames"))]
pub struct SKINRSlotName {
    pub _key: u32,  // TODO: Document
    pub name: String
}

impl_map_collect!(u32, SKINRSlotName, _key);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="skinrSlots"))]
pub struct SKINRSlot {
    pub _key: u32,  // TODO: Document
    pub category: ids::SKINRSlotCategoryID,
    pub name: LocalizedString,
    pub allowedDesignComponentCategories: Vec<ids::SKINRComponentCategoryID>
}

impl_map_collect!(u32, SKINRSlot, _key);


#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="skinrSlotsToMaterials"))]
pub struct SKINRSlotsToMaterials {
    pub _key: u32,  // TODO: Document
    pub _value: Vec<SKINRSlotMaterialBinding>
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct SKINRSlotMaterialBinding {
    pub materialID: u32,
    pub slotID: u32
}

impl_map_collect!(u32, SKINRSlotsToMaterials, _key);


#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="skinrTierThresholds"))]
pub struct SKINRTierThreshold {
    pub _key: u32,  // TODO: Document
    #[serde(deserialize_with="deserialize_explicit_entry_map")]
    pub _value: IndexMap<u32, u32>
}

impl_map_collect!(u32, SKINRTierThreshold, _key);

/// Skin license item
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="skinLicenses"))]
pub struct SkinLicense {
    /// TypeID of the license item
    ///
    /// Equal to `licenseTypeID`
    #[serde(rename="_key")]
    pub typeID: ids::TypeID,
    /// TypeID of the license item
    ///
    /// Equal to `typeID`
    pub licenseTypeID: ids::TypeID,
    /// Skin duration, -1 for permanent skins
    pub duration: i32,
    /// [`Skin`] this license is for
    pub skinID: ids::SkinID,
    /// Whether skin is a "single use" (default if unspecified). Unused value
    pub isSingleUse: Option<bool>
}

impl_map_collect!(ids::TypeID, SkinLicense, typeID);

/// Skin material; The design & colours of a skin, shared between multiple ships
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="skinMaterials"))]
pub struct SkinMaterial {
    /// ID for this material
    #[serde(rename="_key")]
    pub materialID: ids::SkinMaterialID,
    /// Name for this material; Name of the skin line/series.
    pub displayName: Option<LocalizedString>,
    /// Graphics info material-set, not to be confused with `materialID` which identifies this SkinMaterial
    pub materialSetID: ids::MaterialSetID,
}

impl_map_collect!(ids::SkinMaterialID, SkinMaterial, materialID);

/// Ship skin, not to be confused with a [`SkinLicense`] item
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="skins"))]
pub struct Skin {
    /// ID for this skin
    #[serde(rename="_key")]
    pub skinID: ids::SkinID,
    /// Non-displayed name for this skin. See [`SkinMaterial::displayName`] for a skin's displayname
    pub internalName: String,
    /// [`SkinMaterial`] for this skin line
    pub skinMaterialID: ids::SkinMaterialID,
    /// Applicable ship (or structure) types
    pub types: Vec<ids::TypeID>,
    /// Whether this skin is visible on the Serenity (Chinese) server
    pub visibleSerenity: bool,
    /// Whether this skin is visible on the Tranquility server
    pub visibleTranquility: bool,
    /// Whether this skin is a structure skin
    pub isStructureSkin: Option<bool>,  // TODO: Default false
    /// CCP-specific field
    pub allowCCPDevs: bool,
}

impl_map_collect!(ids::SkinID, Skin, skinID);

/// Sovereignty Upgrade for use with the Sovereignty Hub
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="sovereigntyUpgrades"))]
pub struct SovereigntyUpgrade {
    /// TypeID for sov-upgrade item
    #[serde(rename="_key")]
    pub typeID: ids::TypeID,
    /// Exclusivity group; Multiple upgrades with the same group cannot be installed at the same time
    pub mutually_exclusive_group: String,
    /// Power consumption of this upgrade
    pub power_allocation: Option<i32>,
    /// Workforce requirement of this upgrade
    pub workforce_allocation: Option<i32>,
    /// Power production of this upgrade
    pub power_production: Option<i32>,
    /// Workforce production of this upgrade
    pub workforce_production: Option<i32>,
    /// Additional fuel required by this upgrade
    pub fuel: Option<SovereigntyUpgradeFuel>
}

/// Additional fuel required by a [`SovereigntyUpgrade`]
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct SovereigntyUpgradeFuel {
    /// Fuel typeID
    pub type_id: ids::TypeID,
    /// Upgrade onlining cost
    pub startup_cost: u32,
    /// Upgrade upkeep cost, per hour
    pub hourly_upkeep: u32
}

impl_map_collect!(ids::TypeID, SovereigntyUpgrade, typeID);

/// [`NpcStation`] operation
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="stationOperations"))]
pub struct StationOperation {
    /// ID for this operation
    #[serde(rename="_key")]
    pub operationID: ids::StationOperationID,
    /// [`CorporationActivity`] this operation is a part of
    pub activityID: ids::CorporationActivityID,
    /// Name for this operation (Shown in station names)
    pub operationName: LocalizedString,
    /// Description for this operation (Shown in station descriptions)
    pub description: Option<LocalizedString>,
    /// Available [`StationService`]s at this station
    pub services: Vec<ids::StationServiceID>,
    /// Station object typeID (determines model & undocks), per character race/faction.
    ///
    /// Only provided for the 4 major factions and Jove.
    #[serde(default)]
    #[serde(deserialize_with="deserialize_explicit_entry_map")]
    pub stationTypes: IndexMap<ids::RaceID, ids::TypeID>,
    /// ???
    pub border: f64,
    /// ???
    pub corridor: f64,
    /// ???
    pub fringe: f64,
    /// ???
    pub hub: f64,
    /// ???
    pub ratio: f64,
    /// ???
    pub manufacturingFactor: f64,
    /// ???
    pub researchFactor: f64,
}

impl_map_collect!(ids::StationOperationID, StationOperation, operationID);

/// [`NpcStation`] service
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="stationServices"))]
pub struct StationService { // TODO: Document icons somewhere
    /// ID for this service
    #[serde(rename="_key")]
    pub serviceID: ids::StationServiceID,
    /// Display name for this service
    pub serviceName: LocalizedString,
    /// Service description
    pub description: Option<LocalizedString>,
}

impl_map_collect!(ids::StationServiceID, StationService, serviceID);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="stationStandingsRestrictions"))]
pub struct StationStandingsRestriction {
    /// Faction this restriction applies to
    #[serde(rename="_key")]
    pub factionID: ids::FactionID,
    #[serde(deserialize_with="deserialize_explicit_entry_map")]
    pub services: IndexMap<ids::StationServiceID, f64>
}

impl_map_collect!(ids::FactionID, StationStandingsRestriction, factionID);


#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="systemDbuffEmitters"))]
pub struct SystemDynamicbuffEmitter {
    #[serde(rename="_key")]
    pub typeID: ids::TypeID,
    #[serde(deserialize_with="deserialize_explicit_entry_map")]
    pub dbuffs: IndexMap<ids::DynamicBuffID, f64>,
    pub duration: u32,
    pub excludeProtected: bool,
    pub interval: u32
}

impl_map_collect!(ids::TypeID, SystemDynamicbuffEmitter, typeID);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="systemWideEffects"))]
pub struct SystemWideEffect {
    #[serde(rename="_key")]
    pub typeID: ids::TypeID,
    #[serde(default)]
    #[serde(deserialize_with="deserialize_explicit_entry_map")]
    pub dbuffs: IndexMap<ids::DynamicBuffID, f64>,
    pub eligibleTypeListID: Option<ids::TypeListID>,
    pub environmentTypeID: Option<ids::TypeID>
}

impl_map_collect!(ids::TypeID, SystemWideEffect, typeID);

/// A language the game officially is translated for
///
/// This SDE library handles translated strings through the [`LocalizedString`] type
#[derive(Debug, Deserialize, Hash, Eq, PartialEq)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="translationLanguages"))]
pub struct TranslationLanguage {
    /// Short name (ISO 639 code)
    #[serde(rename="_key")]
    pub shortName: String,
    /// Full name
    pub name: String
}

/// Ship & effect beacon bonuses
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="typeBonus"))]
pub struct TypeBonuses {
    /// Item [`Type`] this bonus info is for
    #[serde(rename="_key")]
    pub typeID: ids::TypeID,
    /// Icon for projected bonuses (e.g. Incursion or wormhole effects)
    pub iconID: Option<ids::IconID>,
    /// Skill bonuses, per level
    ///
    /// Displayed first
    #[serde(default)]
    #[serde(rename = "types")]
    #[serde(deserialize_with="deserialize_explicit_entry_map")]
    pub skillBonuses: IndexMap<ids::TypeID, Vec<TypeBonus>>,
    /// Misc bonuses, used for effect beacons and T3 Destroyers
    ///
    /// Currently mutually exclusive with role bonuses, displayed after skill bonuses
    #[serde(default)]
    pub miscBonuses: Vec<TypeBonus>,
    /// Ship role bonuses
    ///
    /// Currently mutually exclusive with misc bonuses, displayed after skill bonuses
    #[serde(default)]
    pub roleBonuses: Vec<TypeBonus>,
}

/// Single bonus
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct TypeBonus {
    /// Bonus importance
    pub importance: i32,
    /// Bonus text
    ///
    /// NOTE: This contains EVE-HTML markup, and needs processing before being suitable for display
    pub bonusText: LocalizedString,
    /// Bonus amount, percentage
    pub bonus: Option<f64>,
    /// Bonus [`EVEUnit`]
    pub unitID: Option<EVEUnit>,
    /// Whether this bonus is considered beneficial (Used primarily for effect beacons, which may have detrimental effects)
    pub isPositive: Option<bool>
}

impl_map_collect!(ids::TypeID, TypeBonuses, typeID);

/// Dogma information for a [`Type`]
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="typeDogma"))]
pub struct TypeDogma {
    /// Applicable type
    #[serde(rename="_key")]
    pub typeID: ids::TypeID,
    /// Type's attributes
    ///
    /// Map of attributeID and attribute value
    #[serde(deserialize_with="deserialize_type_attributes")]
    pub dogmaAttributes: IndexMap<ids::AttributeID, f64>,
    /// Type's effects
    ///
    /// Map of effectID and whether or not the effect is set as "isDefault". (Meaning of "isDefault" not documented here)
    #[serde(default)]
    #[serde(deserialize_with="deserialize_type_effects")]
    pub dogmaEffects: IndexMap<ids::EffectID, bool>
}

fn deserialize_type_attributes<'de, D: Deserializer<'de>>(deserializer: D) -> Result<IndexMap<ids::AttributeID, f64>, D::Error> {
    struct EntryVisitor;
    impl<'de> Visitor<'de> for EntryVisitor {
        type Value = IndexMap<ids::AttributeID, f64>;

        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            formatter.write_str("array of type attributes")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error> where A: SeqAccess<'de> {
            #[derive(Debug, Deserialize)]
            #[allow(non_snake_case)]
            #[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
            struct TypeDogmaAttribute {
                pub attributeID: ids::AttributeID,
                pub value: f64
            }

            let size_hint = seq.size_hint();
            let mut map = size_hint.map(IndexMap::with_capacity).unwrap_or_else(IndexMap::new);
            while let Some(value) = seq.next_element::<TypeDogmaAttribute>()? {
                map.insert(value.attributeID, value.value);
            }
            Ok(map)
        }
    }

    deserializer.deserialize_seq(EntryVisitor)
}

fn deserialize_type_effects<'de, D: Deserializer<'de>>(deserializer: D) -> Result<IndexMap<ids::EffectID, bool>, D::Error> {
    struct EntryVisitor;
    impl<'de> Visitor<'de> for EntryVisitor {
        type Value = IndexMap<ids::EffectID, bool>;

        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            formatter.write_str("array of type effects")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error> where A: SeqAccess<'de> {
            #[derive(Debug, Deserialize)]
            #[allow(non_snake_case)]
            #[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
            struct TypeDogmaEffect {
                pub effectID: ids::EffectID,
                pub isDefault: bool
            }

            let size_hint = seq.size_hint();
            let mut map = size_hint.map(IndexMap::with_capacity).unwrap_or_else(IndexMap::new);
            while let Some(value) = seq.next_element::<TypeDogmaEffect>()? {
                map.insert(value.effectID, value.isDefault);
            }
            Ok(map)
        }
    }

    deserializer.deserialize_seq(EntryVisitor)
}

impl_map_collect!(ids::TypeID, TypeDogma, typeID);


#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="typeElements"))]
pub struct TypeElement {
    #[serde(rename="_key")]
    pub typeID: ids::TypeID,
    #[serde(deserialize_with="deserialize_explicit_entry_map")]
    pub elements: IndexMap<u32, u32>,   // TODO: Document key/value
}

impl_map_collect!(ids::TypeID, TypeElement, typeID);

/// TypeList; List of types
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="typeLists"))]
pub struct TypeList {
    /// ID for this type list
    #[serde(rename="_key")]
    pub typeListID: ids::TypeID,
    /// Display Name for this typeList (as used in-game, such as "Common Ores")
    pub displayName: Option<LocalizedString>,
    /// Display Description for this typeList (as used in-game, such as "<b>Tech 1</b> and empire faction <b>Navy</b> ships of <b>Battleship size or smaller</b>." for acceleration gate restrictions)
    pub displayDescription: Option<LocalizedString>,
    /// "Developer" name
    pub name: String,
    /// Included typeIDs, may be overridden by excluded typeIDs
    #[serde(default)]
    pub includedTypeIDs: Vec<ids::TypeID>,
    /// Excluded typeIDs, overrides included typeIDs
    #[serde(default)]
    pub excludedTypeIDs: Vec<ids::TypeID>,
    /// Included groupIDs, may be overridden by excluded groupIDs
    #[serde(default)]
    pub includedGroupIDs: Vec<ids::GroupID>,
    /// Excluded groupIDs, overrides included groupIDs
    #[serde(default)]
    pub excludedGroupIDs: Vec<ids::GroupID>,
    /// Included categoryIDs, may be overridden by excluded categoryIDs
    #[serde(default)]
    pub includedCategoryIDs: Vec<ids::CategoryID>,
    /// Excluded categoryIDs, overrides included categoryIDs
    #[serde(default)]
    pub excludedCategoryIDs: Vec<ids::CategoryID>,
}

impl TypeList {

    /// Test if a given type is contained in this TypeList.
    ///
    /// # Arguments
    ///
    /// * `item_type`: TypeID for the type to lookup
    /// * `item_group`: GroupID for the type to lookup (Obtained through [`Type::groupID`])
    /// * `item_category`: CategoryID for the type to lookup (Obtained through [`Group::categoryID`])
    ///
    /// returns: bool; True if this list contains the specified type
    pub fn contains(&self, item_type: ids::TypeID, item_group: ids::GroupID, item_category: ids::CategoryID) -> bool {
        (
            (
                (
                    (
                        self.includedCategoryIDs.contains(&item_category)
                            && !self.excludedCategoryIDs.contains(&item_category)
                    ) || self.includedGroupIDs.contains(&item_group)
                ) && !self.excludedGroupIDs.contains(&item_group)
            ) || self.includedTypeIDs.contains(&item_type)
        ) && !self.excludedTypeIDs.contains(&item_type)
    }

    pub fn flatten<C: Fn(ids::CategoryID) -> IG, G: Fn(ids::GroupID) -> IT, IG: IntoIterator<Item=ids::GroupID>, IT: IntoIterator<Item=ids::TypeID>>(&self, category_groups: C, group_types: G) -> impl Iterator<Item=ids::TypeID> {
        self.includedCategoryIDs.iter().copied()
            .filter(|c| !self.excludedCategoryIDs.contains(c))
            .flat_map(category_groups)
            .chain(self.includedGroupIDs.iter().copied())
            .filter(|g| !self.excludedGroupIDs.contains(g))
            .flat_map(group_types)
            .chain(self.includedTypeIDs.iter().copied())
            .filter(|t| !self.excludedTypeIDs.contains(t))
    }

    pub fn as_reflist(&self) -> RefList<'_> {
        RefList {
            name: &*self.name,
            included_types: &self.includedTypeIDs,
            excluded_types: &self.excludedTypeIDs,
            included_groups: &self.includedGroupIDs,
            excluded_groups: &self.excludedGroupIDs,
            included_categories: &self.includedCategoryIDs,
            excluded_categories: &self.excludedCategoryIDs,
        }
    }
}

impl_map_collect!(ids::TypeListID, TypeList, typeListID);


/// Type reprocessing output
///
/// To reprocess an item, a stack of [`Type::portionSize`] units is required
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="typeMaterials"))]
pub struct TypeMaterials {
    /// Input/reprocessed typeID
    #[serde(rename="_key")]
    pub typeID: ids::TypeID,
    /// Output materials
    #[serde(default)]
    pub materials: Vec<TypeMaterial>,
    /// Output materials subject to random selection
    ///
    /// Reprocessing will yield one of these materials, with a quantity between [`TypeRandomMaterial::quantityMin`] and [`TypeRandomMaterial::quantityMax`]
    #[serde(default)]
    pub randomizedMaterials: Vec<TypeRandomMaterial>
}

/// Single type reprocessing output
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct TypeMaterial {
    /// Result output [`Type`]
    pub materialTypeID: ids::TypeID,
    /// Output quantity in units, per [`Type::portionSize`] of input materials
    pub quantity: u32
}

/// Single random reprocessing output possibility
///
/// During reprocessing, a random roll between `quantityMin` and `quantityMax` is made
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(internal_type))]
pub struct TypeRandomMaterial {
    /// Result output [`Type`]
    pub materialTypeID: ids::TypeID,
    /// Maximum quantity, per [`Type::portionSize`] of input materials
    pub quantityMax: u32,   // TODO: Document whether these are inclusive
    /// Minimum quantity, per [`Type::portionSize`] of input materials
    pub quantityMin: u32,
}

impl_map_collect!(ids::TypeID, TypeMaterials, typeID);

/// [`ShipTreeElement`]s for a [`Type`]
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct TypeShipTreeElements {
    /// TypeID for which elements are provided
    #[serde(rename="_key")]
    pub typeID: ids::TypeID,
    /// Mapping of abstract 'GUI slot number' to [`ShipTreeElement`]
    ///
    /// Practically: Treat as a ordered list ([`IndexMap::values()`])
    pub elements: IndexMap<u32, ids::ShipTreeElementID>
}

impl_map_collect!(ids::TypeID, TypeShipTreeElements, typeID);

/// Item type
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
#[cfg_attr(feature="docs_export", doc_sde(sde_file="types"))]
pub struct Type {
    /// ID for this type
    #[serde(rename="_key")]
    pub typeID: ids::TypeID,
    /// [`Group`] this type belongs to
    pub groupID: ids::GroupID,
    /// Item type name
    ///
    /// Name for the "packaged"/stackable items. Certain items like assembled ships have unique names for each instance
    pub name: LocalizedString,
    /// Item description
    pub description: Option<LocalizedString>,
    /// Cargo capacity, in m³
    pub capacity: Option<f64>,
    /// Meta group; Tech tier of this item. Defaults to `1` or "Tech 1" if unspecified.
    pub metaGroupID: Option<ids::MetaGroupID>,
    /// Meta level; Ordinal ranking of types
    pub metaLevel: Option<values::MetaLevel>,
    /// Tech level; Whether this item is Tech 1, Tech 2, or Tech 3 regardless of metaGroup (e.g. 'faction T2'). Caution: Some types have unusual values
    pub techLevel: Option<values::TechLevel>,
    /// Marketgroup for this type. `None` indicates this type cannot be sold on the market, but may possibly be sold through contracts
    pub marketGroupID: Option<ids::MarketGroupID>,
    /// Variant type "parent"; The basic tier 1 version of this module/ship/etc.
    ///
    /// The parent itself has no field to identify it as a parent or having variants
    pub variationParentTypeID: Option<ids::TypeID>,
    /// Item's faction. Used for e.g. ships and drones
    pub factionID: Option<ids::FactionID>,
    /// Item's associated character race. Used for e.g. ships and drones
    pub raceID: Option<ids::RaceID>,
    /// Base "standard" price. Affects seeded items (blueprints, skillbooks, etc) pricing
    pub basePrice: Option<f64>,
    /// 2D Item [`Icon`]
    ///
    /// Certain items (ships, drones, anything with a 3D model) do not have a 2D icon and use a rendered image derived from the graphicID instead
    pub iconID: Option<ids::IconID>,
    /// 3D model information, see [`Graphic`]
    pub graphicID: Option<ids::GraphicID>,
    /// Item volume in m³
    ///
    /// This is the assembled volume for ships/etc. Packaged volumes are currently unavailable through the SDE.
    pub volume: Option<f64>,
    /// Item mass in kg
    ///
    /// Mainly used for wormhole transit and jump portal fuel calculations
    pub mass: Option<f64>,
    /// Item radius
    ///
    /// Used for e.g. ship collisions
    pub radius: Option<f64>,
    /// Reprocessing batch size
    ///
    /// Reprocessing output is available through [`TypeMaterials`]
    pub portionSize: i32,
    /// Whether this type is set as visible in-game
    pub published: bool,
    /// Type sounds
    ///
    /// soundID is not currently useful for third party developers as information about sounds is not made available
    pub soundID: Option<ids::SoundID>,
    /// ShipTree Group
    pub shipTreeGroupID: Option<ids::ShipTreeGroupID>,
    /// Volume when packaged
    ///
    /// Only applicable if [`Type::isRepackable`] is set true
    pub packagedVolume: Option<f64>,
    /// If true, this item can be (re)packaged
    #[serde(default)] // Types without this field are not repackable
    pub isRepackable: bool,
    /// If true, this item is a dynamic type for which attributes are given per-instance through [`DynamicItemAttributes`]
    #[serde(default)]
    pub isDynamicType: bool
}

impl_map_collect!(ids::TypeID, Type, typeID);

/// Entire Static Data Export as a single struct
///
/// Loaded through [`SDELoader::full`]
#[allow(non_camel_case_types)]  // "SDE" is an abbreviation here
#[derive(Debug)]
pub struct SDE_Full {
    pub accounting_entry_types: IndexMap<ids::AccountingEntryTypeID, AccountingEntryType>,
    pub agent_types: IndexMap<ids::AgentTypeID, AgentType>,
    pub agents_in_space: IndexMap<ids::CharacterID, AgentInSpace>,
    pub ancestries: IndexMap<ids::AncestryID, Ancestry>,
    pub applied_proximity_effects: IndexMap<ids::TypeID, AppliedProximityEffect>,
    pub archetypes: IndexMap<ids::DungeonArchetypeID, Archetype>,
    pub bloodlines: IndexMap<ids::BloodlineID, Bloodline>,
    pub blueprints: IndexMap<ids::TypeID, Blueprint>,
    pub categories: IndexMap<ids::CategoryID, Category>,
    pub certificates: IndexMap<ids::CertificateID, Certificate>,
    pub character_attributes: IndexMap<ids::CharacterAttributeID, CharacterAttribute>,
    pub character_titles: IndexMap<uuids::CharacterTitleID, CharacterTitle>,
    pub clone_grades: IndexMap<ids::CloneGradeID, CloneGrade>,
    pub compressible_types: IndexMap<ids::TypeID, ids::TypeID>,
    pub contraband_types: IndexMap<ids::TypeID, ContrabandType>,
    pub control_tower_resources: IndexMap<ids::TypeID, ControlTowerResources>,
    pub corporation_activities: IndexMap<ids::CorporationActivityID, CorporationActivity>,
    pub corporation_role_groups: IndexMap<ids::CorporationRoleGroupID, CorporationRoleGroup>,
    pub corporation_roles: IndexMap<ids::CorporationRoleID, CorporationRole>,
    pub dbuff_collections: IndexMap<ids::DynamicBuffID, DynamicBuff>,
    pub dogma_attribute_categories: IndexMap<ids::AttributeCategoryID, AttributeCategory>,
    pub dogma_attributes: IndexMap<ids::AttributeID, Attribute>,
    pub dogma_effects: IndexMap<ids::EffectID, Effect>,
    pub dogma_units: IndexMap<EVEUnit, DogmaUnit>,
    pub dungeons: IndexMap<ids::DungeonID, Dungeon>,
    pub dynamic_item_attributes: IndexMap<ids::TypeID, DynamicItemAttributes>,
    pub epic_arcs: IndexMap<ids::EpicArcID, EpicArc>,
    pub expert_systems: IndexMap<ids::TypeID, ExpertSystem>,
    pub factions: IndexMap<ids::FactionID, Faction>,
    pub fighter_abilities: IndexMap<ids::FighterAbilityID, FighterAbility>,
    pub fighter_abilities_by_type: IndexMap<ids::TypeID, FighterAbilitiesByType>,
    pub freelance_job_schemas: IndexMap<ids::JobSchemaID, Vec<FreelanceJobSchema>>,
    pub graphic_material_sets: IndexMap<ids::MaterialSetID, GraphicMaterialSet>,
    pub graphics: IndexMap<ids::GraphicID, Graphic>,
    pub groups: IndexMap<ids::GroupID, Group>,
    pub icons: IndexMap<ids::IconID, Icon>,
    pub industry_activities: IndexMap<ids::IndustryActivityID, IndustryActivity>,
    pub industry_assembly_lines: IndexMap<ids::IndustryAssemblyLineID, IndustryAssemblyLine>,
    pub industry_installation_types: IndexMap<ids::TypeID, IndustryInstallationType>,
    pub industry_modifier_sources: IndexMap<ids::TypeID, IndustryModifierSource>,
    pub industry_target_filters: IndexMap<ids::IndustryFilterID, IndustryTargetFilter>,
    pub landmarks: IndexMap<ids::LandmarkID, Landmark>,
    pub link_with_ship: IndexMap<ids::TypeID, LinkWithShip>,
    pub map_asteroid_belts: IndexMap<ids::AsteroidBeltID, AsteroidBelt>,
    pub map_constellations: IndexMap<ids::ConstellationID, Constellation>,
    pub map_moons: IndexMap<ids::MoonID, Moon>,
    pub map_planets: IndexMap<ids::PlanetID, Planet>,
    pub map_regions: IndexMap<ids::RegionID, Region>,
    pub map_secondarysuns: IndexMap<ids::SolarSystemID, SecondarySun>,
    pub map_solarsystems: IndexMap<ids::SolarSystemID, SolarSystem>,
    pub map_stargates: IndexMap<ids::StargateID, Stargate>,
    pub map_stars: IndexMap<ids::StarID, Star>,
    pub market_groups: IndexMap<ids::MarketGroupID, MarketGroup>,
    pub masteries: IndexMap<ids::TypeID, MasteryInfo>,
    pub mercenary_tactical_operations: IndexMap<ids::DungeonID, MercenaryTacticalOperation>,
    pub meta_groups: IndexMap<ids::MetaGroupID, MetaGroup>,
    pub metenox_moon_drill: IndexMap<ids::TypeID, MetenoxMoonDrill>,
    pub military_campaign_objectives: IndexMap<uuids::MilitaryCampaignObjectiveID, MilitaryCampaignObjective>,
    pub military_campaigns: IndexMap<uuids::MilitaryCampaignID, MilitaryCampaign>,
    pub missions: IndexMap<ids::MissionID, Mission>,
    pub notification_types: IndexMap<ids::NotificationTypeID, NotificationType>,
    pub npc_characters: IndexMap<ids::CharacterID, NpcCharacter>,
    pub npc_corporation_divisions: IndexMap<ids::DivisionID, CorporationDivision>,
    pub npc_corporations: IndexMap<ids::CorporationID, NpcCorporation>,
    pub npc_stations: IndexMap<ids::StationID, NpcStation>,
    pub planet_resources: IndexMap<ids::PlanetID, PlanetResource>,
    pub planet_schematics: IndexMap<ids::PlanetSchematicID, PlanetSchematic>,
    pub proximity_trap: IndexMap<ids::TypeID, ProximityTrap>,
    pub races: IndexMap<ids::RaceID, CharacterRace>,
    pub school_map: IndexMap<ids::SchoolID, ids::StationID>,
    pub schools: IndexMap<ids::SchoolID, School>,
    pub ship_tree_elements: IndexMap<ids::ShipTreeElementID, ShipTreeElement>,
    pub ship_tree_factions: IndexMap<ids::FactionID, ShipTreeFaction>,
    pub ship_tree_groups: IndexMap<ids::ShipTreeGroupID, ShipTreeGroup>,
    pub skill_plans: IndexMap<u32, SkillPlan>,
    pub skin_licenses: IndexMap<ids::TypeID, SkinLicense>,
    pub skin_materials: IndexMap<ids::SkinMaterialID, SkinMaterial>,
    pub skinr_component_categories: IndexMap<ids::SKINRComponentCategoryID, SKINRComponentCategory>,
    pub skinr_component_point_values: IndexMap<ids::SKINRComponentCategoryID, SKINRComponentPointValues>,
    pub skinr_component_rarities: IndexMap<u32, SKINRComponentRarity>,
    pub skinr_components: IndexMap<ids::TypeID, SKINRComponent>,
    pub skinr_slot_categories: IndexMap<ids::SKINRSlotCategoryID, SKINRSlotCategory>,
    pub skinr_slot_configurations: IndexMap<u32, SKINRSlotConfiguration>,
    pub skinr_slot_names: IndexMap<u32, SKINRSlotName>,
    pub skinr_slots: IndexMap<u32, SKINRSlot>,
    pub skinr_slots_to_materials: IndexMap<u32, SKINRSlotsToMaterials>,
    pub skinr_tier_thresholds: IndexMap<u32, SKINRTierThreshold>,
    pub skins: IndexMap<ids::SkinID, Skin>,
    pub sovereignty_upgrades: IndexMap<ids::TypeID, SovereigntyUpgrade>,
    pub station_operations: IndexMap<ids::StationOperationID, StationOperation>,
    pub station_services: IndexMap<ids::StationServiceID, StationService>,
    pub station_standings_restrictions: IndexMap<ids::FactionID, StationStandingsRestriction>,
    pub system_dbuff_emitters: IndexMap<ids::TypeID, SystemDynamicbuffEmitter>,
    pub system_wide_effects: IndexMap<ids::TypeID, SystemWideEffect>,
    pub translation_languages: Vec<TranslationLanguage>,
    pub type_bonus: IndexMap<ids::TypeID, TypeBonuses>,
    pub type_dogma: IndexMap<ids::TypeID, TypeDogma>,
    pub type_elements: IndexMap<ids::TypeID, TypeElement>,
    pub type_lists: IndexMap<ids::TypeListID, TypeList>,
    pub type_materials: IndexMap<ids::TypeID, TypeMaterials>,
    pub types: IndexMap<ids::TypeID, Type>,
}

// SDELoader encapsulates ZipArchive & zip crate dependency
pub struct SDELoader<R: Read + Seek = File> {
    archive: ZipArchive<R>,
    build_number: u32
}

impl SDELoader<File> {
    /// Updates the specified file to the latest version of the SDE, then opens it and returns a `SDELoader`
    #[cfg(all(feature="sde_load", feature="sde_update"))]
    pub fn open_latest<P: AsRef<std::path::Path>>(file: P) -> Result<Self, SDELoadError> {
        crate::sde::update::update_sde(file.as_ref())?;
        Self::new(File::open(file)?)
    }
}

impl<R: Read + Seek> SDELoader<R> {
    pub fn new(reader: R) -> Result<Self, SDELoadError> {
        let mut loader = SDELoader {
            archive: ZipArchive::new(reader)?,
            build_number: 0
        };

        #[derive(Deserialize)]
        #[allow(non_snake_case, unused)]
        struct SDEVersion {
            _key: String,
            buildNumber: u32,
            releaseDate: String
        }

        let sde_version = loader.load_file::<SDEVersion>("_sde.jsonl")?.next()
            .ok_or_else(|| SDELoadError::IntegrityError("No entries in _sde.jsonl!?".to_string()))??;

        loader.build_number = sde_version.buildNumber;

        Ok(loader)
    }

    pub fn version(&self) -> u32 {
        self.build_number
    }

    /// Load a single file from the zip archive, and parse it to a datatype
    ///
    /// Returns an iterator over each entry
    fn load_file<'a, T: DeserializeOwned>(&'a mut self, file_name: &'a str) -> Result<impl Iterator<Item=Result<T, SDELoadError>> + use<'a, T, R>, SDELoadError> {
        let mut str_buf = String::new();
        let mut reader = BufReader::new(
            self.archive
                .by_name(file_name)
                .map_err(|err| {
                    if let ZipError::FileNotFound = err {
                        SDELoadError::ArchiveFileNotFound(file_name.to_owned())
                    } else {
                        SDELoadError::Zip(err)
                    }
                })?
        );

        let mut entry = 0;
        Ok(std::iter::from_fn(move || { // TODO: Replace with a proper custom iterator implementation to provide better support for skip/nth
            match reader.read_line(&mut str_buf) {
                Ok(0) => None,
                Ok(_) => {
                    entry += 1;
                    let res = serde_json::from_str::<T>(&str_buf).map_err(|error| SDELoadError::ParseError { file: file_name.to_owned(), entry, error });
                    str_buf.clear();
                    Some(res)
                }
                Err(err) => Some(Err(SDELoadError::IO(err))),
            }
        }))
    }

    /// Load `accountingEntryTypes` as iterator
    pub fn load_accounting_entry_types(&mut self) -> Result<impl Iterator<Item=Result<AccountingEntryType, SDELoadError>>, SDELoadError> {
        self.load_file::<AccountingEntryType>("accountingEntryTypes.jsonl")
    }

    /// Load `accountingEntryTypes` as map
    pub fn load_accounting_entry_types_map(&mut self) -> Result<IndexMap<ids::AccountingEntryTypeID, AccountingEntryType>, SDELoadError> {
        self.load_accounting_entry_types()?.collect()
    }

    /// Load 'agentTypes' as iterator
    pub fn load_agent_types(&mut self) -> Result<impl Iterator<Item=Result<(ids::AgentTypeID, AgentType), SDELoadError>>, SDELoadError> {
        self.load_file::<AgentTypeEntry>("agentTypes.jsonl")
            .map(|iter| iter.map(|res| res.map(|entry| (entry.agentTypeID, entry.name))))
    }

    /// Load 'agentTypes' as map
    pub fn load_agent_types_map(&mut self) -> Result<IndexMap<ids::AgentTypeID, AgentType>, SDELoadError> {
        self.load_agent_types()?.collect()
    }

    /// Load 'agentsInSpace' as iterator
    pub fn load_agents_in_space(&mut self) -> Result<impl Iterator<Item=Result<AgentInSpace, SDELoadError>>, SDELoadError> {
        self.load_file::<AgentInSpace>("agentsInSpace.jsonl")
    }

    /// Load 'agentsInSpace' map
    pub fn load_agents_in_space_map(&mut self) -> Result<IndexMap<ids::CharacterID, AgentInSpace>, SDELoadError> {
        self.load_agents_in_space()?.collect()
    }

    /// Load 'ancestries' as iterator
    pub fn load_ancestries(&mut self) -> Result<impl Iterator<Item=Result<Ancestry, SDELoadError>>, SDELoadError> {
        self.load_file::<Ancestry>("ancestries.jsonl")
    }

    /// Load 'ancestries' as map
    pub fn load_ancestries_map(&mut self) -> Result<IndexMap<ids::AncestryID, Ancestry>, SDELoadError> {
        self.load_ancestries()?.collect()
    }

    /// Load `appliedProximityEffects` as iterator
    pub fn load_applied_proximity_effects(&mut self) -> Result<impl Iterator<Item=Result<AppliedProximityEffect, SDELoadError>>, SDELoadError> {
        self.load_file::<AppliedProximityEffect>("appliedProximityEffects.jsonl")
    }

    /// Load `appliedProximityEffects` as map
    pub fn load_applied_proximity_effects_map(&mut self) -> Result<IndexMap<ids::TypeID, AppliedProximityEffect>, SDELoadError> {
        self.load_applied_proximity_effects()?.collect()
    }

    /// Load 'archetypes' as iterator
    pub fn load_archetypes(&mut self) -> Result<impl Iterator<Item=Result<Archetype, SDELoadError>>, SDELoadError> {
        self.load_file::<Archetype>("archetypes.jsonl")
    }

    /// Load 'archetypes' as map
    pub fn load_archetypes_map(&mut self) -> Result<IndexMap<ids::DungeonArchetypeID, Archetype>, SDELoadError> {
        self.load_archetypes()?.collect()
    }

    /// Load 'bloodlines' as iterator
    pub fn load_bloodlines(&mut self) -> Result<impl Iterator<Item=Result<Bloodline, SDELoadError>>, SDELoadError> {
        self.load_file::<Bloodline>("bloodlines.jsonl")
    }

    /// Load 'bloodlines' as map
    pub fn load_bloodlines_map(&mut self) -> Result<IndexMap<ids::BloodlineID, Bloodline>, SDELoadError> {
        self.load_bloodlines()?.collect()
    }

    /// Load 'blueprints' as iterator
    pub fn load_blueprints(&mut self) -> Result<impl Iterator<Item=Result<Blueprint, SDELoadError>>, SDELoadError> {
        self.load_file::<Blueprint>("blueprints.jsonl")
    }

    /// Load 'blueprints' as map
    pub fn load_blueprints_map(&mut self) -> Result<IndexMap<ids::TypeID, Blueprint>, SDELoadError> {
        self.load_blueprints()?.collect()
    }

    /// Load 'categories' as iterator
    pub fn load_categories(&mut self) -> Result<impl Iterator<Item=Result<Category, SDELoadError>>, SDELoadError> {
        self.load_file::<Category>("categories.jsonl")
    }

    /// Load 'categories' as map
    pub fn load_categories_map(&mut self) -> Result<IndexMap<ids::CategoryID, Category>, SDELoadError> {
        self.load_categories()?.collect()
    }

    /// Load 'certificates' as iterator
    pub fn load_certificates(&mut self) -> Result<impl Iterator<Item=Result<Certificate, SDELoadError>>, SDELoadError> {
        self.load_file::<Certificate>("certificates.jsonl")
    }

    /// Load 'certificates' as map
    pub fn load_certificates_map(&mut self) -> Result<IndexMap<ids::CertificateID, Certificate>, SDELoadError> {
        self.load_certificates()?.collect()
    }

    /// Load 'characterAttributes' as iterator
    pub fn load_character_attributes(&mut self) -> Result<impl Iterator<Item=Result<CharacterAttribute, SDELoadError>>, SDELoadError> {
        self.load_file::<CharacterAttribute>("characterAttributes.jsonl")
    }

    /// Load 'characterAttributes' as map
    pub fn load_character_attributes_map(&mut self) -> Result<IndexMap<ids::CharacterAttributeID, CharacterAttribute>, SDELoadError> {
        self.load_character_attributes()?.collect()
    }

    /// Load 'characterTitles' as iterator
    pub fn load_character_titles(&mut self) -> Result<impl Iterator<Item=Result<CharacterTitle, SDELoadError>>, SDELoadError> {
        self.load_file::<CharacterTitle>("characterTitles.jsonl")
    }

    /// Load 'characterTitles' as map
    pub fn load_character_titles_map(&mut self) -> Result<IndexMap<uuids::CharacterTitleID, CharacterTitle>, SDELoadError> {
        self.load_character_titles()?.collect()
    }

    /// Load 'cloneGrades' as iterator
    pub fn load_clone_grades(&mut self) -> Result<impl Iterator<Item=Result<CloneGrade, SDELoadError>>, SDELoadError> {
        self.load_file::<CloneGrade>("cloneGrades.jsonl")
    }

    /// Load 'cloneGrades' as map
    pub fn load_clone_grades_map(&mut self) -> Result<IndexMap<ids::CloneGradeID, CloneGrade>, SDELoadError> {
        self.load_clone_grades()?.collect()
    }

    /// Load 'compressibleTypes' as iterator
    pub fn load_compressible_types(&mut self) -> Result<impl Iterator<Item=Result<CompressibleType, SDELoadError>>, SDELoadError> {
        self.load_file::<CompressibleType>("compressibleTypes.jsonl")
    }

    /// Load 'compressibleTypes' as map
    ///
    /// Map of ore typeID to compressed "block" typeID
    pub fn load_compressible_types_map(&mut self) -> Result<IndexMap<ids::TypeID, ids::TypeID>, SDELoadError> {
        self.load_compressible_types()?.collect()
    }

    /// Load 'contrabandTypes' as iterator
    pub fn load_contraband_types(&mut self) -> Result<impl Iterator<Item=Result<ContrabandType, SDELoadError>>, SDELoadError> {
        self.load_file::<ContrabandType>("contrabandTypes.jsonl")
    }

    /// Load 'contrabandTypes' as map
    pub fn load_contraband_types_map(&mut self) -> Result<IndexMap<ids::TypeID, ContrabandType>, SDELoadError> {
        self.load_contraband_types()?.collect()
    }

    /// Load 'controlTowerResources' as iterator
    pub fn load_controltower_resources(&mut self) -> Result<impl Iterator<Item=Result<ControlTowerResources, SDELoadError>>, SDELoadError> {
        self.load_file::<ControlTowerResources>("controlTowerResources.jsonl")
    }

    /// Load 'controlTowerResources' as map
    pub fn load_controltower_resources_map(&mut self) -> Result<IndexMap<ids::TypeID, ControlTowerResources>, SDELoadError> {
        self.load_controltower_resources()?.collect()
    }

    /// Load 'corporationActivities' as iterator
    pub fn load_corporation_activities(&mut self) -> Result<impl Iterator<Item=Result<CorporationActivity, SDELoadError>>, SDELoadError> {
        self.load_file::<CorporationActivity>("corporationActivities.jsonl")
    }

    /// Load 'corporationActivities' as map
    pub fn load_corporation_activities_map(&mut self) -> Result<IndexMap<ids::CorporationActivityID, CorporationActivity>, SDELoadError> {
        self.load_corporation_activities()?.collect()
    }

    /// Load `corporationRoleGroups` as iterator
    pub fn load_corporation_role_groups(&mut self) -> Result<impl Iterator<Item=Result<CorporationRoleGroup, SDELoadError>>, SDELoadError> {
        self.load_file::<CorporationRoleGroup>("corporationRoleGroups.jsonl")
    }

    /// Load `corporationRoleGroups` as map
    pub fn load_corporation_role_groups_map(&mut self) -> Result<IndexMap<ids::CorporationRoleGroupID, CorporationRoleGroup>, SDELoadError> {
        self.load_corporation_role_groups()?.collect()
    }

    /// Load `corporationRoles` as iterator
    pub fn load_corporation_roles(&mut self) -> Result<impl Iterator<Item=Result<CorporationRole, SDELoadError>>, SDELoadError> {
        self.load_file::<CorporationRole>("corporationRoles.jsonl")
    }

    /// Load `corporationRoles` as map
    pub fn load_corporation_roles_map(&mut self) -> Result<IndexMap<ids::CorporationRoleID, CorporationRole>, SDELoadError> {
        self.load_corporation_roles()?.collect()
    }

    /// Load 'dbuffCollections' as iterator
    pub fn load_dbuff_collections(&mut self) -> Result<impl Iterator<Item=Result<DynamicBuff, SDELoadError>>, SDELoadError> {
        self.load_file::<DynamicBuff>("dbuffCollections.jsonl")
    }

    /// Load 'dbuffCollections' as map
    pub fn load_dbuff_collections_map(&mut self) -> Result<IndexMap<ids::DynamicBuffID, DynamicBuff>, SDELoadError> {
        self.load_dbuff_collections()?.collect()
    }

    /// Load 'dogmaAttributeCategories' as iterator
    pub fn load_dogma_attribute_categories(&mut self) -> Result<impl Iterator<Item=Result<AttributeCategory, SDELoadError>>, SDELoadError> {
        self.load_file::<AttributeCategory>("dogmaAttributeCategories.jsonl")
    }

    /// Load 'dogmaAttributeCategories' as map
    pub fn load_dogma_attribute_categories_map(&mut self) -> Result<IndexMap<ids::AttributeCategoryID, AttributeCategory>, SDELoadError> {
        self.load_dogma_attribute_categories()?.collect()
    }

    /// Load 'dogmaAttributes' as iterator
    pub fn load_dogma_attributes(&mut self) -> Result<impl Iterator<Item=Result<Attribute, SDELoadError>>, SDELoadError> {
        self.load_file::<Attribute>("dogmaAttributes.jsonl")
    }

    /// Load 'dogmaAttributes' as map
    pub fn load_dogma_attributes_map(&mut self) -> Result<IndexMap<ids::AttributeID, Attribute>, SDELoadError> {
        self.load_dogma_attributes()?.collect()
    }

    /// Load 'dogmaEffects' as iterator
    pub fn load_dogma_effects(&mut self) -> Result<impl Iterator<Item=Result<Effect, SDELoadError>>, SDELoadError> {
        self.load_file::<Effect>("dogmaEffects.jsonl")
    }

    /// Load 'dogmaEffects' as map
    pub fn load_dogma_effects_map(&mut self) -> Result<IndexMap<ids::EffectID, Effect>, SDELoadError> {
        self.load_dogma_effects()?.collect()
    }

    /// Load 'dogmaUnits' as iterator
    ///
    /// You probably want [`EVEUnit`] directly
    pub fn load_dogma_units(&mut self) -> Result<impl Iterator<Item=Result<DogmaUnit, SDELoadError>>, SDELoadError> {
        self.load_file::<DogmaUnit>("dogmaUnits.jsonl")
    }

    /// Load 'dogmaUnits' as map
    ///
    /// You probably want [`EVEUnit`] directly
    pub fn load_dogma_units_map(&mut self) -> Result<IndexMap<EVEUnit, DogmaUnit>, SDELoadError> {
        self.load_dogma_units()?.collect()
    }

    /// Load 'dungeons' as iterator
    pub fn load_dungeons(&mut self) -> Result<impl Iterator<Item=Result<Dungeon, SDELoadError>>, SDELoadError> {
        self.load_file::<Dungeon>("dungeons.jsonl")
    }

    /// Load 'dungeons' as map
    pub fn load_dungeons_map(&mut self) -> Result<IndexMap<ids::DungeonID, Dungeon>, SDELoadError> {
        self.load_dungeons()?.collect()
    }

    /// Load 'dynamicItemAttributes' as iterator
    pub fn load_dynamic_item_attributes(&mut self) -> Result<impl Iterator<Item=Result<DynamicItemAttributes, SDELoadError>>, SDELoadError> {
        self.load_file::<DynamicItemAttributes>("dynamicItemAttributes.jsonl")
    }

    /// Load 'dynamicItemAttributes' as map
    pub fn load_dynamic_item_attributes_map(&mut self) -> Result<IndexMap<ids::TypeID, DynamicItemAttributes>, SDELoadError> {
        self.load_dynamic_item_attributes()?.collect()
    }

    /// Load 'epicArcs' as iterator
    pub fn load_epic_arcs(&mut self) -> Result<impl Iterator<Item=Result<EpicArc, SDELoadError>>, SDELoadError> {
        self.load_file::<EpicArc>("epicArcs.jsonl")
    }

    /// Load 'epicArcs' as map
    pub fn load_epic_arcs_map(&mut self) -> Result<IndexMap<ids::EpicArcID, EpicArc>, SDELoadError> {
        self.load_epic_arcs()?.collect()
    }

    /// Load `expertSystems` as iterator
    pub fn load_expert_systems(&mut self) -> Result<impl Iterator<Item=Result<ExpertSystem, SDELoadError>>, SDELoadError> {
        self.load_file::<ExpertSystem>("expertSystems.jsonl")
    }

    /// Load `expertSystems` as map
    pub fn load_expert_systems_map(&mut self) -> Result<IndexMap<ids::TypeID, ExpertSystem>, SDELoadError> {
        self.load_expert_systems()?.collect()
    }

    /// Load 'factions' as iterator
    pub fn load_factions(&mut self) -> Result<impl Iterator<Item=Result<Faction, SDELoadError>>, SDELoadError> {
        self.load_file::<Faction>("factions.jsonl")
    }

    /// Load 'factions' as map
    pub fn load_factions_map(&mut self) -> Result<IndexMap<ids::FactionID, Faction>, SDELoadError> {
        self.load_factions()?.collect()
    }

    /// Load `fighterAbilities` as iterator
    pub fn load_fighter_abilities(&mut self) -> Result<impl Iterator<Item=Result<FighterAbility, SDELoadError>>, SDELoadError> {
        self.load_file::<FighterAbility>("fighterAbilities.jsonl")
    }

    /// Load `fighterAbilities` as map
    pub fn load_fighter_abilities_map(&mut self) -> Result<IndexMap<ids::FighterAbilityID, FighterAbility>, SDELoadError> {
        self.load_fighter_abilities()?.collect()
    }

    /// Load `fighterAbilitiesByType` as iterator
    pub fn load_fighter_abilities_by_type(&mut self) -> Result<impl Iterator<Item=Result<FighterAbilitiesByType, SDELoadError>>, SDELoadError> {
        self.load_file::<FighterAbilitiesByType>("fighterAbilitiesByType.jsonl")
    }

    /// Load `fighterAbilitiesByType` as map
    pub fn load_fighter_abilities_by_type_map(&mut self) -> Result<IndexMap<ids::TypeID, FighterAbilitiesByType>, SDELoadError> {
        self.load_fighter_abilities_by_type()?.collect()
    }

    /// Load 'freelanceJobSchemas' as iterator
    pub fn load_freelance_job_schemas(&mut self) -> Result<impl Iterator<Item=Result<(ids::JobSchemaID, Vec<FreelanceJobSchema>), SDELoadError>>, SDELoadError> {
        self.load_file::<ExplicitMapEntry<u32, Vec<FreelanceJobSchema>>>("freelanceJobSchemas.jsonl")
            .map(|iter| {
                iter.map(|res| res.map(|entry| (entry._key, entry._value)))
            })
    }

    /// Load 'freelanceJobSchemas' as map
    pub fn load_freelance_job_schemas_map(&mut self) -> Result<IndexMap<ids::JobSchemaID, Vec<FreelanceJobSchema>>, SDELoadError> {
        self.load_freelance_job_schemas()?.collect()
    }

    /// Load `graphicMaterialSets` as iterator
    pub fn load_graphic_material_sets(&mut self) -> Result<impl Iterator<Item=Result<GraphicMaterialSet, SDELoadError>>, SDELoadError> {
        self.load_file::<GraphicMaterialSet>("graphicMaterialSets.jsonl")
    }

    /// Load `graphicMaterialSets` as map
    pub fn load_graphic_material_sets_map(&mut self) -> Result<IndexMap<ids::MaterialSetID, GraphicMaterialSet>, SDELoadError> {
        self.load_graphic_material_sets()?.collect()
    }

    /// Load 'graphics' as iterator
    pub fn load_graphics(&mut self) -> Result<impl Iterator<Item=Result<Graphic, SDELoadError>>, SDELoadError> {
        self.load_file::<Graphic>("graphics.jsonl")
    }

    /// Load 'graphics' as map
    pub fn load_graphics_map(&mut self) -> Result<IndexMap<ids::GraphicID, Graphic>, SDELoadError> {
        self.load_graphics()?.collect()
    }

    /// Load 'groups' as iterator
    pub fn load_groups(&mut self) -> Result<impl Iterator<Item=Result<Group, SDELoadError>>, SDELoadError> {
        self.load_file::<Group>("groups.jsonl")
    }

    /// Load 'groups' as map
    pub fn load_groups_map(&mut self) -> Result<IndexMap<ids::GroupID, Group>, SDELoadError> {
        self.load_groups()?.collect()
    }

    /// Load 'icons' as iterator
    pub fn load_icons(&mut self) -> Result<impl Iterator<Item=Result<Icon, SDELoadError>>, SDELoadError> {
        self.load_file::<Icon>("icons.jsonl")
    }

    /// Load 'icons' as map
    pub fn load_icons_map(&mut self) -> Result<IndexMap<ids::IconID, Icon>, SDELoadError> {
        self.load_icons()?.collect()
    }

    /// Load `industryActivities` as iterator
    pub fn load_industry_activities(&mut self) -> Result<impl Iterator<Item=Result<IndustryActivity, SDELoadError>>, SDELoadError> {
        self.load_file::<IndustryActivity>("industryActivities.jsonl")
    }

    /// Load `industryActivities` as map
    pub fn load_industry_activities_map(&mut self) -> Result<IndexMap<ids::IndustryActivityID, IndustryActivity>, SDELoadError> {
        self.load_industry_activities()?.collect()
    }

    /// Load `industryAssemblyLines` as iterator
    pub fn load_industry_assembly_lines(&mut self) -> Result<impl Iterator<Item=Result<IndustryAssemblyLine, SDELoadError>>, SDELoadError> {
        self.load_file::<IndustryAssemblyLine>("industryAssemblyLines.jsonl")
    }

    /// Load `industryAssemblyLines` as map
    pub fn load_industry_assembly_lines_map(&mut self) -> Result<IndexMap<ids::IndustryAssemblyLineID, IndustryAssemblyLine>, SDELoadError> {
        self.load_industry_assembly_lines()?.collect()
    }

    /// Load `industryInstallationTypes` as iterator
    pub fn load_industry_installation_types(&mut self) -> Result<impl Iterator<Item=Result<IndustryInstallationType, SDELoadError>>, SDELoadError> {
        self.load_file::<IndustryInstallationType>("industryInstallationTypes.jsonl")
    }

    /// Load `industryInstallationTypes` as map
    pub fn load_industry_installation_types_map(&mut self) -> Result<IndexMap<ids::TypeID, IndustryInstallationType>, SDELoadError> {
        self.load_industry_installation_types()?.collect()
    }

    /// Load `industryModifierSources` as iterator
    pub fn load_industry_modifier_sources(&mut self) -> Result<impl Iterator<Item=Result<IndustryModifierSource, SDELoadError>>, SDELoadError> {
        self.load_file::<IndustryModifierSource>("industryModifierSources.jsonl")
    }

    /// Load `industryModifierSources` as map
    pub fn load_industry_modifier_sources_map(&mut self) -> Result<IndexMap<ids::TypeID, IndustryModifierSource>, SDELoadError> {
        self.load_industry_modifier_sources()?.collect()
    }

    /// Load `industryTargetFilters` as iterator
    pub fn load_industry_target_filters(&mut self) -> Result<impl Iterator<Item=Result<IndustryTargetFilter, SDELoadError>>, SDELoadError> {
        self.load_file::<IndustryTargetFilter>("industryTargetFilters.jsonl")
    }

    /// Load `industryTargetFilters` as map
    pub fn load_industry_target_filters_map(&mut self) -> Result<IndexMap<ids::IndustryFilterID, IndustryTargetFilter>, SDELoadError> {
        self.load_industry_target_filters()?.collect()
    }

    /// Load 'landmarks' as iterator
    pub fn load_landmarks(&mut self) -> Result<impl Iterator<Item=Result<Landmark, SDELoadError>>, SDELoadError> {
        self.load_file::<Landmark>("landmarks.jsonl")
    }

    /// Load 'landmarks' as map
    pub fn load_landmarks_map(&mut self) -> Result<IndexMap<ids::LandmarkID, Landmark>, SDELoadError> {
        self.load_landmarks()?.collect()
    }

    /// Load `linkWithShip` as iterator
    pub fn load_link_with_ship(&mut self) -> Result<impl Iterator<Item=Result<LinkWithShip, SDELoadError>>, SDELoadError> {
        self.load_file::<LinkWithShip>("linkWithShip.jsonl")
    }

    /// Load `linkWithShip` as map
    pub fn load_link_with_ship_map(&mut self) -> Result<IndexMap<ids::TypeID, LinkWithShip>, SDELoadError> {
        self.load_link_with_ship()?.collect()
    }

    /// Load 'mapAsteroidBelts' as iterator
    pub fn load_asteroid_belts(&mut self) -> Result<impl Iterator<Item=Result<AsteroidBelt, SDELoadError>>, SDELoadError> {
        self.load_file::<AsteroidBelt>("mapAsteroidBelts.jsonl")
    }

    /// Load 'mapAsteroidBelts' as map
    pub fn load_asteroid_belts_map(&mut self) -> Result<IndexMap<ids::AsteroidBeltID, AsteroidBelt>, SDELoadError> {
        self.load_asteroid_belts()?.collect()
    }

    /// Load 'mapConstellations' as iterator
    pub fn load_constellations(&mut self) -> Result<impl Iterator<Item=Result<Constellation, SDELoadError>>, SDELoadError> {
        self.load_file::<Constellation>("mapConstellations.jsonl")
    }

    /// Load 'mapConstellations' as map
    pub fn load_constellations_map(&mut self) -> Result<IndexMap<ids::ConstellationID, Constellation>, SDELoadError> {
        self.load_constellations()?.collect()
    }

    /// Load 'mapMoons' as iterator
    pub fn load_moons(&mut self) -> Result<impl Iterator<Item=Result<Moon, SDELoadError>>, SDELoadError> {
        self.load_file::<Moon>("mapMoons.jsonl")
    }

    /// Load 'mapMoons' as map
    pub fn load_moons_map(&mut self) -> Result<IndexMap<ids::MoonID, Moon>, SDELoadError> {
        self.load_moons()?.collect()
    }

    /// Load 'mapPlanets' as iterator
    pub fn load_planets(&mut self) -> Result<impl Iterator<Item=Result<Planet, SDELoadError>>, SDELoadError> {
        self.load_file::<Planet>("mapPlanets.jsonl")
    }

    /// Load 'mapPlanets' as map
    pub fn load_planets_map(&mut self) -> Result<IndexMap<ids::PlanetID, Planet>, SDELoadError> {
        self.load_planets()?.collect()
    }

    /// Load 'mapRegions' as iterator
    pub fn load_regions(&mut self) -> Result<impl Iterator<Item=Result<Region, SDELoadError>>, SDELoadError> {
        self.load_file::<Region>("mapRegions.jsonl")
    }

    /// Load 'mapRegions' as map
    pub fn load_regions_map(&mut self) -> Result<IndexMap<ids::RegionID, Region>, SDELoadError> {
        self.load_regions()?.collect()
    }

    /// Load 'mapSecondarySuns' as iterator
    pub fn load_secondarysuns(&mut self) -> Result<impl Iterator<Item=Result<SecondarySun, SDELoadError>>, SDELoadError> {
        self.load_file::<SecondarySun>("mapSecondarySuns.jsonl")
    }

    /// Load 'mapSecondarySuns' as map
    pub fn load_secondarysuns_map(&mut self) -> Result<IndexMap<ids::SolarSystemID, SecondarySun>, SDELoadError> {
        self.load_secondarysuns()?.collect()
    }

    /// Load 'mapSolarSystems' as iterator
    pub fn load_solarsystems(&mut self) -> Result<impl Iterator<Item=Result<SolarSystem, SDELoadError>>, SDELoadError> {
        self.load_file::<SolarSystem>("mapSolarSystems.jsonl")
    }

    /// Load 'mapSolarSystems' as map
    pub fn load_solarsystems_map(&mut self) -> Result<IndexMap<ids::SolarSystemID, SolarSystem>, SDELoadError> {
        self.load_solarsystems()?.collect()
    }

    /// Load 'mapStargates' as iterator
    pub fn load_stargates(&mut self) -> Result<impl Iterator<Item=Result<Stargate, SDELoadError>>, SDELoadError> {
        self.load_file::<Stargate>("mapStargates.jsonl")
    }

    /// Load 'mapStargates' as map
    pub fn load_stargates_map(&mut self) -> Result<IndexMap<ids::StargateID, Stargate>, SDELoadError> {
        self.load_stargates()?.collect()
    }

    /// Load 'mapStars' as iterator
    pub fn load_stars(&mut self) -> Result<impl Iterator<Item=Result<Star, SDELoadError>>, SDELoadError> {
        self.load_file::<Star>("mapStars.jsonl")
    }

    /// Load 'mapStars' as map
    pub fn load_stars_map(&mut self) -> Result<IndexMap<ids::StarID, Star>, SDELoadError> {
        self.load_stars()?.collect()
    }

    /// Load 'marketGroups' as iterator
    pub fn load_market_groups(&mut self) -> Result<impl Iterator<Item=Result<MarketGroup, SDELoadError>>, SDELoadError> {
        self.load_file::<MarketGroup>("marketGroups.jsonl")
    }

    /// Load 'marketGroups' as map
    pub fn load_market_groups_map(&mut self) -> Result<IndexMap<ids::MarketGroupID, MarketGroup>, SDELoadError> {
        self.load_market_groups()?.collect()
    }

    /// Load 'masteries' as iterator
    pub fn load_masteries(&mut self) -> Result<impl Iterator<Item=Result<(ids::TypeID, MasteryInfo), SDELoadError>>, SDELoadError> {
        self.load_file::<ExplicitMapEntry<_, _>>("masteries.jsonl")
            .map(|iter| iter.map(|value| value.map(|entry| (entry._key, entry._value))))
    }

    /// Load 'masteries' as map
    pub fn load_masteries_map(&mut self) -> Result<IndexMap<ids::TypeID, MasteryInfo>, SDELoadError> {
        self.load_masteries()?.collect()
    }

    /// Load 'mercenaryTacticalOperations' as iterator
    pub fn load_merc_tactical_operations(&mut self) -> Result<impl Iterator<Item=Result<MercenaryTacticalOperation, SDELoadError>>, SDELoadError> {
        self.load_file::<MercenaryTacticalOperation>("mercenaryTacticalOperations.jsonl")
    }

    /// Load 'mercenaryTacticalOperations' as map
    pub fn load_merc_tactical_operations_map(&mut self) -> Result<IndexMap<ids::DungeonID, MercenaryTacticalOperation>, SDELoadError> {
        self.load_merc_tactical_operations()?.collect()
    }

    /// Load `metenoxMoonDrill` as iterator
    pub fn load_metenox_moon_drill(&mut self) -> Result<impl Iterator<Item=Result<MetenoxMoonDrill, SDELoadError>>, SDELoadError> {
        self.load_file::<MetenoxMoonDrill>("metenoxMoonDrill.jsonl")
    }

    /// Load `metenoxMoonDrill` as map
    pub fn load_metenox_moon_drill_map(&mut self) -> Result<IndexMap<ids::TypeID, MetenoxMoonDrill>, SDELoadError> {
        self.load_metenox_moon_drill()?.collect()
    }

    /// Load 'metaGroups' as iterator
    pub fn load_meta_groups(&mut self) -> Result<impl Iterator<Item=Result<MetaGroup, SDELoadError>>, SDELoadError> {
        self.load_file::<MetaGroup>("metaGroups.jsonl")
    }

    /// Load 'metaGroups' as map
    pub fn load_meta_groups_map(&mut self) -> Result<IndexMap<ids::MetaGroupID, MetaGroup>, SDELoadError> {
        self.load_meta_groups()?.collect()
    }

    /// Load 'militaryCampaigns' as iterator
    pub fn load_military_campaigns(&mut self) -> Result<impl Iterator<Item=Result<MilitaryCampaign, SDELoadError>>, SDELoadError> {
        self.load_file::<MilitaryCampaign>("militaryCampaigns.jsonl")
    }

    /// Load 'militaryCampaigns' as map
    pub fn load_military_campaigns_map(&mut self) -> Result<IndexMap<uuids::MilitaryCampaignID, MilitaryCampaign>, SDELoadError> {
        self.load_military_campaigns()?.collect()
    }

    /// Load 'militaryCampaignObjectives' as iterator
    pub fn load_military_campaign_objectives(&mut self) -> Result<impl Iterator<Item=Result<MilitaryCampaignObjective, SDELoadError>>, SDELoadError> {
        self.load_file::<MilitaryCampaignObjective>("militaryCampaignObjectives.jsonl")
    }

    /// Load 'militaryCampaignObjectives' as map
    pub fn load_military_campaign_objectives_map(&mut self) -> Result<IndexMap<uuids::MilitaryCampaignObjectiveID, MilitaryCampaignObjective>, SDELoadError> {
        self.load_military_campaign_objectives()?.collect()
    }

    /// Load 'missions' as iterator
    pub fn load_missions(&mut self) -> Result<impl Iterator<Item=Result<Mission, SDELoadError>>, SDELoadError> {
        self.load_file::<Mission>("missions.jsonl")
    }

    /// Load 'missions' as map
    pub fn load_missions_map(&mut self) -> Result<IndexMap<ids::MissionID, Mission>, SDELoadError> {
        self.load_missions()?.collect()
    }

    /// Load `notificationTypes` as iterator
    pub fn load_notification_types(&mut self) -> Result<impl Iterator<Item=Result<NotificationType, SDELoadError>>, SDELoadError> {
        self.load_file::<NotificationType>("notificationTypes.jsonl")
    }

    /// Load `notificationTypes` as map
    pub fn load_notification_types_map(&mut self) -> Result<IndexMap<ids::NotificationTypeID, NotificationType>, SDELoadError> {
        self.load_notification_types()?.collect()
    }

    /// Load 'npcCharacters' as iterator
    pub fn load_npc_characters(&mut self) -> Result<impl Iterator<Item=Result<NpcCharacter, SDELoadError>>, SDELoadError> {
        self.load_file::<NpcCharacter>("npcCharacters.jsonl")
    }

    /// Load 'npcCharacters' as map
    pub fn load_npc_characters_map(&mut self) -> Result<IndexMap<ids::CharacterID, NpcCharacter>, SDELoadError> {
        self.load_npc_characters()?.collect()
    }

    /// Load 'npcCorporationDivisions' as iterator
    pub fn load_npc_corporation_divisions(&mut self) -> Result<impl Iterator<Item=Result<CorporationDivision, SDELoadError>>, SDELoadError> {
        self.load_file::<CorporationDivision>("npcCorporationDivisions.jsonl")
    }

    /// Load 'npcCorporationDivisions' as map
    pub fn load_npc_corporation_divisions_map(&mut self) -> Result<IndexMap<ids::DivisionID, CorporationDivision>, SDELoadError> {
        self.load_npc_corporation_divisions()?.collect()
    }

    /// Load 'npcCorporations' as iterator
    pub fn load_npc_corporations(&mut self) -> Result<impl Iterator<Item=Result<NpcCorporation, SDELoadError>>, SDELoadError> {
        self.load_file::<NpcCorporation>("npcCorporations.jsonl")
    }

    /// Load 'npcCorporations' as map
    pub fn load_npc_corporations_map(&mut self) -> Result<IndexMap<ids::CorporationID, NpcCorporation>, SDELoadError> {
        self.load_npc_corporations()?.collect()
    }

    /// Load 'npcStations' as iterator
    pub fn load_npc_stations(&mut self) -> Result<impl Iterator<Item=Result<NpcStation, SDELoadError>>, SDELoadError> {
        self.load_file::<NpcStation>("npcStations.jsonl")
    }

    /// Load 'npcStations' as map
    pub fn load_npc_stations_map(&mut self) -> Result<IndexMap<ids::StationID, NpcStation>, SDELoadError> {
        self.load_npc_stations()?.collect()
    }

    /// Load 'planetResources' as iterator
    pub fn load_planet_resources(&mut self) -> Result<impl Iterator<Item=Result<PlanetResource, SDELoadError>>, SDELoadError> {
        self.load_file::<PlanetResource>("planetResources.jsonl")
    }

    /// Load 'planetResources' as map
    pub fn load_planet_resources_map(&mut self) -> Result<IndexMap<ids::PlanetID, PlanetResource>, SDELoadError> {
        self.load_planet_resources()?.collect()
    }

    /// Load 'planetSchematics' as iterator
    pub fn load_planet_schematics(&mut self) -> Result<impl Iterator<Item=Result<PlanetSchematic, SDELoadError>>, SDELoadError> {
        self.load_file::<PlanetSchematic>("planetSchematics.jsonl")
    }

    /// Load 'planetSchematics' as map
    pub fn load_planet_schematics_map(&mut self) -> Result<IndexMap<ids::PlanetSchematicID, PlanetSchematic>, SDELoadError> {
        self.load_planet_schematics()?.collect()
    }

    /// Load `proximityTrap` as iterator
    pub fn load_proximity_trap(&mut self) -> Result<impl Iterator<Item=Result<ProximityTrap, SDELoadError>>, SDELoadError> {
        self.load_file::<ProximityTrap>("proximityTrap.jsonl")
    }

    /// Load `proximityTrap` as map
    pub fn load_proximity_trap_map(&mut self) -> Result<IndexMap<ids::TypeID, ProximityTrap>, SDELoadError> {
        self.load_proximity_trap()?.collect()
    }

    /// Load 'races' as iterator
    pub fn load_races(&mut self) -> Result<impl Iterator<Item=Result<CharacterRace, SDELoadError>>, SDELoadError> {
        self.load_file::<CharacterRace>("races.jsonl")
    }

    /// Load 'races' as map
    pub fn load_races_map(&mut self) -> Result<IndexMap<ids::RaceID, CharacterRace>, SDELoadError> {
        self.load_races()?.collect()
    }

    /// Load `schoolMap` as iterator
    pub fn load_school_map(&mut self) -> Result<impl Iterator<Item=Result<SchoolMap, SDELoadError>>, SDELoadError> {
        self.load_file::<SchoolMap>("schoolMap.jsonl")
    }

    /// Load `schoolMap` as map
    pub fn load_school_map_map(&mut self) -> Result<IndexMap<ids::SchoolID, ids::StationID>, SDELoadError> {
        self.load_school_map()?.collect()
    }

    /// Load `schools` as iterator
    pub fn load_schools(&mut self) -> Result<impl Iterator<Item=Result<School, SDELoadError>>, SDELoadError> {
        self.load_file::<School>("schools.jsonl")
    }

    /// Load `schools` as map
    pub fn load_schools_map(&mut self) -> Result<IndexMap<ids::SchoolID, School>, SDELoadError> {
        self.load_schools()?.collect()
    }

    /// Load 'shipTreeElements' as iterator
    pub fn load_ship_tree_elements(&mut self) -> Result<impl Iterator<Item=Result<ShipTreeElement, SDELoadError>>, SDELoadError> {
        self.load_file::<ShipTreeElement>("shipTreeElements.jsonl")
    }

    /// Load 'shipTreeElements' as map
    pub fn load_ship_tree_elements_map(&mut self) -> Result<IndexMap<ids::ShipTreeElementID, ShipTreeElement>, SDELoadError> {
        self.load_ship_tree_elements()?.collect()
    }

    /// Load 'shipTreeFactions' as iterator
    pub fn load_ship_tree_factions(&mut self) -> Result<impl Iterator<Item=Result<ShipTreeFaction, SDELoadError>>, SDELoadError> {
        self.load_file::<ShipTreeFaction>("shipTreeFactions.jsonl")
    }

    /// Load 'shipTreeFactions' as map
    pub fn load_ship_tree_factions_map(&mut self) -> Result<IndexMap<ids::FactionID, ShipTreeFaction>, SDELoadError> {
        self.load_ship_tree_factions()?.collect()
    }

    /// Load 'shipTreeGroups' as iterator
    pub fn load_ship_tree_groups(&mut self) -> Result<impl Iterator<Item=Result<ShipTreeGroup, SDELoadError>>, SDELoadError> {
        self.load_file::<ShipTreeGroup>("shipTreeGroups.jsonl")
    }

    /// Load 'shipTreeGroups' as map
    pub fn load_ship_tree_groups_map(&mut self) -> Result<IndexMap<ids::ShipTreeGroupID, ShipTreeGroup>, SDELoadError> {
        self.load_ship_tree_groups()?.collect()
    }

    /// Load `skillPlans` as iterator
    pub fn load_skill_plans(&mut self) -> Result<impl Iterator<Item=Result<SkillPlan, SDELoadError>>, SDELoadError> {
        self.load_file::<SkillPlan>("skillPlans.jsonl")
    }

    /// Load `skillPlans` as map
    pub fn load_skill_plans_map(&mut self) -> Result<IndexMap<u32, SkillPlan>, SDELoadError> {
        self.load_skill_plans()?.collect()
    }

    /// Load 'skinLicenses' as iterator
    pub fn load_skin_licenses(&mut self) -> Result<impl Iterator<Item=Result<SkinLicense, SDELoadError>>, SDELoadError> {
        self.load_file::<SkinLicense>("skinLicenses.jsonl")
    }

    /// Load 'skinLicenses' as map
    pub fn load_skin_licenses_map(&mut self) -> Result<IndexMap<ids::TypeID, SkinLicense>, SDELoadError> {
        self.load_skin_licenses()?.collect()
    }

    /// Load 'skinMaterials' as iterator
    pub fn load_skin_materials(&mut self) -> Result<impl Iterator<Item=Result<SkinMaterial, SDELoadError>>, SDELoadError> {
        self.load_file::<SkinMaterial>("skinMaterials.jsonl")
    }

    /// Load 'skinMaterials' as map
    pub fn load_skin_materials_map(&mut self) -> Result<IndexMap<ids::SkinMaterialID, SkinMaterial>, SDELoadError> {
        self.load_skin_materials()?.collect()
    }

    /// Load `skinrComponentCategories` as iterator
    pub fn load_skinr_component_categories(&mut self) -> Result<impl Iterator<Item=Result<SKINRComponentCategory, SDELoadError>>, SDELoadError> {
        self.load_file::<SKINRComponentCategory>("skinrComponentCategories.jsonl")
    }

    /// Load `skinrComponentCategories` as map
    pub fn load_skinr_component_categories_map(&mut self) -> Result<IndexMap<ids::SKINRComponentCategoryID, SKINRComponentCategory>, SDELoadError> {
        self.load_skinr_component_categories()?.collect()
    }

    /// Load `skinrComponentPointValues` as iterator
    pub fn load_skinr_component_point_values(&mut self) -> Result<impl Iterator<Item=Result<SKINRComponentPointValues, SDELoadError>>, SDELoadError> {
        self.load_file::<SKINRComponentPointValues>("skinrComponentPointValues.jsonl")
    }

    /// Load `skinrComponentPointValues` as map
    pub fn load_skinr_component_point_values_map(&mut self) -> Result<IndexMap<ids::SKINRComponentCategoryID, SKINRComponentPointValues>, SDELoadError> {
        self.load_skinr_component_point_values()?.collect()
    }

    /// Load `skinrComponentRarities` as iterator
    pub fn load_skinr_component_rarities(&mut self) -> Result<impl Iterator<Item=Result<SKINRComponentRarity, SDELoadError>>, SDELoadError> {
        self.load_file::<SKINRComponentRarity>("skinrComponentRarities.jsonl")
    }

    /// Load `skinrComponentRarities` as map
    pub fn load_skinr_component_rarities_map(&mut self) -> Result<IndexMap<u32, SKINRComponentRarity>, SDELoadError> {
        self.load_skinr_component_rarities()?.collect()
    }

    /// Load `skinrComponents` as iterator
    pub fn load_skinr_components(&mut self) -> Result<impl Iterator<Item=Result<SKINRComponent, SDELoadError>>, SDELoadError> {
        self.load_file::<SKINRComponent>("skinrComponents.jsonl")
    }

    /// Load `skinrComponents` as map
    pub fn load_skinr_components_map(&mut self) -> Result<IndexMap<ids::SKINRComponentID, SKINRComponent>, SDELoadError> {
        self.load_skinr_components()?.collect()
    }

    /// Load `skinrSlotCategories` as iterator
    pub fn load_skinr_slot_categories(&mut self) -> Result<impl Iterator<Item=Result<SKINRSlotCategory, SDELoadError>>, SDELoadError> {
        self.load_file::<SKINRSlotCategory>("skinrSlotCategories.jsonl")
    }

    /// Load `skinrSlotCategories` as map
    pub fn load_skinr_slot_categories_map(&mut self) -> Result<IndexMap<ids::SKINRSlotCategoryID, SKINRSlotCategory>, SDELoadError> {
        self.load_skinr_slot_categories()?.collect()
    }

    /// Load `skinrSlotConfigurations` as iterator
    pub fn load_skinr_slot_configurations(&mut self) -> Result<impl Iterator<Item=Result<SKINRSlotConfiguration, SDELoadError>>, SDELoadError> {
        self.load_file::<SKINRSlotConfiguration>("skinrSlotConfigurations.jsonl")
    }

    /// Load `skinrSlotConfigurations` as map
    pub fn load_skinr_slot_configurations_map(&mut self) -> Result<IndexMap<u32, SKINRSlotConfiguration>, SDELoadError> {
        self.load_skinr_slot_configurations()?.collect()
    }

    /// Load `skinrSlotNames` as iterator
    pub fn load_skinr_slot_names(&mut self) -> Result<impl Iterator<Item=Result<SKINRSlotName, SDELoadError>>, SDELoadError> {
        self.load_file::<SKINRSlotName>("skinrSlotNames.jsonl")
    }

    /// Load `skinrSlotNames` as map
    pub fn load_skinr_slot_names_map(&mut self) -> Result<IndexMap<u32, SKINRSlotName>, SDELoadError> {
        self.load_skinr_slot_names()?.collect()
    }

    /// Load `skinrSlots` as iterator
    pub fn load_skinr_slots(&mut self) -> Result<impl Iterator<Item=Result<SKINRSlot, SDELoadError>>, SDELoadError> {
        self.load_file::<SKINRSlot>("skinrSlots.jsonl")
    }

    /// Load `skinrSlots` as map
    pub fn load_skinr_slots_map(&mut self) -> Result<IndexMap<u32, SKINRSlot>, SDELoadError> {
        self.load_skinr_slots()?.collect()
    }

    /// Load `skinrSlotsToMaterials` as iterator
    pub fn load_skinr_slots_to_materials(&mut self) -> Result<impl Iterator<Item=Result<SKINRSlotsToMaterials, SDELoadError>>, SDELoadError> {
        self.load_file::<SKINRSlotsToMaterials>("skinrSlotsToMaterials.jsonl")
    }

    /// Load `skinrSlotsToMaterials` as map
    pub fn load_skinr_slots_to_materials_map(&mut self) -> Result<IndexMap<u32, SKINRSlotsToMaterials>, SDELoadError> {
        self.load_skinr_slots_to_materials()?.collect()
    }

    /// Load `skinrTierThresholds` as iterator
    pub fn load_skinr_tier_thresholds(&mut self) -> Result<impl Iterator<Item=Result<SKINRTierThreshold, SDELoadError>>, SDELoadError> {
        self.load_file::<SKINRTierThreshold>("skinrTierThresholds.jsonl")
    }

    /// Load `skinrTierThresholds` as map
    pub fn load_skinr_tier_thresholds_map(&mut self) -> Result<IndexMap<u32, SKINRTierThreshold>, SDELoadError> {
        self.load_skinr_tier_thresholds()?.collect()
    }

    /// Load 'skins' as iterator
    pub fn load_skins(&mut self) -> Result<impl Iterator<Item=Result<Skin, SDELoadError>>, SDELoadError> {
        self.load_file::<Skin>("skins.jsonl")
    }

    /// Load 'skins' as map
    pub fn load_skins_map(&mut self) -> Result<IndexMap<ids::SkinID, Skin>, SDELoadError> {
        self.load_skins()?.collect()
    }

    /// Load 'sovereigntyUpgrades' as iterator
    pub fn load_sovereignty_upgrades(&mut self) -> Result<impl Iterator<Item=Result<SovereigntyUpgrade, SDELoadError>>, SDELoadError> {
        self.load_file::<SovereigntyUpgrade>("sovereigntyUpgrades.jsonl")
    }

    /// Load 'sovereigntyUpgrades' as map
    pub fn load_sovereignty_upgrades_map(&mut self) -> Result<IndexMap<ids::TypeID, SovereigntyUpgrade>, SDELoadError> {
        self.load_sovereignty_upgrades()?.collect()
    }

    /// Load 'stationOperations' as iterator
    pub fn load_station_operations(&mut self) -> Result<impl Iterator<Item=Result<StationOperation, SDELoadError>>, SDELoadError> {
        self.load_file::<StationOperation>("stationOperations.jsonl")
    }

    /// Load 'stationOperations' as map
    pub fn load_station_operations_map(&mut self) -> Result<IndexMap<ids::StationOperationID, StationOperation>, SDELoadError> {
        self.load_station_operations()?.collect()
    }

    /// Load 'stationServices' as iterator
    pub fn load_station_services(&mut self) -> Result<impl Iterator<Item=Result<StationService, SDELoadError>>, SDELoadError> {
        self.load_file::<StationService>("stationServices.jsonl")
    }

    /// Load 'stationServices' as map
    pub fn load_station_services_map(&mut self) -> Result<IndexMap<ids::StationServiceID, StationService>, SDELoadError> {
        self.load_station_services()?.collect()
    }

    /// Load `stationStandingsRestrictions` as iterator
    pub fn load_station_standings_restrictions(&mut self) -> Result<impl Iterator<Item=Result<StationStandingsRestriction, SDELoadError>>, SDELoadError> {
        self.load_file::<StationStandingsRestriction>("stationStandingsRestrictions.jsonl")
    }

    /// Load `stationStandingsRestrictions` as map
    pub fn load_station_standings_restrictions_map(&mut self) -> Result<IndexMap<ids::FactionID, StationStandingsRestriction>, SDELoadError> {
        self.load_station_standings_restrictions()?.collect()
    }

    /// Load `systemDbuffEmitters` as iterator
    pub fn load_system_dbuff_emitters(&mut self) -> Result<impl Iterator<Item=Result<SystemDynamicbuffEmitter, SDELoadError>>, SDELoadError> {
        self.load_file::<SystemDynamicbuffEmitter>("systemDbuffEmitters.jsonl")
    }

    /// Load `systemDbuffEmitters` as map
    pub fn load_system_dbuff_emitters_map(&mut self) -> Result<IndexMap<ids::TypeID, SystemDynamicbuffEmitter>, SDELoadError> {
        self.load_system_dbuff_emitters()?.collect()
    }

    /// Load `systemWideEffects` as iterator
    pub fn load_system_wide_effects(&mut self) -> Result<impl Iterator<Item=Result<SystemWideEffect, SDELoadError>>, SDELoadError> {
        self.load_file::<SystemWideEffect>("systemWideEffects.jsonl")
    }

    /// Load `systemWideEffects` as map
    pub fn load_system_wide_effects_map(&mut self) -> Result<IndexMap<ids::TypeID, SystemWideEffect>, SDELoadError> {
        self.load_system_wide_effects()?.collect()
    }

    /// Load 'translationLanguages' as iterator
    pub fn load_translation_languages(&mut self) -> Result<impl Iterator<Item=Result<TranslationLanguage, SDELoadError>>, SDELoadError> {
        self.load_file::<_>("translationLanguages.jsonl")
    }

    /// Load 'translationLanguages' as vec
    pub fn load_translation_languages_list(&mut self) -> Result<Vec<TranslationLanguage>, SDELoadError> {
        self.load_translation_languages()?.collect()
    }

    /// Load 'typeBonus' as iterator
    pub fn load_type_bonuses(&mut self) -> Result<impl Iterator<Item=Result<TypeBonuses, SDELoadError>>, SDELoadError> {
        self.load_file::<TypeBonuses>("typeBonus.jsonl")
    }

    /// Load 'typeBonus' as map
    pub fn load_type_bonuses_map(&mut self) -> Result<IndexMap<ids::TypeID, TypeBonuses>, SDELoadError> {
        self.load_type_bonuses()?.collect()
    }

    /// Load 'typeDogma' as iterator
    pub fn load_type_dogma(&mut self) -> Result<impl Iterator<Item=Result<TypeDogma, SDELoadError>>, SDELoadError> {
        self.load_file::<TypeDogma>("typeDogma.jsonl")
    }

    /// Load 'typeDogma' as map
    pub fn load_type_dogma_map(&mut self) -> Result<IndexMap<ids::TypeID, TypeDogma>, SDELoadError> {
        self.load_type_dogma()?.collect()
    }

    /// Load `typeElements` as iterator
    pub fn load_type_elements(&mut self) -> Result<impl Iterator<Item=Result<TypeElement, SDELoadError>>, SDELoadError> {
        self.load_file::<TypeElement>("typeElements.jsonl")
    }

    /// Load `typeElements` as map
    pub fn load_type_elements_map(&mut self) -> Result<IndexMap<ids::TypeID, TypeElement>, SDELoadError> {
        self.load_type_elements()?.collect()
    }

    /// Load 'typelist' as iterator
    pub fn load_type_lists(&mut self) -> Result<impl Iterator<Item=Result<TypeList, SDELoadError>>, SDELoadError> {
        self.load_file::<TypeList>("typeLists.jsonl")
    }

    /// Load 'typelist' as map
    pub fn load_type_lists_map(&mut self) -> Result<IndexMap<ids::TypeListID, TypeList>, SDELoadError> {
        self.load_type_lists()?.collect()
    }

    /// Load 'typeMaterials' as iterator
    pub fn load_type_materials(&mut self) -> Result<impl Iterator<Item=Result<TypeMaterials, SDELoadError>>, SDELoadError> {
        self.load_file::<TypeMaterials>("typeMaterials.jsonl")
    }

    /// Load 'typeMaterials' as map
    pub fn load_type_materials_map(&mut self) -> Result<IndexMap<ids::TypeID, TypeMaterials>, SDELoadError> {
        self.load_type_materials()?.collect()
    }

    /// Load 'types' as iterator
    pub fn load_types(&mut self) -> Result<impl Iterator<Item=Result<Type, SDELoadError>>, SDELoadError> {
        self.load_file::<Type>("types.jsonl")
    }

    /// Load 'types' as map
    pub fn load_types_map(&mut self) -> Result<IndexMap<ids::TypeID, Type>, SDELoadError> {
        self.load_types()?.collect()
    }

    pub fn full(&mut self) -> Result<SDE_Full, SDELoadError> {
        Ok(SDE_Full {
            accounting_entry_types: self.load_accounting_entry_types_map()?,
            agent_types: self.load_agent_types_map()?,
            agents_in_space: self.load_agents_in_space_map()?,
            ancestries: self.load_ancestries_map()?,
            applied_proximity_effects: self.load_applied_proximity_effects_map()?,
            archetypes: self.load_archetypes_map()?,
            bloodlines: self.load_bloodlines_map()?,
            blueprints: self.load_blueprints_map()?,
            categories: self.load_categories_map()?,
            certificates: self.load_certificates_map()?,
            character_attributes: self.load_character_attributes_map()?,
            character_titles: self.load_character_titles_map()?,
            clone_grades: self.load_clone_grades_map()?,
            compressible_types: self.load_compressible_types_map()?,
            contraband_types: self.load_contraband_types_map()?,
            control_tower_resources: self.load_controltower_resources_map()?,
            corporation_activities: self.load_corporation_activities_map()?,
            corporation_role_groups: self.load_corporation_role_groups_map()?,
            corporation_roles: self.load_corporation_roles_map()?,
            dbuff_collections: self.load_dbuff_collections_map()?,
            dogma_attribute_categories: self.load_dogma_attribute_categories_map()?,
            dogma_attributes: self.load_dogma_attributes_map()?,
            dogma_effects: self.load_dogma_effects_map()?,
            dogma_units: self.load_dogma_units_map()?,
            dungeons: self.load_dungeons_map()?,
            dynamic_item_attributes: self.load_dynamic_item_attributes_map()?,
            epic_arcs: self.load_epic_arcs_map()?,
            expert_systems: self.load_expert_systems_map()?,
            factions: self.load_factions_map()?,
            fighter_abilities: self.load_fighter_abilities_map()?,
            fighter_abilities_by_type: self.load_fighter_abilities_by_type_map()?,
            freelance_job_schemas: self.load_freelance_job_schemas_map()?,
            graphic_material_sets: self.load_graphic_material_sets_map()?,
            graphics: self.load_graphics_map()?,
            groups: self.load_groups_map()?,
            icons: self.load_icons_map()?,
            industry_activities: self.load_industry_activities_map()?,
            industry_assembly_lines: self.load_industry_assembly_lines_map()?,
            industry_installation_types: self.load_industry_installation_types_map()?,
            industry_modifier_sources: self.load_industry_modifier_sources_map()?,
            industry_target_filters: self.load_industry_target_filters_map()?,
            landmarks: self.load_landmarks_map()?,
            link_with_ship: self.load_link_with_ship_map()?,
            map_asteroid_belts: self.load_asteroid_belts_map()?,
            map_constellations: self.load_constellations_map()?,
            map_moons: self.load_moons_map()?,
            map_planets: self.load_planets_map()?,
            map_regions: self.load_regions_map()?,
            map_secondarysuns: self.load_secondarysuns_map()?,
            map_solarsystems: self.load_solarsystems_map()?,
            map_stargates: self.load_stargates_map()?,
            map_stars: self.load_stars_map()?,
            market_groups: self.load_market_groups_map()?,
            masteries: self.load_masteries_map()?,
            mercenary_tactical_operations: self.load_merc_tactical_operations_map()?,
            meta_groups: self.load_meta_groups_map()?,
            metenox_moon_drill: self.load_metenox_moon_drill_map()?,
            military_campaigns: self.load_military_campaigns_map()?,
            military_campaign_objectives: self.load_military_campaign_objectives_map()?,
            missions: self.load_missions_map()?,
            notification_types: self.load_notification_types_map()?,
            npc_characters: self.load_npc_characters_map()?,
            npc_corporation_divisions: self.load_npc_corporation_divisions_map()?,
            npc_corporations: self.load_npc_corporations_map()?,
            npc_stations: self.load_npc_stations_map()?,
            planet_resources: self.load_planet_resources_map()?,
            planet_schematics: self.load_planet_schematics_map()?,
            proximity_trap: self.load_proximity_trap_map()?,
            races: self.load_races_map()?,
            school_map: self.load_school_map_map()?,
            schools: self.load_schools_map()?,
            ship_tree_elements: self.load_ship_tree_elements_map()?,
            ship_tree_factions: self.load_ship_tree_factions_map()?,
            ship_tree_groups: self.load_ship_tree_groups_map()?,
            skill_plans: self.load_skill_plans_map()?,
            skin_licenses: self.load_skin_licenses_map()?,
            skin_materials: self.load_skin_materials_map()?,
            skinr_component_categories: self.load_skinr_component_categories_map()?,
            skinr_component_point_values: self.load_skinr_component_point_values_map()?,
            skinr_component_rarities: self.load_skinr_component_rarities_map()?,
            skinr_components: self.load_skinr_components_map()?,
            skinr_slot_categories: self.load_skinr_slot_categories_map()?,
            skinr_slot_configurations: self.load_skinr_slot_configurations_map()?,
            skinr_slot_names: self.load_skinr_slot_names_map()?,
            skinr_slots: self.load_skinr_slots_map()?,
            skinr_slots_to_materials: self.load_skinr_slots_to_materials_map()?,
            skinr_tier_thresholds: self.load_skinr_tier_thresholds_map()?,
            skins: self.load_skins_map()?,
            sovereignty_upgrades: self.load_sovereignty_upgrades_map()?,
            station_operations: self.load_station_operations_map()?,
            station_services: self.load_station_services_map()?,
            station_standings_restrictions: self.load_station_standings_restrictions_map()?,
            system_dbuff_emitters: self.load_system_dbuff_emitters_map()?,
            system_wide_effects: self.load_system_wide_effects_map()?,
            translation_languages: self.load_translation_languages_list()?,
            type_bonus: self.load_type_bonuses_map()?,
            type_dogma: self.load_type_dogma_map()?,
            type_elements: self.load_type_elements_map()?,
            type_lists: self.load_type_lists_map()?,
            type_materials: self.load_type_materials_map()?,
            types: self.load_types_map()?
        })
    }
}