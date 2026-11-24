#![allow(non_snake_case)]

#[cfg(feature = "npc_data")]
pub mod npc;


use crate::sde::types::*;
use crate::types::{ids, uuids};
use crate::util;
use indexmap::IndexMap;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::fs::File;
use std::{fs, io};
use std::io::{BufRead, BufReader, ErrorKind, Seek, SeekFrom};
use std::path::PathBuf;
use std::str::FromStr;
use util::units::EVEUnit;

pub mod types;

/// Error indicating failure to load SDE
#[derive(Debug)]
pub enum SDEError {
    /// IO Error while reading from SDE
    IO(io::Error),
    /// Requested SDE tenant is not available
    TenantNotFound(String),
    /// SDE did not contain expected file, did the SDE format change?
    FileNotFound(String),
    /// Parsing the JSON content failed, did the SDE schema change?
    ParseError { file: String, entry: usize, error: serde_json::Error},
    /// Data integrity problem, did the SDE schema change?
    IntegrityError(String)
}

impl Display for SDEError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SDEError::IO(err) => write!(f, "IO error: {}", err),
            SDEError::TenantNotFound(tenant) => write!(f, "Tenant `{}` is not available", tenant),
            SDEError::FileNotFound(filename) => write!(f, "SDE did not contain expected file: `{}`", filename),
            SDEError::ParseError { file, entry, error } => write!(f, "Parse error in `{}` entry/line {}: {}", file, entry + 1, error),
            SDEError::IntegrityError(err_description) => write!(f, "SDE data integrity error ({})", err_description)
        }
    }
}

impl Error for SDEError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            SDEError::IO(err) => Some(err),
            SDEError::TenantNotFound(_) => None,
            SDEError::FileNotFound(_) => None,
            SDEError::ParseError { error, .. } => Some(error),
            SDEError::IntegrityError(_) => None
        }
    }
}

impl From<io::Error> for SDEError {
    fn from(value: io::Error) -> Self {
        SDEError::IO(value)
    }
}

#[allow(non_snake_case)] // Deserialized struct
#[derive(Debug, Deserialize)]
pub struct SDEInfo {
    pub release: String,
    pub buildNumber: u64,
    pub releaseDate: String,
    pub tables: IndexMap<String, SDETable>
}

#[allow(non_snake_case)] // Deserialized struct
#[derive(Debug, Deserialize)]
pub struct SDETable {
    pub version: String,
    pub jsonl: SDEFile,
    pub record: SDERecord,
}
#[derive(Debug, Deserialize)]
pub struct SDEFile { pub url: String }
#[derive(Debug, Deserialize)]
pub struct SDERecord { pub urlTemplate: String }

pub struct SDE {
    client: reqwest::blocking::Client,
    info: SDEInfo,
    cache: Option<PathBuf>
}

impl SDE {
    pub fn initialize(user_agent: &str) -> Result<Self, SDEError> {
        if user_agent.len() == 0 { return Err(SDEError::IO(io::Error::other("A useragent identifying the application must be set in SDE::initialize()"))) };

        let client = reqwest::blocking::Client::builder()
            .user_agent(format!("{} (turtletools-staticdata/{})", user_agent, crate::CRATE_VERSION))
            .build().map_err(io::Error::other)?;

        let info = client.get("https://sde.evetech.net/tranquility/latest/").send().map_err(io::Error::other)?
            .json::<SDEInfo>().map_err(io::Error::other)?;

        Ok(SDE { client, info, cache: None })
    }

