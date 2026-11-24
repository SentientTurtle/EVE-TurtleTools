use std::fmt::Display;
use std::hash::Hash;
use std::marker::PhantomData;
use indexmap::IndexMap;
use serde::{Deserialize, Deserializer, Serialize};
use serde::de::{SeqAccess, Unexpected, Visitor};
use crate::types::{ids, uuids, values};
use crate::util::reflist::RefList;
use crate::util::units::EVEUnit;

/// Helper trait for `deserialize_inline_entry_map`
trait InlineEntry<K> {
    fn key(&self) -> K;
}

/// Deserialize a json-array of [`InlineEntry`]-trait values into an IndexMap
fn deserialize_inline_entry_map<'de, K: Deserialize<'de> + Hash + Eq + Ord, V: Deserialize<'de> + InlineEntry<K>, D: Deserializer<'de>>(deserializer: D) -> Result<IndexMap<K, V>, D::Error> {
    struct EntryVisitor<K, V>(PhantomData<K>, PhantomData<V>);
    impl<'de, K: Deserialize<'de> + Hash + Eq + Ord, V: Deserialize<'de> + InlineEntry<K>> Visitor<'de> for EntryVisitor<K, V> {
        type Value = IndexMap<K, V>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
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

// Helper macro for proxy-deserialization
macro_rules! deserialize_with_proxy {
    ($name:ident, $keyname:ident, $key:ty, $valname:ident, $val:ty, SEQTOMAP) => {
        fn $name<'de, D: Deserializer<'de>>(deserializer: D) -> Result<IndexMap<$key, $val>, D::Error> {
            #[derive(Deserialize)]
            #[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
            struct Entry {
                $keyname: $key,
                $valname: $val
            }
            struct EntryVisitor;
            impl<'de> Visitor<'de> for EntryVisitor {
                type Value = IndexMap<$key, $val>;

                fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                    formatter.write_str("map encoded as array of entries")
                }

                fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error> where A: SeqAccess<'de> {
                    let size_hint = seq.size_hint();
                    let mut map = size_hint.map(IndexMap::with_capacity).unwrap_or_else(IndexMap::new);
                    while let Some(value) = seq.next_element::<Entry>()? {
                        map.insert(value.$keyname, value.$valname);
                    }
                    Ok(map)
                }
            }

            deserializer.deserialize_seq(EntryVisitor)
        }
    };
    ($name:ident, $field:ident, $val:ty, SEQTOSEQ) => {
        fn $name<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<$val>, D::Error> {
            #[derive(Deserialize)]
            #[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
            struct Entry {
                $field: $val
            }
            struct EntryVisitor;
            impl<'de> Visitor<'de> for EntryVisitor {
                type Value = Vec<$val>;

                fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                    formatter.write_str("list encoded as array of entries")
                }

                fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error> where A: SeqAccess<'de> {
                    let size_hint = seq.size_hint();
                    let mut vec = size_hint.map(Vec::with_capacity).unwrap_or_else(Vec::new);
                    while let Some(value) = seq.next_element::<Entry>()? {
                        vec.push(value.$field);
                    }
                    Ok(vec)
                }
            }

            deserializer.deserialize_seq(EntryVisitor)
        }
    };
}

deserialize_with_proxy!(deserialize_skill_level_map, typeID, ids::TypeID, level, values::SkillLevel, SEQTOMAP);
deserialize_with_proxy!(deserialize_dbuffs_generic, dbuffID, ids::DynamicBuffID, value, f64, SEQTOMAP);

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