    pub fn initialize_for_tenant(user_agent: &str, tenant: &str, preferred_build: Option<&str>) -> Result<Self, SDEError> {
        if user_agent.len() == 0 { return Err(SDEError::IO(io::Error::other("A useragent identifying the application must be set in SDE::initialize()"))) };

        #[derive(Debug, Deserialize)]
        struct Root { tenants: Vec<TenantInfo> }
        #[derive(Debug, Deserialize)]
        struct TenantInfo {
            name: String,
            tenantUrl: String
        }
        #[derive(Debug, Deserialize)]
        struct SingleRelease {
            #[allow(unused)]
            release: String,
            releaseUrl: String
        }
        #[derive(Debug, Deserialize)]
        struct Releases {
            latestUrl: String,
            releases: Vec<SingleRelease>
        }

        let client = reqwest::blocking::Client::builder()
            .user_agent(format!("{} (turtletools-staticdata {} +{})", user_agent, crate::CRATE_VERSION, crate::CRATE_REPO))
            .build().map_err(io::Error::other)?;

        let root = client.get("https://sde.evetech.net/").send().map_err(io::Error::other)?
            .json::<Root>().map_err(io::Error::other)?;

        for tenant_info in root.tenants {
            if tenant_info.name.eq_ignore_ascii_case(tenant) {
                let releases = client.get(tenant_info.tenantUrl).send().map_err(io::Error::other)?
                    .json::<Releases>().map_err(io::Error::other)?;

                let mut info = None;
                if let Some(preferred_build) = preferred_build.and_then(|b| u64::from_str(b).ok()) {
                    for release in releases.releases {
                        let release_info = client.get(release.releaseUrl).send().map_err(io::Error::other)?
                            .json::<SDEInfo>().map_err(io::Error::other)?;
                        if release_info.buildNumber == preferred_build {
                            info = Some(release_info);
                            break
                        }
                    }
                }

                let info = if let Some(info) = info {
                    info
                } else {
                    client.get(releases.latestUrl).send().map_err(io::Error::other)?.json::<SDEInfo>().map_err(io::Error::other)?
                };
                return Ok(SDE {
                    client,
                    cache: None,
                    info
                })
            }
        }
        Err(SDEError::TenantNotFound(tenant.to_owned()))
    }

    pub fn with_cache<P: Into<PathBuf>>(self, cache: P) -> SDE {
        SDE { client: self.client, info: self.info, cache: Some(cache.into()) }
    }

    pub fn info(&self) -> &SDEInfo {
        &self.info
    }

    pub fn stream(&mut self) -> Stream<'_> { Stream(self) }
    pub fn collected(&mut self) -> Collected<'_> { Collected(self) }

    fn load_file<T: DeserializeOwned>(&mut self, name: &str) -> Result<Box<dyn Iterator<Item=Result<T, SDEError>>>, SDEError> {
        let table = self.info.tables.get(name).ok_or_else(|| SDEError::FileNotFound(name.to_owned()))?;
        let name = name.to_owned();

        if let Some(cachedir) = &self.cache {
            let mut cachefile = cachedir.join(&table.version); cachefile.add_extension("jsonl");

            match File::open(&cachefile) {
                Ok(file) => {
                    Ok(Box::new(
                        BufReader::new(file)
                            .lines()
                            .enumerate()
                            .map(move |(entry, res)| {
                                res.map_err(SDEError::from)
                                    .and_then(|line| serde_json::from_str(&line).map_err(|error| SDEError::ParseError { file: name.clone(), entry, error, }))
                            })
                    ))
                }
                Err(err) => {
                    if err.kind() == ErrorKind::NotFound {
                        fs::create_dir_all(cachedir)?;
                        let mut out_file = File::create_new(cachefile)?;
                        io::copy(&mut self.client.get(&table.jsonl.url).send().map_err(io::Error::other)?, &mut out_file)?;
                        out_file.seek(SeekFrom::Start(0))?;

                        Ok(Box::new(
                            BufReader::new(out_file)
                                .lines()
                                .enumerate()
                                .map(move |(entry, res)| {
                                    res.map_err(SDEError::from)
                                        .and_then(|line| serde_json::from_str(&line).map_err(|error| SDEError::ParseError { file: name.clone(), entry, error, }))
                                })
                        ))
                    } else {
                        Err(err)?
                    }
                }
            }
        } else {
            Ok(Box::new(
                BufReader::new(self.client.get(&table.jsonl.url).send().map_err(io::Error::other)?)
                    .lines()
                    .enumerate()
                    .map(move |(entry, res)| {
                        res.map_err(SDEError::from)
                            .and_then(|line| serde_json::from_str(&line).map_err(|error| SDEError::ParseError { file: name.clone(), entry, error, }))
                    })
            ))
        }
    }
}

pub struct Stream<'a>(&'a mut SDE);
pub struct Collected<'a>(&'a mut SDE);

macro_rules! impl_loader {
    ($($name:ident, $keytype:ty, $valtype:ty);+) => {
        impl<'a> Stream<'a>  {
            $(pub fn $name(&mut self) -> Result<impl Iterator<Item=Result<$valtype, SDEError>>, SDEError> { self.0.load_file::<$valtype>(stringify!($name)) })+
        }

        impl<'a> Collected<'a> {
            $(pub fn $name(&mut self) -> Result<IndexMap<$keytype, $valtype>, SDEError> { self.0.stream().$name()?.collect() })+
        }

        #[cfg(test)]
        fn load_macro_parts(sde: &mut SDE) -> Result<(), SDEError> {
            $(drop(sde.collected().$name()?);)+
            Ok(())
        }
    };
}

impl_loader!(
    accountingEntryTypes, ids::AccountingEntryTypeID, AccountingEntryType;
    agentsInSpace, ids::CharacterID, AgentInSpace;
    ancestries, ids::AncestryID, Ancestry;
    appliedProximityEffects, ids::TypeID, AppliedProximityEffect;
    archetypes, ids::DungeonArchetypeID, Archetype;
    bloodlines, ids::BloodlineID, Bloodline;
    blueprints, ids::TypeID, Blueprint;
    categories, ids::CategoryID, Category;
    certificates, ids::CertificateID, Certificate;
    characterAttributes, ids::CharacterAttributeID, CharacterAttribute;
    characterTitles, uuids::CharacterTitleID, CharacterTitle;
    cloneGrades, ids::CloneGradeID, CloneGrade;
    contrabandTypes, ids::TypeID, ContrabandType;
    controlTowerResources, ids::TypeID, ControlTowerResources;
    corporationActivities, ids::CorporationActivityID, CorporationActivity;
    corporationRoleGroups, ids::CorporationRoleGroupID, CorporationRoleGroup;
    corporationRoles, ids::CorporationRoleID, CorporationRole;
    dbuffCollections, ids::DynamicBuffID, DynamicBuff;
    dogmaAttributeCategories, ids::AttributeCategoryID, AttributeCategory;
    dogmaAttributes, ids::AttributeID, Attribute;
    dogmaEffects, ids::EffectID, Effect;
    dogmaUnits, EVEUnit, DogmaUnit;
    dungeons, ids::DungeonID, Dungeon;
    dynamicItemAttributes, ids::TypeID, DynamicItemAttributes;
    epicArcs, ids::EpicArcID, EpicArc;
    expertSystems, ids::TypeID, ExpertSystem;
    factions, ids::FactionID, Faction;
    fighterAbilities, ids::FighterAbilityID, FighterAbility;
    fighterAbilitiesByType, ids::TypeID, FighterAbilitiesByType;
    graphicMaterialSets, ids::MaterialSetID, GraphicMaterialSet;
    graphics, ids::GraphicID, Graphic;
    groups, ids::GroupID, Group;
    icons, ids::IconID, Icon;
    industryActivities, ids::IndustryActivityID, IndustryActivity;
    industryAssemblyLines, ids::IndustryAssemblyLineID, IndustryAssemblyLine;
    industryInstallationTypes, ids::TypeID, IndustryInstallationType;
    industryModifierSources, ids::TypeID, IndustryModifierSource;
    industryTargetFilters, ids::IndustryFilterID, IndustryTargetFilter;
    landmarks, ids::LandmarkID, Landmark;
    linkWithShip, ids::TypeID, LinkWithShip;
    mapAsteroidBelts, ids::AsteroidBeltID, AsteroidBelt;
    mapConstellations, ids::ConstellationID, Constellation;
    mapMoons, ids::MoonID, Moon;
    mapPlanets, ids::PlanetID, Planet;
    mapRegions, ids::RegionID, Region;
    mapSecondarySuns, ids::SolarSystemID, SecondarySun;
    mapSolarSystems, ids::SolarSystemID, SolarSystem;
    mapStargates, ids::StargateID, Stargate;
    mapStars, ids::StarID, Star;
    marketGroups, ids::MarketGroupID, MarketGroup;
    mercenaryTacticalOperations, ids::DungeonID, MercenaryTacticalOperation;
    metenoxMoonDrill, ids::TypeID, MetenoxMoonDrill;
    metaGroups, ids::MetaGroupID, MetaGroup;
    militaryCampaigns, uuids::MilitaryCampaignID, MilitaryCampaign;
    militaryCampaignObjectives, uuids::MilitaryCampaignObjectiveID, MilitaryCampaignObjective;
    missions, ids::MissionID, Mission;
    notificationTypes, ids::NotificationTypeID, NotificationType;
    npcCharacters, ids::CharacterID, NpcCharacter;
    npcCorporationDivisions, ids::DivisionID, CorporationDivision;
    npcCorporations, ids::CorporationID, NpcCorporation;
    npcStations, ids::StationID, NpcStation;
    planetResources, ids::PlanetID, PlanetResource;
    planetSchematics, ids::PlanetSchematicID, PlanetSchematic;
    proximityTrap, ids::TypeID, ProximityTrap;
    races, ids::RaceID, CharacterRace;
    schools, ids::SchoolID, School;
    shipTreeElements, ids::ShipTreeElementID, ShipTreeElement;
    shipTreeFactions, ids::FactionID, ShipTreeFaction;
    shipTreeGroups, ids::ShipTreeGroupID, ShipTreeGroup;
    skillPlans, u32, SkillPlan;
    skinLicenses, ids::TypeID, SkinLicense;
    skinMaterials, ids::SkinMaterialID, SkinMaterial;
    skinrComponentCategories, ids::SKINRComponentCategoryID, SKINRComponentCategory;
    skinrComponentPointValues, ids::SKINRComponentCategoryID, SKINRComponentPointValues;
    skinrComponentRarities, u32, SKINRComponentRarity;
    skinrComponents, ids::SKINRComponentID, SKINRComponent;
    skinrSlotCategories, ids::SKINRSlotCategoryID, SKINRSlotCategory;
    skinrSlotConfigurations, u32, SKINRSlotConfiguration;
    skinrSlotNames, u32, SKINRSlotName;
    skinrSlots, u32, SKINRSlot;
    skinrSlotsToMaterials, u32, SKINRSlotsToMaterials;
    skinrTierThresholds, u32, SKINRTierThreshold;
    skins, ids::SkinID, Skin;
    sovereigntyUpgrades, ids::TypeID, SovereigntyUpgrade;
    stationOperations, ids::StationOperationID, StationOperation;
    stationServices, ids::StationServiceID, StationService;
    stationStandingsRestrictions, ids::FactionID, StationStandingsRestriction;
    systemDbuffEmitters, ids::TypeID, SystemDynamicbuffEmitter;
    systemWideEffects, ids::TypeID, SystemWideEffect;
    typeBonus, ids::TypeID, TypeBonuses;
    typeDogma, ids::TypeID, TypeDogma;
    typeElements, ids::TypeID, TypeElement;
    typeLists, ids::TypeListID, TypeList;
    typeMaterials, ids::TypeID, TypeMaterials;
    types, ids::TypeID, Type
);