/// Position of an object on the New Eden cluster map, units in metres.
///
/// See <https://developers.eveonline.com/docs/guides/map-data/> for detailed explanation
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
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
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
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
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
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
pub struct AccountingEntryType {
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
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
pub(crate) struct AgentTypeEntry {
    /// Identifier for this AgentType
    pub(crate) agentTypeID: ids::AgentTypeID,
    /// Name for this AgentType
    pub(crate) name: AgentType
}

/// Agent (Mission NPC) that is located in space, rather than docked in a station
///
/// Additional Agent information is contained in [`NpcCharacter`] data
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct AgentInSpace {
    /// CharacterID for this agent
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
pub struct Ancestry {
    /// Identifier for this Ancestry
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
pub struct AppliedProximityEffect {
    /// TypeID for the celestial object this effect is centered on
    pub typeID: ids::TypeID,
    #[serde(deserialize_with="deserialize_dbuffs_generic")]
    pub dbuffs: IndexMap<ids::DynamicBuffID, f64>,
    pub radius: f64,
    pub delaySeconds: f64
}

impl_map_collect!(ids::TypeID, AppliedProximityEffect, typeID);

/// Dungeon Archetype
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct Archetype {
    /// ID for this Archetype
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
pub struct Bloodline {
    /// Identifier for this Bloodline
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
pub struct Blueprint {
    /// Blueprint TypeID.
    #[allow(unused)]
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
pub struct BPActivity {
    /// Materials and quantity required for one run of this activity
    #[serde(default)]
    #[serde(deserialize_with="deserialize_activity_materials")]
    pub materials: IndexMap<ids::TypeID, u32>,
    /// Products, quantity, and optional probability for one run of this activity.
    /// Only one product type is allowed per run of this activity; When multiple types of products are available, one must be selected by the player when setting up the industry job
    #[serde(default)]
    #[serde(deserialize_with="deserialize_activity_products")]
    pub products: IndexMap<ids::TypeID, (u32, Option<f64>)>,
    /// Skills required to set up a run of this activity
    #[serde(default)]
    #[serde(deserialize_with="deserialize_skill_level_map")]
    pub skills: IndexMap<ids::TypeID, values::SkillLevel>,
    /// Time required for one run of this activity, in seconds
    pub time: u32
}

deserialize_with_proxy!(deserialize_activity_materials, typeID, ids::TypeID, quantity, u32, SEQTOMAP);
fn deserialize_activity_products<'de, D: Deserializer<'de>>(deserializer: D) -> Result<IndexMap<ids::TypeID, (u32, Option::<f64>)>, D::Error> {
    #[derive(Debug, Deserialize)]
    #[allow(non_snake_case)]    // TODO: Move to module level
    #[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
    struct BPProduct { typeID: ids::TypeID, quantity: u32, probability: Option<f64> }

    struct EntryVisitor;
    impl<'de> Visitor<'de> for EntryVisitor {
        type Value = IndexMap<ids::TypeID, (u32, Option::<f64>)>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("map encoded as array of entries")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let size_hint = seq.size_hint();
            let mut map = size_hint.map(IndexMap::with_capacity).unwrap_or_else(IndexMap::new);
            while let Some(e) = seq.next_element::<BPProduct>()? {
                map.insert(e.typeID, (e.quantity, e.probability));
            }
            Ok(map)
        }
    }

    deserializer.deserialize_seq(EntryVisitor)
}

impl_map_collect!(ids::TypeID, Blueprint, blueprintTypeID);


/// Item Type 'Category'; Collection of [Groups](Group)
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct Category {
    /// ID for this category
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
pub struct Certificate {
    /// ID for this certificate
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
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct CertificateSkillLevels {
    /// Skill this 'levels' data is for
    pub typeID: ids::TypeID,
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
        self.typeID
    }
}

impl_map_collect!(ids::CertificateID, Certificate, certificateID);

/// Character skill training Attribute
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct CharacterAttribute {
    /// ID for this character attribute
    pub attributeID: ids::CharacterAttributeID,
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

impl_map_collect!(ids::CharacterAttributeID, CharacterAttribute, attributeID);

/// Character title, cosmetic text tag earned through achievements
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct CharacterTitle {
    /// ID for this character attribute
    pub titleID: uuids::CharacterTitleID,
    /// Title text, as displayed on the character sheet
    pub name: LocalizedString
}

impl_map_collect!(uuids::CharacterTitleID, CharacterTitle, titleID);

/// Information about Alpha clones
/// Currently there is one entry for each of the 4 races' Alpha Clones, but the entries are the same; Each character race is allowed to train the same skills, including ships of the other races
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct CloneGrade {
    /// ID for this clone grade
    pub cloneGradeID: ids::CloneGradeID,
    /// Name (Not displayed in-game)
    pub name: String,
    /// Skills that may be trained by this clone grade
    #[serde(deserialize_with="deserialize_skill_level_map")]
    pub skills: IndexMap<ids::TypeID, values::SkillLevel>
}

impl_map_collect!(ids::CloneGradeID, CloneGrade, cloneGradeID);

/// Information about ore/gas/ice compression
/// Ore compression has a 1:1 input output ratio, N units of 'oreTypeID' yield N units of 'compressedTypeID'
/// Volume ratio is provided by `compressedType.volume / oreType.volume`
/// For ore and ice, compression is lossless. Gas must be decompressed before use, where some losses are had. (Depending on skills & facility used)
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct CompressibleType {
    /// Ore (input) typeID
    #[serde(rename="typeID")]
    pub oreTypeID: ids::TypeID,
    /// Compressed ore (output) typeID
    pub compressedTypeID: ids::TypeID
}

impl_map_collect!(ids::TypeID, ids::TypeID, CompressibleType, fn |c| (c.oreTypeID, c.compressedTypeID));

/// Contraband status information for a [`Type`]
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct ContrabandType {
    /// Type for which this Contraband information applies
    pub typeID: ids::TypeID,
    /// Per-faction contraband info; An entry means the Type is contraband in the given faction
    #[serde(deserialize_with="deserialize_inline_entry_map")]
    pub factions: IndexMap<ids::FactionID, ContrabandFactionInfo>
}

/// Per-faction Contraband information
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct ContrabandFactionInfo {
    /// Faction for which this info applies
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
pub struct ControlTowerResources {
    /// TypeID of the Control Tower type this information applies to
    pub typeID: ids::TypeID,
    /// Resources required for the operation of this Control Tower
    pub resources: Vec<ControlTowerResourceInfo>
}

/// Resources required for Player-owned-Starbase Control Tower operation
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
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
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self.name(), f)
    }
}

impl_map_collect!(ids::TypeID, ControlTowerResources, typeID);

/// NPC Station Activity/"Specialization"
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct CorporationActivity {
    /// ID for this activity
    pub activityID: ids::CorporationActivityID,
    /// Name for this activity
    pub name: LocalizedString
}

impl_map_collect!(ids::CorporationActivityID, CorporationActivity, activityID);


/// Grouping of corporation permission roles
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct CorporationRoleGroup {
    /// ID for this role group
    pub roleGroupID: ids::CorporationRoleGroupID,
    pub name: LocalizedString,
    pub appliesTo: CorporationRoleGroupAppliesTo,
    pub appliesToGrantable: CorporationRoleGroupAppliesToGrantable,
    pub isDivisional: bool,
    pub isLocational: bool,
}

#[derive(Debug, Deserialize)]
#[allow(non_camel_case_types)]
pub enum CorporationRoleGroupAppliesTo {
    roles,
    rolesAtHQ,
    rolesAtBase,
    rolesAtOther
}

#[derive(Debug, Deserialize)]
#[allow(non_camel_case_types)]
pub enum CorporationRoleGroupAppliesToGrantable {
    grantableRoles,
    grantableRolesAtHQ,
    grantableRolesAtBase,
    grantableRolesAtOther
}

impl_map_collect!(ids::CorporationRoleGroupID, CorporationRoleGroup, roleGroupID);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct CorporationRole {
    /// ID for this role
    pub roleID: ids::CorporationRoleID,
    pub name: LocalizedString,
    pub shortName: String,
    pub description: LocalizedString,
    #[serde(default)]
    pub roleGroupIDs: Vec<ids::CorporationRoleGroupID>
}

impl_map_collect!(ids::CorporationRoleID, CorporationRole, roleID);

/// 'Dynamic Buff'; e.g. Command Burst bonus effects
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct DynamicBuff {
    /// ID for this warfare buff. Referenced by attributes on Command Burst charges
    pub dbuffCollectionID: ids::DynamicBuffID,
    /// Aggregate mode for multiple buffs; Whether the maximum or minimum value is selected when multiple buffs of different strengths are applied to a ship
    pub aggregateMode: DynamicBuffAggregateMode,
    /// Developer description, in English
    pub developerDescription: String,
    /// Display name, as shown in tooltip in-game
    pub displayName: Option<LocalizedString>,
    /// Attributes whose effects are applied as Item Modifiers
    #[serde(default)]
    #[serde(deserialize_with="deserialize_dynamicbuff_attribute_list")]
    pub itemModifiers: Vec<ids::AttributeID>,
    /// Attributes whose effects are applied as Location Group Modifiers
    #[serde(default)]
    pub locationGroupModifiers: Vec<DynamicBuffLocationGroupModifier>,
    /// Attributes whose effects are applied as Location Modifiers
    #[serde(default)]
    #[serde(deserialize_with="deserialize_dynamicbuff_attribute_list")]
    pub locationModifiers: Vec<ids::AttributeID>,
    /// Attributes whose effects are applied as Location with-required-skill Modifiers
    #[serde(default)]
    pub locationRequiredSkillModifiers: Vec<DynamicBuffLocationRequiredSkillModifier>,
    /// Operation applied by modifiers
    pub operationName: DynamicBuffOperation,
    /// How the effect value is displayed in-game
    pub showOutputValueInUI: DynamicBuffUIMode
}
deserialize_with_proxy!(deserialize_dynamicbuff_attribute_list, dogmaAttributeID, ids::AttributeID, SEQTOSEQ);

/// Aggregate mode for warfare buff effect stacking
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
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
pub enum DynamicBuffOperation {
    // Dogma is weird and complicated, so no individual docs on these
    PostMul, PostPercent, ModAdd, PreAssignment, PostAssignment
}

/// Warfare buff display mode
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
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
pub struct DynamicBuffLocationRequiredSkillModifier {
    /// Attribute source for effect
    pub dogmaAttributeID: ids::AttributeID,
    /// Skill required by applicable types
    pub skillID: ids::TypeID
}

impl_map_collect!(ids::DynamicBuffID, DynamicBuff, dbuffCollectionID);


/// Attribute Category, grouping of [`Attribute`]
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct AttributeCategory {
    /// ID for this category
    pub categoryID: ids::AttributeCategoryID,
    /// Category name, in English
    pub name: String,
    /// Description, in English
    pub description: Option<String>
}

impl_map_collect!(ids::AttributeCategoryID, AttributeCategory, categoryID);

/// Dogma Attribute, describing properties for [`Type`]s. Such as HP, maximum velocity, and other item stats
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct Attribute {
    /// ID for this attribute
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
    pub unitID: Option<EVEUnit>,
}

impl_map_collect!(ids::AttributeID, Attribute, attributeID);

/// Dogma Effect, describing interactions of [`Attribute`]s
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct Effect {
    /// ID for this effect
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
pub struct DogmaUnit {
    /// UnitID for this unit, encoded as an [`EVEUnit`] enum
    ///
    /// EVEUnit provides formatting functions
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
pub struct Dungeon {
    /// ID for this dungeon
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
pub struct DynamicItemAttributes {
    /// Mutaplasmid typeID
    pub typeID: ids::TypeID,
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
pub struct DynamicAttributeInfo {
    /// AttributeID that is modified by mutaplasmid
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
pub struct DynamicItemAttributesIOMapping {
    /// List of typeIDs the parent mutaplasmid can be applied to
    pub applicableTypes: Vec<ids::TypeID>,
    /// Output type of modified module
    pub resultingType: ids::TypeID
}

impl_map_collect!(ids::TypeID, DynamicItemAttributes, typeID);

/// Epic Arc
///
/// Replayable Mission "arc"/storyline
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct EpicArc {
    /// Epic Arc ID
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
pub struct EpicArcMission {
    /// Mission ID
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
    fn key(&self) -> ids::MissionID {
        self.missionID
    }
}

/// Temporary skill unlock package
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct ExpertSystem {
    /// Expert System inventory type
    pub typeID: ids::TypeID,
    #[serde(default)]
    pub associatedShipTypes: Vec<ids::TypeID>,
    pub durationDays: u32,
    pub hidden: bool,
    pub internalName: String,
    pub retired: bool,
    #[serde(deserialize_with="deserialize_skill_level_map")]
    pub skillsGranted: IndexMap<ids::TypeID, values::SkillLevel>
}

impl_map_collect!(ids::TypeID, ExpertSystem, typeID);

/// The major and minor NPC factions
///
/// e.g. Caldari/Minmatar/Amarr/Gallente but also CONCORD, ORE, and SOE
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct Faction {
    /// FactionID
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
pub struct FighterAbility {
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
pub enum FighterAbilityTargetMode {
    itemTargeted,
    pointTargeted,
    untargeted
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct FighterAbilitiesByType {
    pub typeID: ids::TypeID,
    pub abilitySlot0: FighterAbilityInfo,
    pub abilitySlot1: FighterAbilityInfo,
    pub abilitySlot2: Option<FighterAbilityInfo>,
}

impl_map_collect!(ids::TypeID, FighterAbilitiesByType, typeID);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct FighterAbilityInfo {
    pub abilityID: ids::FighterAbilityID,
    pub cooldownSeconds: Option<u32>,
    pub charges: Option<FighterAbilityCharges>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct FighterAbilityCharges {
    pub chargeCount: u32,
    pub rearmTimeSeconds: u32
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct FreelanceJobSchemas {
    pub major: u32,
    pub schemas: Vec<FreelanceJobSchema>
}
impl_map_collect!(u32, Vec<FreelanceJobSchema>, FreelanceJobSchemas, fn |f| (f.major, f.schemas));

/// Freelance job schema, describes the possible kinds of freelance job
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct FreelanceJobSchema {
    /// Type of job this schema describes
    pub name: JobType,
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
pub struct JobSchemaParameter {
    /// Parameter name
    ///
    /// Parameter type is stored in one of the fields below
    pub key: String,
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
        self.key.clone()
    }
}

/// Title and description for a boolean job schema parameter
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct JobSchemaParameterBooleanOption {
    pub title: LocalizedString,
    pub description: LocalizedString,
}

/// Boolean-type parameter
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
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
pub struct GraphicMaterialSet {
    /// ID for this material set
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
pub struct Graphic {
    /// ID for this graphic
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
pub struct Group {
    /// ID for this group
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
pub struct Icon {
    /// ID for this icon
    pub iconID: ids::IconID,
    /// Cache resource for this icon
    pub iconFile: values::CacheResource
}

impl_map_collect!(ids::IconID, Icon, iconID);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct IndustryActivity {
    pub activityID: ids::IndustryActivityID,
    pub name: String,
    pub description: String
}

impl_map_collect!(ids::IndustryActivityID, IndustryActivity, activityID);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct IndustryAssemblyLine {
    pub assemblyLineID: ids::IndustryAssemblyLineID,
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
pub struct IndustryAssemblyLineDetailsPerCategory {
    pub categoryID: ids::CategoryID,
    pub costMultiplier: Option<f64>,
    pub materialMultiplier: f64,
    pub timeMultiplier: f64,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct IndustryAssemblyLineDetailsPerGroup {
    pub groupID: ids::GroupID,
    pub costMultiplier: Option<f64>,
    pub materialMultiplier: f64,
    pub timeMultiplier: f64,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct IndustryAssemblyLineDetailsPerTypeList {
    pub typeListID: ids::TypeListID,
    pub costMultiplier: Option<f64>,
    pub materialMultiplier: f64,
    pub timeMultiplier: f64,
}

impl_map_collect!(ids::IndustryAssemblyLineID, IndustryAssemblyLine, assemblyLineID);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct IndustryInstallationType {
    pub typeID: ids::TypeID,
    #[serde(deserialize_with="deserialize_industry_installation_type_assemblylines")]
    pub assemblyLines: Vec<ids::IndustryAssemblyLineID>
}

deserialize_with_proxy!(deserialize_industry_installation_type_assemblylines, assemblyLineID, ids::IndustryAssemblyLineID, SEQTOSEQ);
impl_map_collect!(ids::TypeID, IndustryInstallationType, typeID);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct IndustryModifierSource {
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
pub struct IndustryModifierAttribute {
    pub dogmaAttributeID: ids::AttributeID,
    pub filterID: Option<ids::IndustryFilterID>
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct IndustryTargetFilter {
    pub targetFilterID: ids::IndustryFilterID,
    pub name: String,
    #[serde(default)]
    pub categoryIDs: Vec<ids::CategoryID>,
    #[serde(default)]
    pub groupIDs: Vec<ids::GroupID>,
}

impl_map_collect!(ids::IndustryFilterID, IndustryTargetFilter, targetFilterID);

/// Landmark in the game world
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct Landmark {
    /// ID for this landmark
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
pub struct LinkWithShip {
    /// TypeID of the linking object
    pub typeID: ids::TypeID,
    pub applyPvpFlag: bool,
    pub canRelink: bool,
    #[serde(deserialize_with="deserialize_dbuffs_generic")]
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
pub struct AsteroidBelt {
    /// ID for this asteroid belt, unique for each belt in the game
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
pub struct Constellation {
    /// ID for this constellation
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
pub struct Moon {
    /// ID for this moon
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
pub struct Planet {
    /// ID for this planet
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
pub struct Region {
    /// ID for this region
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
pub struct SecondarySun {
    /// CelestialID for the star object
    pub sunID: ids::ItemID,
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
pub struct SolarSystem {
    /// ID for this solarsystem
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
pub struct Stargate {
    /// ID for this stargate
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
pub struct Star {
    /// ID for this star
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
pub struct MarketGroup {
    /// ID for this market group
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
#[derive(Debug, Deserialize)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct TypeMasteries {
    pub typeID: ids::TypeID,
    pub levels: MasteryInfo
}
impl_map_collect!(ids::TypeID, MasteryInfo, TypeMasteries, fn |m| (m.typeID, m.levels));

#[derive(Debug)]
#[allow(non_snake_case)]
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

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
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

                #[derive(Deserialize)]
                #[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
                struct Entry {
                    level: u8,
                    certificateIDs: Vec<ids::CertificateID>
                }

                while let Some(Entry { level, certificateIDs }) = seq.next_element::<Entry>()? {
                    match level {
                        0 => levels.lvl1 = certificateIDs,
                        1 => levels.lvl2 = certificateIDs,
                        2 => levels.lvl3 = certificateIDs,
                        3 => levels.lvl4 = certificateIDs,
                        4 => levels.lvl5 = certificateIDs,
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
pub struct MercenaryTacticalOperation {
    /// ID for this operation
    pub dungeonID: ids::DungeonID,
    /// Anarchy score modifier if this event is successfully completed
    pub anarchyImpact: i32,
    /// Development score modifier if this event is successfully completed
    pub developmentImpact: i32,
    /// Infomorph production modifier if this event is successfully completed
    pub infomorphBonus: i32,
    /// Operation name
    pub name: LocalizedString,
    /// Operation description
    pub description: LocalizedString
}

impl_map_collect!(ids::DungeonID, MercenaryTacticalOperation, dungeonID);

/// Metagroup or "tech tier" for [`Type`]s
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct MetaGroup {
    /// ID for this metagroup
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
pub struct MetenoxMoonDrill {   // TODO: Maybe just make singleton?, TODO: This still doesn't specify what reagent is used T.T
    /// Drill typeID
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
pub struct MilitaryCampaign {
    // ID for this Campaign
    pub campaignID: uuids::MilitaryCampaignID,
    pub title: LocalizedString,
    pub subtitle: LocalizedString,
    pub issuer: MilitaryCampaignIssuer, // TODO: Flatten
    /// ???
    pub targetProgress: u32,
    pub annotations: MilitaryCampaignAnnotations
}

impl_map_collect!(uuids::MilitaryCampaignID, MilitaryCampaign, campaignID);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct MilitaryCampaignIssuer {
    pub factionID: ids::FactionID
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct MilitaryCampaignAnnotations {
    pub achievementUUID: Option<String>,
    pub aoCampaignCardButtonImage: String,  // TODO: Apply SharedCache Resource type alias
    pub backgroundVideoLoop: String,
    pub briefingBackground: String,
    pub briefingFailureDescription: LocalizedString,
    pub briefingFailureHeader: LocalizedString,
    pub briefingFinalWords: LocalizedString,
    pub briefingForeground: String,
    pub briefingGoalDescription: LocalizedString,
    pub briefingHeader: LocalizedString,
    pub briefingMiddleground: String,
    pub briefingSuccessDescription: LocalizedString,
    pub briefingSuccessHeader: LocalizedString,
    pub campaignSet: String,
    pub dashboardAmbientBackground: String,
    pub dashboardBackground: String,
    pub dashboardForeground: String,
    pub dashboardMiddleground: String,
    pub finishedCampaignEnded: LocalizedString,
    pub finishedFailureDescription: LocalizedString,
    pub finishedResolutionStateFailure: LocalizedString,
    pub finishedResolutionStateSuccess: LocalizedString,
    pub finishedSuccessDescription: LocalizedString,
    pub foregroundVideoIntro: String,
    pub foregroundVideoLoop: String,
    pub foregroundVideoOutro: String,
    pub infoLabel: Option<LocalizedString>,
    pub infoLabelHint: Option<LocalizedString>,
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
    pub middlegroundVideoIntro: String,
    pub middlegroundVideoLoop: String,
    pub middlegroundVideoOutro: String,
    pub presentingCharacterName: LocalizedString,
    pub presentingCharacterSubtitle: LocalizedString,
    pub presentingCharacterTexturePath: String,
    pub race: MilitaryCampaignAnnotationRace,
    pub themePack: String,
    pub towCampaignCardButtonImage: String,
    pub uiIndex: u32
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
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
pub struct MilitaryCampaignObjective {
    // ID for this Campaign
    pub objectiveID: uuids::MilitaryCampaignObjectiveID,
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

impl_map_collect!(uuids::MilitaryCampaignObjectiveID, MilitaryCampaignObjective, objectiveID);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct MilitaryCampaignObjectiveAnnotations {
    pub requiredEnlistmentWithFactionID: Option<ids::FactionID>,
    pub restrictionTooltip: Option<LocalizedString>,
    pub warning1: Option<LocalizedString>,
    pub warning2: Option<LocalizedString>
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct MilitaryCampaignContributionMethod {
    pub name: String,   // TODO: Enum
    pub parameters: Vec<MilitaryCampaignContributionMethodParameter>
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct MilitaryCampaignContributionMethodParameter {
    pub key: String,
    pub matcher: MilitaryCampaignContributionMethodParameterMatcher
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct MilitaryCampaignContributionMethodParameterMatcher {
    pub values: Vec<MilitaryCampaignContributionMethodParameterMatcherValue>
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct MilitaryCampaignContributionMethodParameterMatcherValue {    // TODO: Fold these, this is rather silly.
    pub valueType: String,
    #[serde(default)]
    pub values: Vec<String>
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct MilitaryCampaignObjectiveIssuer {
    pub corporationID : ids::CorporationID
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct MilitaryCampaignObjectiveRewards {
    pub isk: MilitaryCampaignObjectiveRewardQuantity,
    pub lp: MilitaryCampaignObjectiveRewardQuantity,
    pub standing: Option<MilitaryCampaignObjectiveRewardStanding>
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct MilitaryCampaignObjectiveRewardQuantity {
    pub amountPerInterval: u32,
    pub issuer: MilitaryCampaignObjectiveIssuer,
    pub progressInterval: u32,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct MilitaryCampaignObjectiveRewardStanding {
    pub gainPercentPerInterval: f64,
    pub issuer: MilitaryCampaignObjectiveStandingIssuer,
    pub progressInterval: u32
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct MilitaryCampaignObjectiveStandingIssuer {
    pub factionID: ids::FactionID
}

/// NPC Mission
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct Mission {
    /// MissionID for this mission
    pub missionID: ids::MissionID,
    pub agentTypeID: Option<ids::AgentTypeID>,
    pub corporationID: Option<ids::CorporationID>,
    pub name: LocalizedString,
    pub expirationTime: Option<u32>,    // TODO: Unit
    pub courierMission: Option<MissionCourier>,
    #[serde(default)]
    #[serde(deserialize_with="deserialize_faction_standings")]
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
deserialize_with_proxy!(deserialize_faction_standings, factionID, ids::FactionID, standing, f64, SEQTOMAP);
deserialize_with_proxy!(deserialize_mission_message_map, kind, String, text, LocalizedString, SEQTOMAP);

impl_map_collect!(ids::MissionID, Mission, missionID);


#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct MissionReward {
    pub reward: Option<MissionRewardType>,
    pub bonusReward: Option<MissionRewardType>,
    pub bonusTimeInterval: Option<u32>
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct MissionRewardType {
    pub rewardQuantity: Option<u32>,
    pub rewardTypeID: Option<ids::TypeID>
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct MissionCourier {
    pub objectiveQuantity: u32,
    pub objectiveSingleton: bool,
    pub objectiveTypeID: ids::TypeID
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct MissionKill {
    pub dropItemInMissionContainer: Option<u32>,
    pub dungeonID: Option<ids::DungeonID>,
    pub objectiveQuantity: Option<u32>,
    pub objectiveTypeID: Option<ids::TypeID>
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct NotificationType {
    /// CharacterID for this NPC
    pub notificationTypeID: ids::NotificationTypeID,
    pub displayName: Option<LocalizedString>,
    pub internalName: String
}

impl_map_collect!(ids::NotificationTypeID, NotificationType, notificationTypeID);


/// NPC character
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct NpcCharacter {
    /// CharacterID for this NPC
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
deserialize_with_proxy!(deserialize_npc_skill, typeID, ids::TypeID, SEQTOSEQ);

/// Additional agent-specific information for an NPC character
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
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
pub struct CorporationDivision {
    /// ID for this division
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
pub struct NpcCorporation {
    /// ID for this corporation
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
    #[serde(deserialize_with="deserialize_corp_trades")]
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
    #[serde(deserialize_with="deserialize_exchange_rates")]
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
    #[serde(deserialize_with="deserialize_investors")]
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
deserialize_with_proxy!(deserialize_corp_trades, typeID, ids::TypeID, value, f64, SEQTOMAP);
deserialize_with_proxy!(deserialize_exchange_rates, corporationID, ids::CorporationID, rate, f64, SEQTOMAP);
deserialize_with_proxy!(deserialize_investors, corporationID, ids::CorporationID, shares, i32, SEQTOMAP);

/// Division of an NPC corporation
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct NpcCorporationDivision {
    /// [`CorporationDivision`] for this division
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
pub struct NpcStation {
    /// ID for this station
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
pub struct PlanetResource {
    /// PlanetID to which this resource info applies
    pub celestialID: ids::PlanetID,
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

impl_map_collect!(ids::PlanetID, PlanetResource, celestialID);

/// Planetary industry schematic
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct PlanetSchematic {
    /// ID for this schematic
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
pub struct PlanetSchematicType {
    /// TypeID
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
pub struct ProximityTrap {
    /// Trap object
    pub typeID: ids::TypeID,
    pub dbuffDuration: u32,
    #[serde(default)]
    #[serde(deserialize_with="deserialize_dbuffs_generic")]
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
pub struct CharacterRace {
    /// ID for this character race
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
    #[serde(deserialize_with="deserialize_skill_level_map")]
    pub skills: IndexMap<ids::TypeID, values::SkillLevel>
}

impl_map_collect!(ids::RaceID, CharacterRace, raceID);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct SchoolMap {
    pub mappingID: u32, // TODO: Better name
    pub schoolID: ids::SchoolID,
    pub solarSystemID: ids::SolarSystemID
}

impl_map_collect!(ids::SchoolID, ids::SolarSystemID, SchoolMap, fn |m| (m.schoolID, m.solarSystemID));

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct School {
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
pub struct ShipTreeElement {
    pub elementID: ids::ShipTreeElementID,
    pub name: LocalizedString,
    pub description: LocalizedString,
    pub icon: String
}

impl_map_collect!(ids::ShipTreeElementID, ShipTreeElement, elementID);

/// Ship Tree Faction
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct ShipTreeFaction {
    pub factionID: ids::FactionID,
    pub description: LocalizedString,
    pub icon: String,
    #[serde(deserialize_with="deserialize_shiptree_elements")]
    pub elements: IndexMap<u32, ids::ShipTreeElementID>
}
deserialize_with_proxy!(deserialize_shiptree_elements, position, u32, elementID, ids::ShipTreeElementID, SEQTOMAP);
impl_map_collect!(ids::FactionID, ShipTreeFaction, factionID);

/// Ship Tree Group (Ship class)
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct ShipTreeGroup {
    pub groupID: ids::ShipTreeGroupID,
    pub name: LocalizedString,
    pub description: Option<LocalizedString>,
    pub icon: String,
    pub iconLarge: String,
    pub iconSmall: String,
    pub iconSmallNPC: String,
    #[serde(default)]
    #[serde(deserialize_with="deserialize_shiptree_elements")]  // Reused from above
    pub elements: IndexMap<u32, ids::ShipTreeElementID>,
    #[serde(default)]
    #[serde(deserialize_with="deserialize_inline_entry_map")]
    pub preReqSkills: IndexMap<ids::FactionID, ShipTreeGroupSkills>
}

impl_map_collect!(ids::ShipTreeGroupID, ShipTreeGroup, groupID);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct ShipTreeGroupSkills {
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
pub struct ShipTreeGroupSkillInfo {
    pub skillTypeID: ids::TypeID,
    pub display: bool,
    pub level: values::SkillLevel
}

impl InlineEntry<ids::TypeID> for ShipTreeGroupSkillInfo {
    fn key(&self) -> ids::TypeID {
        self.skillTypeID
    }
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct SkillPlan {
    pub skillPlanID: ids::SkillPlanID,
    pub careerPathID: Option<ids::CareerPathID>,
    pub name: LocalizedString,
    pub description: LocalizedString,
    pub factionID: Option<ids::FactionID>,
    pub internalName: String,
    pub npcCorporationDivision: Option<ids::DivisionID>,
    #[serde(deserialize_with = "deserialize_skillplan_milestones")]
    pub milestones: IndexMap<ids::TypeID, Option<values::SkillLevel>>,
    #[serde(deserialize_with = "deserialize_skill_level_map")]
    pub skillRequirements: IndexMap<ids::TypeID, values::SkillLevel>,
}
deserialize_with_proxy!(deserialize_skillplan_milestones, typeID, ids::TypeID, level, Option<values::SkillLevel>, SEQTOMAP);
impl_map_collect!(ids::SkillPlanID, SkillPlan, skillPlanID);

// SKINR


/// SKINR Component Category (currently: Material, Pattern, Metallic [material]
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct SKINRComponentCategory {
    pub categoryID: ids::SKINRComponentCategoryID,
    pub name: String
}

impl_map_collect!(ids::SKINRComponentCategoryID, SKINRComponentCategory, categoryID);


#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct SKINRComponentPointValues {
    pub categoryID: ids::SKINRComponentCategoryID,
    #[serde(deserialize_with="deserialize_skinr_comp_points")]
    pub rarities: IndexMap<ids::SKINRComponentRarity, u32>
}

deserialize_with_proxy!(deserialize_skinr_comp_points, rarityID, ids::SKINRComponentRarity, points, u32, SEQTOMAP);
impl_map_collect!(ids::SKINRComponentCategoryID, SKINRComponentPointValues, categoryID);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct SKINRComponentRarity {
    pub rarityID: ids::SKINRComponentRarity,
    pub name: LocalizedString,
    pub rank: u32
}

impl_map_collect!(u32, SKINRComponentRarity, rarityID);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct SKINRComponent {
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
deserialize_with_proxy!(deserialize_component_associated_typeids, typeID, ids::TypeID, licenseUsesGranted, i32, SEQTOMAP);

impl_map_collect!(ids::SKINRComponentID, SKINRComponent, componentID);


#[derive(Debug, Deserialize, Eq, PartialEq)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub enum SKINRFinish {
    Matte, Satin, Gloss
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct SKINRSequenceBinder {
    pub itemTypeID: ids::TypeID,
    pub count: u32
}


#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct SKINRSlotCategory {
    pub categoryID: ids::SKINRSlotCategoryID,
    pub name: String
}

impl_map_collect!(ids::SKINRSlotCategoryID, SKINRSlotCategory, categoryID);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct SKINRSlotConfiguration {
    pub id: u32,  // TODO: Document
    pub allowAllShips: Option<bool>,
    pub config: Option<Vec<u32>>,
    pub name: String,
    pub priority: u32,
    pub ships: Option<Vec<ids::TypeID>>
}

impl_map_collect!(u32, SKINRSlotConfiguration, id);


#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct SKINRSlotName {
    pub id: u32,  // TODO: Document
    pub name: String
}

impl_map_collect!(u32, SKINRSlotName, id);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct SKINRSlot {
    pub slotID: u32,  // TODO: Document
    pub category: ids::SKINRSlotCategoryID,
    pub name: LocalizedString,
    pub allowedDesignComponentCategories: Vec<ids::SKINRComponentCategoryID>
}

impl_map_collect!(u32, SKINRSlot, slotID);


#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct SKINRSlotsToMaterials {
    pub factionID: ids::FactionID,
    pub slots: Vec<SKINRSlotMaterialBinding>
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct SKINRSlotMaterialBinding {
    pub materialID: u32,
    pub slotID: u32
}

impl_map_collect!(u32, SKINRSlotsToMaterials, factionID);


#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct SKINRTierThreshold {
    pub infoBubbleGroupID: u32,
    #[serde(deserialize_with="deserialize_SKINR_Thresholds")]
    pub tiers: IndexMap<u32, u32>
}

deserialize_with_proxy!(deserialize_SKINR_Thresholds, level, u32, maxPoints, u32, SEQTOMAP);
impl_map_collect!(u32, SKINRTierThreshold, infoBubbleGroupID);

/// Skin license item
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct SkinLicense {
    /// TypeID of the license item
    ///
    /// Equal to `licenseTypeID`
    pub licenseTypeID: ids::TypeID,
    /// Skin duration, -1 for permanent skins
    pub duration: i32,
    /// [`Skin`] this license is for
    pub skinID: ids::SkinID,
    /// Whether skin is a "single use" (default if unspecified). Unused value
    pub isSingleUse: Option<bool>
}

impl_map_collect!(ids::TypeID, SkinLicense, licenseTypeID);

/// Skin material; The design & colours of a skin, shared between multiple ships
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct SkinMaterial {
    /// ID for this material
    pub skinMaterialID: ids::SkinMaterialID,
    /// Name for this material; Name of the skin line/series.
    pub displayName: Option<LocalizedString>,
    /// Graphics info material-set, not to be confused with `materialID` which identifies this SkinMaterial
    pub materialSetID: ids::MaterialSetID,
}

impl_map_collect!(ids::SkinMaterialID, SkinMaterial, skinMaterialID);

/// Ship skin, not to be confused with a [`SkinLicense`] item
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct Skin {
    /// ID for this skin
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
pub struct SovereigntyUpgrade {
    /// TypeID for sov-upgrade item
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
pub struct StationOperation {
    /// ID for this operation
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
    #[serde(deserialize_with="deserialize_stationtypes")]
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

deserialize_with_proxy!(deserialize_stationtypes, raceID, ids::RaceID, typeID, ids::TypeID, SEQTOMAP);
impl_map_collect!(ids::StationOperationID, StationOperation, operationID);

/// [`NpcStation`] service
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct StationService { // TODO: Document icons somewhere
    /// ID for this service
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
pub struct StationStandingsRestriction {
    /// Faction this restriction applies to
    #[serde(rename="id")]
    pub factionID: ids::FactionID,
    #[serde(deserialize_with="deserialize_station_restriction_services")]
    pub services: IndexMap<ids::StationServiceID, f64>
}

deserialize_with_proxy!(deserialize_station_restriction_services, serviceID, ids::StationServiceID, standing, f64, SEQTOMAP);
impl_map_collect!(ids::FactionID, StationStandingsRestriction, factionID);


#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct SystemDynamicbuffEmitter {
    pub typeID: ids::TypeID,
    #[serde(default, deserialize_with="deserialize_dbuffs_emitter")]
    pub dbuffs: IndexMap<ids::DynamicBuffID, f64>,
    pub duration: u32,
    pub excludeProtected: bool,
    pub interval: u32
}

deserialize_with_proxy!(deserialize_dbuffs_emitter, dbuffCollectionID, ids::DynamicBuffID, value, f64, SEQTOMAP);
impl_map_collect!(ids::TypeID, SystemDynamicbuffEmitter, typeID);

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct SystemWideEffect {
    pub typeID: ids::TypeID,
    #[serde(default)]
    #[serde(deserialize_with="deserialize_dbuffs_generic")]
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
pub struct TranslationLanguage {
    /// Short name (ISO 639 code)
    pub language: String,
    /// Full name
    pub name: String
}

/// Ship & effect beacon bonuses
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct TypeBonuses {
    /// Item [`Type`] this bonus info is for
    pub typeID: ids::TypeID,
    /// Icon for projected bonuses (e.g. Incursion or wormhole effects)
    pub iconID: Option<ids::IconID>,
    /// Skill bonuses, per level
    ///
    /// Displayed first
    #[serde(default)]
    #[serde(rename = "types")]
    #[serde(deserialize_with="deserialize_skill_bonuses")]
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
deserialize_with_proxy!(deserialize_skill_bonuses, typeID, ids::TypeID, bonuses, Vec<TypeBonus>, SEQTOMAP);

/// Single bonus
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
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
pub struct TypeDogma {
    /// Applicable type
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
deserialize_with_proxy!(deserialize_type_attributes, attributeID, ids::AttributeID, value, f64, SEQTOMAP);
deserialize_with_proxy!(deserialize_type_effects, effectID, ids::EffectID, isDefault, bool, SEQTOMAP);
impl_map_collect!(ids::TypeID, TypeDogma, typeID);


#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct TypeElement {
    pub typeID: ids::TypeID,
    #[serde(deserialize_with="deserialize_type_elements")]
    pub elements: IndexMap<u32, ids::ShipTreeElementID>,   // TODO: Document key/value
}

deserialize_with_proxy!(deserialize_type_elements, position, u32, elementID, ids::ShipTreeElementID, SEQTOMAP);
impl_map_collect!(ids::TypeID, TypeElement, typeID);

/// TypeList; List of types
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct TypeList {
    /// ID for this type list
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
pub struct TypeMaterials {
    /// Input/reprocessed typeID
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
#[cfg_attr(feature="sde_strict", serde(deny_unknown_fields))]
pub struct TypeShipTreeElements {
    /// TypeID for which elements are provided
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
pub struct Type {
    /// ID for this type
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