// Special cases
impl<'a> Stream<'a> {
    pub fn agentTypes(&mut self) -> Result<impl Iterator<Item=Result<(ids::AgentTypeID, AgentType), SDEError>>, SDEError> {
        self.0.load_file::<AgentTypeEntry>("agentTypes")
            .map(|iter| iter.map(|res| res.map(|entry| (entry.agentTypeID, entry.name))))
    }
    pub fn compressibleTypes(&mut self) -> Result<impl Iterator<Item=Result<CompressibleType, SDEError>>, SDEError> { self.0.load_file::<CompressibleType>("compressibleTypes") }
    pub fn freelanceJobSchemas(&mut self) -> Result<impl Iterator<Item=Result<FreelanceJobSchemas, SDEError>>, SDEError> { self.0.load_file::<FreelanceJobSchemas>("freelanceJobSchemas") }
    pub fn masteries(&mut self) -> Result<impl Iterator<Item=Result<TypeMasteries, SDEError>>, SDEError> { self.0.load_file::<TypeMasteries>("masteries") }
    pub fn translationLanguages(&mut self) -> Result<impl Iterator<Item=Result<TranslationLanguage, SDEError>>, SDEError> { self.0.load_file::<TranslationLanguage>("translationLanguages") }
}
impl<'a> Collected<'a> {
    pub fn agentTypes(&mut self) -> Result<IndexMap<ids::AgentTypeID, AgentType>, SDEError> { self.0.stream().agentTypes()?.collect() }
    pub fn compressibleTypes(&mut self) -> Result<IndexMap<ids::TypeID, ids::TypeID>, SDEError> { self.0.stream().compressibleTypes()?.collect() }
    pub fn freelanceJobSchemas(&mut self) -> Result<IndexMap<u32, Vec<FreelanceJobSchema>>, SDEError> { self.0.stream().freelanceJobSchemas()?.collect() }
    pub fn masteries(&mut self) -> Result<IndexMap<ids::TypeID, MasteryInfo>, SDEError> { self.0.stream().masteries()?.collect() }
    pub fn translationLanguages(&mut self) -> Result<Vec<TranslationLanguage>, SDEError> { self.0.stream().translationLanguages()?.collect() }
}

#[cfg(test)]
fn load_special_parts(sde: &mut SDE) -> Result<(), SDEError> {
    drop(sde.collected().agentTypes()?);
    drop(sde.collected().compressibleTypes()?);
    drop(sde.collected().freelanceJobSchemas()?);
    drop(sde.collected().masteries()?);
    drop(sde.collected().translationLanguages()?);
    Ok(())
}

#[cfg(test)]
pub(crate) fn load_full_sde(sde: &mut SDE) -> Result<(), SDEError> {
    load_special_parts(sde)?;
    load_macro_parts(sde)?;
    Ok(())
}