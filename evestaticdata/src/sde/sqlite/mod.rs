//! Functions for converting the Static Data Export to an SQLite format
//! Broadly follows community standard database schema
//! Exact schema subject to change, use an established conversion if you require consistent schema

use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::io::{Read, Seek};
use rusqlite::types::Null;
use crate::sde::load::{LocalizedString, ResourcePurpose, SDEError, SDELoader, Type, TypeMaterial};
use crate::types::ids;
use crate::util::units::EVEUnit;

#[derive(Debug)]
pub enum ExportError {
    SDELoad(SDEError),
    SQLite(rusqlite::Error)
}

impl Display for ExportError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportError::SDELoad(err) => Display::fmt(err, f),
            ExportError::SQLite(err) => Display::fmt(err, f),
        }
    }
}
impl Error for ExportError {}

impl From<SDEError> for ExportError {
    fn from(value: SDEError) -> Self {
        ExportError::SDELoad(value)
    }
}

impl From<rusqlite::Error> for ExportError {
    fn from(value: rusqlite::Error) -> Self {
        ExportError::SQLite(value)
    }
}

pub fn export_agent_types<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "agtAgentTypes" (
            "agentTypeID" INTEGER NOT NULL,
            "agentType" VARCHAR(50),
            PRIMARY KEY ("agentTypeID")
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "agtAgentTypes""#, ())?;
    }

    let tx = db.transaction()?;
    let mut statement = tx.prepare("INSERT INTO agtAgentTypes (agentTypeID, agentType) VALUES (?, ?)")?;

    let mut modified = 0;
    for res in sde.load_agent_types()? {
        let (agent_type_id, agent_type) = res?;
        modified += statement.execute((agent_type_id, agent_type.to_string()))?;
    }
    statement.finalize()?;
    tx.commit()?;

    Ok(modified)
}

pub fn export_agents<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "agtAgents" (
            "agentID" INTEGER NOT NULL,
            "divisionID" INTEGER,
            "corporationID" INTEGER,
            "locationID" INTEGER,
            level INTEGER,
            "agentTypeID" INTEGER,
            "isLocator" BOOLEAN,
            PRIMARY KEY ("agentID"),
            CONSTRAINT aa_isloc CHECK ("isLocator" IN (0, 1))
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "agtAgents""#, ())?;
    }

    let tx = db.transaction()?;
    let mut statement = tx.prepare("INSERT INTO agtAgents (agentID, divisionID, corporationID, locationID, level, agentTypeID, isLocator) VALUES (?, ?, ?, ?, ?, ?, ?)")?;

    let mut modified = 0;
    for res in sde.load_npc_characters()? {
        let character = res?;

        if let Some(agent_info) = character.agent {
            modified += statement.execute((
                character.characterID,
                agent_info.divisionID,
                character.corporationID,
                character.locationID,
                agent_info.level,
                agent_info.agentTypeID,
                agent_info.isLocator
            ))?;
        }

    }
    statement.finalize()?;
    tx.commit()?;

    Ok(modified)
}

pub fn export_agents_in_space<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "agtAgentsInSpace" (
            "agentID" INTEGER NOT NULL,
            "dungeonID" INTEGER,
            "solarSystemID" INTEGER,
            "spawnPointID" INTEGER,
            "typeID" INTEGER,
            PRIMARY KEY ("agentID")
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "agtAgentsInSpace""#, ())?;
    }

    let tx = db.transaction()?;
    let mut statement = tx.prepare("INSERT INTO agtAgentsInSpace (agentID, dungeonID, solarSystemID, spawnPointID, typeID) VALUES (?, ?, ?, ?, ?)")?;

    let mut modified = 0;
    for res in sde.load_agents_in_space()? {
        let agent = res?;
        modified += statement.execute((agent.agentID, agent.dungeonID, agent.solarSystemID, agent.spawnPointID, agent.typeID))?;
    }
    statement.finalize()?;
    tx.commit()?;

    Ok(modified)
}

// no agtResearchAgents

pub fn export_certs<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "certCerts" (
            "certID" INTEGER NOT NULL,
            description TEXT,
            "groupID" INTEGER,
            name VARCHAR(255),
            PRIMARY KEY ("certID")
        )"#, ())?;

        db.execute(r#"CREATE TABLE IF NOT EXISTS "certSkills" (
            "certID" INTEGER,
            "skillID" INTEGER,
            "certLevelInt" INTEGER,
            "skillLevel" INTEGER,
            "certLevelText" VARCHAR(8)
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "certCerts""#, ())?;
        db.execute(r#"DELETE FROM "certSkills""#, ())?;
    }

    let tx = db.transaction()?;
    let mut st_certs = tx.prepare("INSERT INTO certCerts (certID, description, groupID, name) VALUES (?, ?, ?, ?)")?;
    let mut st_skills = tx.prepare("INSERT INTO certSkills (certID, skillID, certLevelInt, skillLevel, certLevelText) VALUES (?, ?, ?, ?, ?)")?;

    let mut modified = 0;
    for res in sde.load_certificates()? {
        let cert = res?;
        modified += st_certs.execute((cert.certificateID, cert.description.en, cert.groupID, cert.name.en))?;

        for (skill_type_id, cert_levels) in cert.skillLevels {
            modified += st_skills.execute((cert.certificateID, skill_type_id, 0, cert_levels.basic, "basic"))?;
            modified += st_skills.execute((cert.certificateID, skill_type_id, 1, cert_levels.standard, "standard"))?;
            modified += st_skills.execute((cert.certificateID, skill_type_id, 2, cert_levels.improved, "improved"))?;
            modified += st_skills.execute((cert.certificateID, skill_type_id, 3, cert_levels.advanced, "advanced"))?;
            modified += st_skills.execute((cert.certificateID, skill_type_id, 4, cert_levels.elite, "elite"))?;
        }
    }
    st_certs.finalize()?;
    st_skills.finalize()?;
    tx.commit()?;

    Ok(modified)
}

pub fn export_masteries<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "certMasteries" (
            "typeID" INTEGER,
            "masteryLevel" INTEGER,
            "certID" INTEGER
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "certMasteries""#, ())?;
    }

    let tx = db.transaction()?;
    let mut statement = tx.prepare("INSERT INTO certMasteries (typeID, masteryLevel, certID) VALUES (?, ?, ?)")?;

    let mut modified = 0;
    for res in sde.load_masteries()? {
        let (type_id, mastery_levels) = res?;

        for cert_id in mastery_levels.lvl1 {
            modified += statement.execute((type_id, 0, cert_id))?;
        }
        for cert_id in mastery_levels.lvl2 {
            modified += statement.execute((type_id, 1, cert_id))?;
        }
        for cert_id in mastery_levels.lvl3 {
            modified += statement.execute((type_id, 2, cert_id))?;
        }
        for cert_id in mastery_levels.lvl4 {
            modified += statement.execute((type_id, 3, cert_id))?;
        }
        for cert_id in mastery_levels.lvl5 {
            modified += statement.execute((type_id, 4, cert_id))?;
        }
    }
    statement.finalize()?;
    tx.commit()?;

    Ok(modified)
}

pub fn export_ancestries<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "chrAncestries" (
            "ancestryID" INTEGER NOT NULL,
            "ancestryName" VARCHAR(100),
            "bloodlineID" INTEGER,
            description VARCHAR(1000),
            perception INTEGER,
            willpower INTEGER,
            charisma INTEGER,
            memory INTEGER,
            intelligence INTEGER,
            "iconID" INTEGER,
            "shortDescription" VARCHAR(500),
            PRIMARY KEY ("ancestryID")
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "chrAncestries""#, ())?;
    }

    let tx = db.transaction()?;
    let mut statement = tx.prepare(r#"INSERT INTO chrAncestries (
            ancestryID,
            ancestryName,
            bloodlineID,
            description,
            perception,
            willpower,
            charisma,
            memory,
            intelligence,
            iconID,
            shortDescription
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#
    )?;

    let mut modified = 0;
    for res in sde.load_ancestries()? {
        let ancestry = res?;
        modified += statement.execute((
            ancestry.ancestryID,
            ancestry.name.en,
            ancestry.bloodlineID,
            ancestry.description.en,
            ancestry.perception,
            ancestry.willpower,
            ancestry.charisma,
            ancestry.memory,
            ancestry.intelligence,
            ancestry.iconID,
            ancestry.shortDescription
        ))?;
    }
    statement.finalize()?;
    tx.commit()?;

    Ok(modified)
}

pub fn export_character_attributes<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "chrAttributes" (
            "attributeID" INTEGER NOT NULL,
            "attributeName" VARCHAR(100),
            description VARCHAR(1000),
            "iconID" INTEGER,
            "shortDescription" VARCHAR(500),
            notes VARCHAR(500),
            PRIMARY KEY ("attributeID")
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "chrAttributes""#, ())?;
    }

    let tx = db.transaction()?;
    let mut statement = tx.prepare(r#"INSERT INTO chrAttributes (
            attributeID,
            attributeName,
            description,
            iconID,
            shortDescription,
            notes
        ) VALUES (?, ?, ?, ?, ?, ?)"#
    )?;

    let mut modified = 0;
    for res in sde.load_character_attributes()? {
        let attribute = res?;
        modified += statement.execute((
            attribute.characterAttributeID,
            attribute.name.en,
            attribute.description,
            attribute.iconID,
            attribute.shortDescription,
            attribute.notes
        ))?;
    }
    statement.finalize()?;
    tx.commit()?;

    Ok(modified)
}

pub fn export_bloodlines<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "chrBloodlines" (
            "bloodlineID" INTEGER NOT NULL,
            "bloodlineName" VARCHAR(100),
            "raceID" INTEGER,
            description VARCHAR(1000),
            corporationID INTEGER,
            perception INTEGER,
            willpower INTEGER,
            charisma INTEGER,
            memory INTEGER,
            intelligence INTEGER,
            "iconID" INTEGER,
            PRIMARY KEY ("bloodlineID")
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "chrBloodlines""#, ())?;
    }

    let tx = db.transaction()?;
    let mut statement = tx.prepare(r#"INSERT INTO chrBloodlines (
            bloodlineID,
            bloodlineName,
            raceID,
            description,
            corporationID,
            perception,
            willpower,
            charisma,
            memory,
            intelligence,
            iconID
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#
    )?;

    let mut modified = 0;
    for res in sde.load_bloodlines()? {
        let bloodline = res?;
        modified += statement.execute((
            bloodline.bloodlineID,
            bloodline.name.en,
            bloodline.raceID,
            bloodline.description.en,
            bloodline.corporationID,
            bloodline.perception,
            bloodline.willpower,
            bloodline.charisma,
            bloodline.memory,
            bloodline.intelligence,
            bloodline.iconID
        ))?;
    }
    statement.finalize()?;
    tx.commit()?;

    Ok(modified)
}

pub fn export_factions<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "chrFactions" (
            "factionID" INTEGER NOT NULL,
            "factionName" VARCHAR(100),
            description VARCHAR(2000),
            "raceIDs" INTEGER,
            "solarSystemID" INTEGER,
            "corporationID" INTEGER,
            "sizeFactor" FLOAT,
            "militiaCorporationID" INTEGER,
            "iconID" INTEGER,
            PRIMARY KEY ("factionID")
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "chrFactions""#, ())?;
    }

    let tx = db.transaction()?;
    let mut statement = tx.prepare(r#"INSERT INTO chrFactions (
            factionID,
            factionName,
            description,
            raceIDs,
            solarSystemID,
            corporationID,
            sizeFactor,
            militiaCorporationID,
            iconID
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#
    )?;

    let mut modified = 0;
    for res in sde.load_factions()? {
        let faction = res?;
        modified += statement.execute((
            faction.factionID,
            faction.name.en,
            faction.description.en,
            faction.memberRaces.first().map(|id| *id),  // TODO: This matches the schema but is weird, figure out a good substitute behaviour
            faction.solarSystemID,
            faction.corporationID,
            faction.sizeFactor,
            faction.militiaCorporationID,
            faction.iconID
        ))?;
    }
    statement.finalize()?;
    tx.commit()?;

    Ok(modified)
}

pub fn export_races<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "chrRaces" (
            "raceID" INTEGER NOT NULL,
            "raceName" VARCHAR(100),
            description VARCHAR(1000),
            "iconID" INTEGER,
            PRIMARY KEY ("raceID")
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "chrRaces""#, ())?;
    }

    let tx = db.transaction()?;
    let mut statement = tx.prepare(r#"INSERT INTO chrRaces (raceID, raceName, description, iconID) VALUES (?, ?, ?, ?)"#)?;

    let mut modified = 0;
    for res in sde.load_races()? {
        let race = res?;
        modified += statement.execute((
            race.raceID,
            race.name.en,
            race.description.map(|s| s.en),
            race.iconID
        ))?;
    }
    statement.finalize()?;
    tx.commit()?;

    Ok(modified)
}

pub fn export_corporation_activities<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "crpActivities" (
            "activityID" INTEGER NOT NULL,
            "activityName" VARCHAR(100),
            PRIMARY KEY ("activityID")
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "crpActivities""#, ())?;
    }

    let tx = db.transaction()?;
    let mut statement = tx.prepare(r#"INSERT INTO crpActivities (activityID, activityName) VALUES (?, ?)"#)?;

    let mut modified = 0;
    for res in sde.load_corporation_activities()? {
        let activity = res?;
        modified += statement.execute((
            activity.corporationActivityID,
            activity.name.en
        ))?;
    }
    statement.finalize()?;
    tx.commit()?;

    Ok(modified)
}

/// Merged crpActivities, crpPCCorporationDivisions, crpPCCorporationResearchFields, crpPCCorporationTrades, crpNPCDivisions
pub fn export_npc_corporations<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "crpNPCCorporationDivisions" (
            "corporationID" INTEGER NOT NULL,
            "divisionID" INTEGER NOT NULL,
            size INTEGER,
            PRIMARY KEY ("corporationID", "divisionID")
        )"#, ())?;
        db.execute(r#"CREATE TABLE IF NOT EXISTS "crpNPCCorporationTrades" (
            "corporationID" INTEGER NOT NULL,
            "typeID" INTEGER NOT NULL,
            PRIMARY KEY ("corporationID", "typeID")
        )"#, ())?;
        db.execute(r#"CREATE TABLE IF NOT EXISTS "crpNPCCorporations" (
            "corporationID" INTEGER NOT NULL,
            size CHAR(1),
            extent CHAR(1),
            "solarSystemID" INTEGER,
            "friendID" INTEGER,
            "enemyID" INTEGER,
            "publicShares" INTEGER,
            "initialPrice" INTEGER,
            "minSecurity" FLOAT,
            "factionID" INTEGER,
            description VARCHAR(4000),
            "iconID" INTEGER,
            PRIMARY KEY ("corporationID")
        )"#, ())?;
        db.execute(r#"CREATE TABLE IF NOT EXISTS "crpNPCDivisions" (
            "divisionID" INTEGER NOT NULL,
            "divisionName" VARCHAR(100),
            description VARCHAR(1000),
            "leaderType" VARCHAR(100),
            PRIMARY KEY ("divisionID")
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "crpNPCCorporationDivisions""#, ())?;
        db.execute(r#"DELETE FROM "crpNPCCorporationTrades""#, ())?;
        db.execute(r#"DELETE FROM "crpNPCCorporations""#, ())?;
        db.execute(r#"DELETE FROM "crpNPCDivisions""#, ())?;
    }

    let tx = db.transaction()?;
    let mut st_corp_divisions = tx.prepare(r#"INSERT INTO crpNPCCorporationDivisions (
        corporationID,
        divisionID,
        size
    ) VALUES (?, ?, ?)"#)?;
    let mut st_corp_trades = tx.prepare(r#"INSERT INTO crpNPCCorporationTrades (
        corporationID,
        typeID
    ) VALUES (?, ?)"#)?;
    let mut st_npc_divisions = tx.prepare(r#"INSERT INTO crpNPCDivisions (
        divisionID,
        divisionName,
        description,
        leaderType
    ) VALUES (?, ?, ?, ?)"#)?;
    let mut st_npc_corps = tx.prepare(r#"INSERT INTO crpNPCCorporations (
        corporationID,
        size,
        extent,
        solarSystemID,
        friendID,
        enemyID,
        publicShares,
        initialPrice,
        minSecurity,
        factionID,
        description,
        iconID
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#)?;

    let mut modified = 0;
    for res in sde.load_npc_corporations()? {
        let corporation = res?;
        modified += st_npc_corps.execute((
            corporation.corporationID,
            corporation.size,
            corporation.extent,
            corporation.solarSystemID,
            corporation.friendID,
            corporation.enemyID,
            corporation.shares as i64,
            corporation.initialPrice,
            corporation.minSecurity,
            corporation.factionID,
            corporation.description.map(|s| s.en),  // TODO: Add name column
            corporation.iconID
        ))?;

        for (division_id, division) in corporation.divisions {
            modified += st_corp_divisions.execute((
                corporation.corporationID,
                division_id,
                division.size
            ))?;
        }

        for (type_id, _UNKNOWN) in corporation.corporationTrades {
            modified += st_corp_trades.execute((
                corporation.corporationID,
                type_id
            ))?;
        }
    }

    for res in sde.load_npc_corporation_divisions()? {
        let division = res?;
        modified += st_npc_divisions.execute((
            division.divisionID,
            division.name.en,
            division.description.map(|s| s.en),
            division.leaderTypeName.en
        ))?;
    }

    st_npc_corps.finalize()?;
    st_corp_divisions.finalize()?;
    st_corp_trades.finalize()?;
    st_npc_divisions.finalize()?;
    tx.commit()?;

    Ok(modified)
}

pub fn export_attribute_categories<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "dgmAttributeCategories" (
            "categoryID" INTEGER NOT NULL,
            "categoryName" VARCHAR(50),
            "categoryDescription" VARCHAR(200),
            PRIMARY KEY ("categoryID")
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "dgmAttributeCategories""#, ())?;
    }

    let tx = db.transaction()?;
    let mut statement = tx.prepare(r#"INSERT INTO dgmAttributeCategories (categoryID, categoryName, categoryDescription) VALUES (?, ?, ?)"#)?;

    let mut modified = 0;
    for res in sde.load_dogma_attribute_categories()? {
        let attribute_category = res?;
        modified += statement.execute((
            attribute_category.attributeCategoryID,
            attribute_category.name,
            attribute_category.description
        ))?;
    }
    statement.finalize()?;
    tx.commit()?;

    Ok(modified)
}

pub fn export_attributes<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "dgmAttributeTypes" (
            "attributeID" INTEGER NOT NULL,
            "attributeName" VARCHAR(100),
            description VARCHAR(1000),
            "iconID" INTEGER,
            "defaultValue" FLOAT,
            published BOOLEAN,
            "displayName" VARCHAR(150),
            "unitID" INTEGER,
            stackable BOOLEAN,
            "highIsGood" BOOLEAN,
            "categoryID" INTEGER,
            PRIMARY KEY ("attributeID"),
            CONSTRAINT dat_pub CHECK (published IN (0, 1)),
            CONSTRAINT dat_stack CHECK (stackable IN (0, 1)),
            CONSTRAINT dat_hig CHECK ("highIsGood" IN (0, 1))
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "dgmAttributeTypes""#, ())?;
    }

    let tx = db.transaction()?;
    let mut statement = tx.prepare(r#"INSERT INTO dgmAttributeTypes (
        attributeID,
        attributeName,
        description,
        iconID,
        defaultValue,
        published,
        displayName,
        unitID,
        stackable,
        highIsGood,
        categoryID
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#)?;

    let mut modified = 0;
    for res in sde.load_dogma_attributes()? {
        let attribute = res?;
        modified += statement.execute((
            attribute.attributeID,
            attribute.name,
            attribute.description,
            attribute.iconID,
            attribute.defaultValue,
            attribute.published,
            attribute.displayName.map(|s| s.en),
            attribute.unitID.map(EVEUnit::unit_id),
            attribute.stackable,
            attribute.highIsGood,
            attribute.attributeCategoryID
        ))?;
    }
    statement.finalize()?;
    tx.commit()?;

    Ok(modified)
}

pub fn export_effects<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "dgmEffects" (
            "effectID" INTEGER NOT NULL,
            "effectName" VARCHAR(400),
            "effectCategory" INTEGER,
            description VARCHAR(1000),
            guid VARCHAR(60),
            "iconID" INTEGER,
            "isOffensive" BOOLEAN,
            "isAssistance" BOOLEAN,
            "durationAttributeID" INTEGER,
            "trackingSpeedAttributeID" INTEGER,
            "dischargeAttributeID" INTEGER,
            "rangeAttributeID" INTEGER,
            "falloffAttributeID" INTEGER,
            "disallowAutoRepeat" BOOLEAN,
            published BOOLEAN,
            "displayName" VARCHAR(100),
            "isWarpSafe" BOOLEAN,
            "rangeChance" BOOLEAN,
            "electronicChance" BOOLEAN,
            "propulsionChance" BOOLEAN,
            distribution INTEGER,
            "sfxName" VARCHAR(20),
            "npcUsageChanceAttributeID" INTEGER,
            "npcActivationChanceAttributeID" INTEGER,
            "fittingUsageChanceAttributeID" INTEGER,
            "modifierInfo" TEXT,
            PRIMARY KEY ("effectID"),
            CONSTRAINT de_offense CHECK ("isOffensive" IN (0, 1)),
            CONSTRAINT de_assist CHECK ("isAssistance" IN (0, 1)),
            CONSTRAINT de_disallowar CHECK ("disallowAutoRepeat" IN (0, 1)),
            CONSTRAINT de_published CHECK (published IN (0, 1)),
            CONSTRAINT de_warpsafe CHECK ("isWarpSafe" IN (0, 1)),
            CONSTRAINT de_rangechance CHECK ("rangeChance" IN (0, 1)),
            CONSTRAINT de_elecchance CHECK ("electronicChance" IN (0, 1)),
            CONSTRAINT de_propchance CHECK ("propulsionChance" IN (0, 1))
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "dgmEffects""#, ())?;
    }

    let tx = db.transaction()?;
    let mut statement = tx.prepare(r#"INSERT INTO dgmEffects (
        effectID,
        effectName,
        effectCategory,
        description,
        guid,
        iconID,
        isOffensive,
        isAssistance,
        durationAttributeID,
        trackingSpeedAttributeID,
        dischargeAttributeID,
        rangeAttributeID,
        falloffAttributeID,
        disallowAutoRepeat,
        published,
        displayName,
        isWarpSafe,
        rangeChance,
        electronicChance,
        propulsionChance,
        distribution,
        sfxName,
        npcUsageChanceAttributeID,
        npcActivationChanceAttributeID,
        fittingUsageChanceAttributeID,
        modifierInfo
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#)?;

    let mut modified = 0;
    for res in sde.load_dogma_effects()? {
        let effect = res?;
        modified += statement.execute(rusqlite::params![
            effect.effectID,
            effect.name,
            effect.effectCategoryID,
            effect.description.map(|s| s.en),
            effect.guid,
            effect.iconID,
            effect.isOffensive,
            effect.isAssistance,
            effect.durationAttributeID,
            effect.trackingSpeedAttributeID,
            effect.dischargeAttributeID,
            effect.rangeAttributeID,
            effect.falloffAttributeID,
            effect.disallowAutoRepeat,
            effect.published,
            effect.displayName.map(|s| s.en),
            effect.isWarpSafe,
            effect.rangeChance,
            effect.electronicChance,
            effect.propulsionChance,
            effect.distribution,
            effect.sfxName,
            effect.npcUsageChanceAttributeID,
            effect.npcActivationChanceAttributeID,
            effect.fittingUsageChanceAttributeID,
            // This used to use YAML, but json is more convenient & accepted by yaml parsers
            if effect.modifierInfo.len() > 0 {
                let mut buf = String::new();
                buf.push('[');
                for modifierinfo in effect.modifierInfo {
                    buf.push_str(&serde_json::to_string(&modifierinfo).map_err(|_| SDEError::IntegrityError("Modifierinfo serialization should always succeed".to_owned()))?);
                    buf.push(',');
                }
                buf.truncate(buf.len() - 1);
                buf.push(']');
                Some(buf)
            } else {
                None
            }
        ])?;
    }
    statement.finalize()?;
    tx.commit()?;

    Ok(modified)
}

// No dgmExpressions anymore

pub fn export_type_dogma<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "dgmTypeAttributes" (
            "typeID" INTEGER NOT NULL,
            "attributeID" INTEGER NOT NULL,
            "value" FLOAT,
            PRIMARY KEY ("typeID", "attributeID")
        )"#, ())?;
        db.execute(r#"CREATE TABLE IF NOT EXISTS "dgmTypeEffects" (
            "typeID" INTEGER NOT NULL,
            "effectID" INTEGER NOT NULL,
            "isDefault" BOOLEAN,
            PRIMARY KEY ("typeID", "effectID"),
            CONSTRAINT dte_default CHECK ("isDefault" IN (0, 1))
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "dgmTypeAttributes""#, ())?;
        db.execute(r#"DELETE FROM "dgmTypeEffects""#, ())?;
    }

    let tx = db.transaction()?;
    let mut st_attribute = tx.prepare(r#"INSERT INTO dgmTypeAttributes (typeID, attributeID, value) VALUES (?, ?, ?)"#)?;
    let mut st_effect = tx.prepare(r#"INSERT INTO dgmTypeEffects (typeID, effectID, isDefault) VALUES (?, ?, ?)"#)?;

    let mut modified = 0;
    for res in sde.load_type_dogma()? {
        let dogma = res?;

        for (attribute_id, value) in dogma.dogmaAttributes {
            modified += st_attribute.execute((dogma.typeID, attribute_id, value))?;
        }

        for (effect_id, is_default) in dogma.dogmaEffects {
            modified += st_effect.execute((dogma.typeID, effect_id, is_default))?;
        }

    }
    st_attribute.finalize()?;
    st_effect.finalize()?;
    tx.commit()?;

    Ok(modified)
}

pub fn export_graphics<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "eveGraphics" (
            "graphicID" INTEGER NOT NULL,
            "sofFactionName" VARCHAR(100),
            "graphicFile" VARCHAR(256),
            "sofHullName" VARCHAR(100),
            "sofRaceName" VARCHAR(100),
            PRIMARY KEY ("graphicID")
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "eveGraphics""#, ())?;
    }

    let tx = db.transaction()?;
    let mut statement = tx.prepare(r#"INSERT INTO eveGraphics (
        graphicID,
        graphicFile,
        sofFactionName,
        sofHullName,
        sofRaceName
    ) VALUES (?, ?, ?, ?, ?)"#)?;

    let mut modified = 0;
    for res in sde.load_graphics()? {
        let graphic = res?;
        modified += statement.execute((
            graphic.graphicID,
            graphic.graphicFile,
            graphic.sofFactionName,
            graphic.sofHullName,
            graphic.sofRaceName
            // TODO: Loss of description
        ))?;
    }
    statement.finalize()?;
    tx.commit()?;

    Ok(modified)
}

pub fn export_icons<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "eveIcons" (
            "iconID" INTEGER NOT NULL,
            "iconFile" VARCHAR(500),
            PRIMARY KEY ("iconID")
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "eveIcons""#, ())?;
    }

    let tx = db.transaction()?;
    let mut statement = tx.prepare(r#"INSERT INTO eveIcons (
        iconID,
        iconFile
    ) VALUES (?, ?)"#)?;

    let mut modified = 0;
    for res in sde.load_icons()? {
        let icon = res?;
        modified += statement.execute((
            icon.iconID,
            icon.iconFile
            // TODO: Loss of description
        ))?;
    }
    statement.finalize()?;
    tx.commit()?;

    Ok(modified)
}

pub fn export_units<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "eveUnits" (
            "unitID" INTEGER NOT NULL,
            "unitName" VARCHAR(100),
            "displayName" VARCHAR(50),
            description VARCHAR(1000),
            PRIMARY KEY ("unitID")
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "eveUnits""#, ())?;
    }

    let tx = db.transaction()?;
    let mut statement = tx.prepare(r#"INSERT INTO eveUnits (
        unitID,
        unitName,
        displayName,
        description
    ) VALUES (?, ?, ?, ?)"#)?;

    let mut modified = 0;
    for res in sde.load_dogma_units()? {
        let unit = res?;
        modified += statement.execute((
            unit.unitID as u32,
            unit.name,
            unit.displayName.map(|s| s.en),
            unit.description.map(|s| s.en)
        ))?;
    }
    statement.finalize()?;
    tx.commit()?;

    Ok(modified)
}

// Merged industry
pub fn export_industry<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "industryActivity" (
            "typeID" INTEGER NOT NULL,
            "activityID" INTEGER NOT NULL,
            time INTEGER,
            PRIMARY KEY ("typeID", "activityID")
        )"#, ())?;
        db.execute(r#"CREATE TABLE IF NOT EXISTS "industryActivityMaterials" (
            "typeID" INTEGER,
            "activityID" INTEGER,
            "materialTypeID" INTEGER,
            quantity INTEGER
        )"#, ())?;

        db.execute(r#"CREATE TABLE IF NOT EXISTS "industryActivityProducts" (
            "typeID" INTEGER,
            "activityID" INTEGER,
            "productTypeID" INTEGER,
            quantity INTEGER
        )"#, ())?;
        db.execute(r#"CREATE TABLE IF NOT EXISTS "industryActivityProbabilities" (
            "typeID" INTEGER,
            "activityID" INTEGER,
            "productTypeID" INTEGER,
            probability DECIMAL(3, 2)
        )"#, ())?;
        db.execute(r#"CREATE TABLE IF NOT EXISTS "industryActivitySkills" (
            "typeID" INTEGER,
            "activityID" INTEGER,
            "skillID" INTEGER,
            level INTEGER
        )"#, ())?;
        db.execute(r#"CREATE TABLE IF NOT EXISTS "industryBlueprints" (
            "typeID" INTEGER NOT NULL,
            "maxProductionLimit" INTEGER,
            PRIMARY KEY ("typeID")
        )"#, ())?;
        db.execute(r#"CREATE TABLE IF NOT EXISTS "ramActivities" (
            "activityID" INTEGER NOT NULL,
            "activityName" VARCHAR(100),
            "iconNo" VARCHAR(5),
            description VARCHAR(1000),
            published BOOLEAN,
            PRIMARY KEY ("activityID"),
            CONSTRAINT ra_pub CHECK (published IN (0, 1))
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "industryActivity""#, ())?;
        db.execute(r#"DELETE FROM "industryActivityMaterials""#, ())?;
        db.execute(r#"DELETE FROM "industryActivityProducts""#, ())?;
        db.execute(r#"DELETE FROM "industryActivityProbabilities""#, ())?;
        db.execute(r#"DELETE FROM "industryActivitySkills""#, ())?;
        db.execute(r#"DELETE FROM "industryBlueprints""#, ())?;
        db.execute(r#"DELETE FROM "ramActivities""#, ())?;
    }

    let mut modified = 0;

    modified += db.execute(include_str!("./ramActivities.sql"), ())?;

    let tx = db.transaction()?;

    let mut st_activity = tx.prepare(r#"INSERT INTO industryActivity (typeID, activityID, time) VALUES (?, ?, ?)"#)?;
    let mut st_activity_materials = tx.prepare(r#"INSERT INTO industryActivityMaterials (typeID, activityID, materialTypeID, quantity) VALUES (?, ?, ?, ?)"#)?;
    let mut st_activity_products = tx.prepare(r#"INSERT INTO industryActivityProducts (typeID, activityID, productTypeID, quantity) VALUES (?, ?, ?, ?)"#)?;
    let mut st_activity_probabilities = tx.prepare(r#"INSERT INTO industryActivityProbabilities (typeID, activityID, productTypeID, probability) VALUES (?, ?, ?, ?)"#)?;
    let mut st_activity_skills = tx.prepare(r#"INSERT INTO industryActivitySkills (typeID, activityID, skillID, level) VALUES (?, ?, ?, ?)"#)?;
    let mut st_blueprint = tx.prepare(r#"INSERT INTO industryBlueprints (typeID, maxProductionLimit) VALUES (?, ?)"#)?;

    for res in sde.load_blueprints()? {
        let blueprint = res?;

        modified += st_blueprint.execute((blueprint.blueprintTypeID, blueprint.maxProductionLimit))?;
        for (activity_id, activity) in blueprint.activities {
            modified += st_activity.execute((blueprint.blueprintTypeID, activity_id, activity.time))?;
            for (material_type_id, quantity) in activity.materials {
                modified += st_activity_materials.execute((blueprint.blueprintTypeID, activity_id, material_type_id, quantity))?;
            }
            for (product_type_id, (quantity, probability)) in activity.products {
                modified += st_activity_products.execute((blueprint.blueprintTypeID, activity_id, product_type_id, quantity))?;
                if let Some(probability) = probability {
                    modified += st_activity_probabilities.execute((blueprint.blueprintTypeID, activity_id, product_type_id, probability))?;
                }
            }
            for (skill_type_id, level) in activity.skills {
                modified += st_activity_skills.execute((blueprint.blueprintTypeID, activity_id, skill_type_id, level))?;
            }
        }

    }
    st_activity.finalize()?;
    st_activity_materials.finalize()?;
    st_activity_products.finalize()?;
    st_activity_probabilities.finalize()?;
    st_activity_skills.finalize()?;
    st_blueprint.finalize()?;
    tx.commit()?;

    Ok(modified)
}

pub fn export_categories<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "invCategories" (
            "categoryID" INTEGER NOT NULL,
            "categoryName" VARCHAR(100),
            "iconID" INTEGER,
            published BOOLEAN,
            PRIMARY KEY ("categoryID"),
            CONSTRAINT invcat_published CHECK (published IN (0, 1))
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "invCategories""#, ())?;
    }

    let tx = db.transaction()?;
    let mut statement = tx.prepare(r#"INSERT INTO invCategories (
        categoryID,
        categoryName,
        iconID,
        published
    ) VALUES (?, ?, ?, ?)"#)?;

    let mut modified = 0;
    for res in sde.load_categories()? {
        let category = res?;
        modified += statement.execute((
            category.categoryID,
            category.name.en,
            category.iconID,
            category.published
        ))?;
    }
    statement.finalize()?;
    tx.commit()?;

    Ok(modified)
}

pub fn export_contraband_types<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "invContrabandTypes" (
            "factionID" INTEGER NOT NULL,
            "typeID" INTEGER NOT NULL,
            "standingLoss" FLOAT,
            "confiscateMinSec" FLOAT,
            "fineByValue" FLOAT,
            "attackMinSec" FLOAT,
            PRIMARY KEY ("factionID", "typeID")
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "invContrabandTypes""#, ())?;
    }

    let tx = db.transaction()?;
    let mut statement = tx.prepare(r#"INSERT INTO invContrabandTypes (
        factionID,
        typeID,
        standingLoss,
        confiscateMinSec,
        fineByValue,
        attackMinSec
    ) VALUES (?, ?, ?, ?, ?, ?)"#)?;

    let mut modified = 0;
    for res in sde.load_contraband_types()? {
        let contraband = res?;
        for (faction_id, contraband_info) in contraband.factions {
            modified += statement.execute((
                faction_id,
                contraband.typeID,
                contraband_info.standingLoss,
                contraband_info.confiscateMinSec,
                contraband_info.fineByValue,
                contraband_info.attackMinSec
            ))?;
        }
    }
    statement.finalize()?;
    tx.commit()?;

    Ok(modified)
}

pub fn export_control_tower_resources<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "invControlTowerResources" (
            "controlTowerTypeID" INTEGER NOT NULL,
            "resourceTypeID" INTEGER NOT NULL,
            purpose INTEGER,
            quantity INTEGER,
            "minSecurityLevel" FLOAT,
            "factionID" INTEGER,
            PRIMARY KEY ("controlTowerTypeID", "resourceTypeID")
        )"#, ())?;
        db.execute(r#"CREATE TABLE IF NOT EXISTS "invControlTowerResourcePurposes" (
            purpose INTEGER NOT NULL,
            "purposeText" VARCHAR(100),
            PRIMARY KEY (purpose)
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "invControlTowerResources""#, ())?;
        db.execute(r#"DELETE FROM "invControlTowerResourcePurposes""#, ())?;
    }

    let tx = db.transaction()?;

    let mut st_purposes = tx.prepare(r#"INSERT INTO invControlTowerResourcePurposes (purpose, purposeText) VALUES (?, ?)"#)?;
    for purpose in [ResourcePurpose::Online, ResourcePurpose::Power, ResourcePurpose::CPU, ResourcePurpose::Reinforce] {
        st_purposes.execute((purpose as u8, purpose.name()))?;
    }
    st_purposes.finalize()?;

    let mut statement = tx.prepare(r#"INSERT INTO invControlTowerResources (
        controlTowerTypeID,
        resourceTypeID,
        purpose,
        quantity,
        minSecurityLevel,
        factionID
    ) VALUES (?, ?, ?, ?, ?, ?)"#)?;

    let mut modified = 0;
    for res in sde.load_controltower_resources()? {
        let resources = res?;
        for info in resources.resources {
            modified += statement.execute((
                resources.towerTypeID,
                info.resourceTypeID,
                info.purpose as u8,
                info.quantity,
                info.minSecurityLevel,
                info.factionID
            ))?;
        }
    }
    statement.finalize()?;
    tx.commit()?;

    Ok(modified)
}

// Hardcoded
pub fn export_invflags(db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "invFlags" (
            "flagID" INTEGER NOT NULL,
            "flagName" VARCHAR(200),
            "flagText" VARCHAR(100),
            "orderID" INTEGER,
            PRIMARY KEY ("flagID")
        );"#, ())?;
    }
    if truncate {
        db.execute("DELETE FROM invFlags", ())?;
    }

    db.execute(include_str!("./invFlags.sql"), ()).map_err(ExportError::from)
}

pub fn export_groups<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "invGroups" (
            "groupID" INTEGER NOT NULL,
            "categoryID" INTEGER,
            "groupName" VARCHAR(100),
            "iconID" INTEGER,
            "useBasePrice" BOOLEAN,
            anchored BOOLEAN,
            anchorable BOOLEAN,
            "fittableNonSingleton" BOOLEAN,
            published BOOLEAN,
            PRIMARY KEY ("groupID"),
            CONSTRAINT invgroup_usebaseprice CHECK ("useBasePrice" IN (0, 1)),
            CONSTRAINT invgroup_anchored CHECK (anchored IN (0, 1)),
            CONSTRAINT invgroup_anchorable CHECK (anchorable IN (0, 1)),
            CONSTRAINT invgroup_fitnonsingle CHECK ("fittableNonSingleton" IN (0, 1)),
            CONSTRAINT invgroup_published CHECK (published IN (0, 1))
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "invGroups""#, ())?;
    }

    let tx = db.transaction()?;
    let mut statement = tx.prepare(r#"INSERT INTO invGroups (
        groupID,
        categoryID,
        groupName,
        iconID,
        useBasePrice,
        anchored,
        anchorable,
        fittableNonSingleton,
        published
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#)?;

    let mut modified = 0;
    for res in sde.load_groups()? {
        let group = res?;
        modified += statement.execute((
            group.groupID,
            group.categoryID,
            group.name.en,
            group.iconID,
            group.useBasePrice,
            group.anchored,
            group.anchorable,
            group.fittableNonSingleton,
            group.published
        ))?;
    }
    statement.finalize()?;
    tx.commit()?;

    Ok(modified)
}

// No invItems

pub fn export_market_groups<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "invMarketGroups" (
            "marketGroupID" INTEGER NOT NULL,
            "parentGroupID" INTEGER,
            "marketGroupName" VARCHAR(100),
            description VARCHAR(3000),
            "iconID" INTEGER,
            "hasTypes" BOOLEAN,
            PRIMARY KEY ("marketGroupID"),
            CONSTRAINT invmarketgroups_hastypes CHECK ("hasTypes" IN (0, 1))
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "invMarketGroups""#, ())?;
    }

    let tx = db.transaction()?;
    let mut statement = tx.prepare(r#"INSERT INTO invMarketGroups (
        marketGroupID,
        parentGroupID,
        marketGroupName,
        description,
        iconID,
        hasTypes
    ) VALUES (?, ?, ?, ?, ?, ?)"#)?;

    let mut modified = 0;
    for res in sde.load_market_groups()? {
        let market_group = res?;
        modified += statement.execute((
            market_group.marketGroupID,
            market_group.parentGroupID,
            market_group.name.en,
            market_group.description.map(|s| s.en),
            market_group.iconID,
            market_group.hasTypes
        ))?;
    }
    statement.finalize()?;
    tx.commit()?;

    Ok(modified)
}

pub fn export_meta_groups<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "invMetaGroups" (
            "metaGroupID" INTEGER NOT NULL,
            "metaGroupName" VARCHAR(100),
            description VARCHAR(1000),
            "iconID" INTEGER,
            PRIMARY KEY ("metaGroupID")
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "invMetaGroups""#, ())?;
    }

    let tx = db.transaction()?;
    let mut statement = tx.prepare(r#"INSERT INTO invMetaGroups (
        metaGroupID,
        metaGroupName,
        description,
        iconID
    ) VALUES (?, ?, ?, ?)"#)?;

    let mut modified = 0;
    for res in sde.load_meta_groups()? {
        let meta_group = res?;
        modified += statement.execute((
            meta_group.metaGroupID,
            meta_group.name.en,
            meta_group.description.map(|s| s.en),
            meta_group.iconID
        ))?;
    }
    statement.finalize()?;
    tx.commit()?;

    Ok(modified)
}

// invMetaTypes is generated in Types

// no invNames

// no invPositions  TODO: Generated with map data?

pub fn export_traits<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "invTraits" (
            "traitID" INTEGER NOT NULL,
            "typeID" INTEGER,
            "skillID" INTEGER,
            bonus FLOAT,
            "bonusText" TEXT,
            "unitID" INTEGER,
            PRIMARY KEY ("traitID")
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "invTraits""#, ())?;
    }

    let tx = db.transaction()?;
    let mut statement = tx.prepare(r#"INSERT INTO invTraits (
        traitID,
        typeID,
        skillID,
        bonus,
        bonusText,
        unitID
    ) VALUES (?, ?, ?, ?, ?, ?)"#)?;

    let mut modified = 0;
    let mut trait_id = 0;
    for res in sde.load_type_bonuses()? {
        let mut bonuses = res?;

        for (skill, mut bonus_list) in bonuses.skillBonuses {
            bonus_list.sort_by_key(|b| b.importance);
            for bonus in bonus_list {
                trait_id += 1;
                modified += statement.execute((trait_id, bonuses.typeID, skill, bonus.bonus, bonus.bonusText.en, bonus.unitID.map(EVEUnit::unit_id)))?;
            }
        }
        bonuses.roleBonuses.sort_by_key(|b| b.importance);
        for bonus in bonuses.roleBonuses {
            trait_id += 1;
            modified += statement.execute((trait_id, bonuses.typeID, -1, bonus.bonus, bonus.bonusText.en, bonus.unitID.map(EVEUnit::unit_id)))?;
        }
        bonuses.miscBonuses.sort_by_key(|b| b.importance);
        for bonus in bonuses.miscBonuses {
            trait_id += 1;
            modified += statement.execute((trait_id, bonuses.typeID, -2, bonus.bonus, bonus.bonusText.en, bonus.unitID.map(EVEUnit::unit_id)))?;
        }
    }
    statement.finalize()?;
    tx.commit()?;

    Ok(modified)
}

// TODO: Table for randomized materials
pub fn export_type_materials<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "invTypeMaterials" (
            "typeID" INTEGER NOT NULL,
            "materialTypeID" INTEGER NOT NULL,
            quantity INTEGER NOT NULL,
            PRIMARY KEY ("typeID", "materialTypeID")
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "invTypeMaterials""#, ())?;
    }

    let tx = db.transaction()?;
    let mut statement = tx.prepare(r#"INSERT INTO invTypeMaterials (
        typeID,
        materialTypeID,
        quantity
    ) VALUES (?, ?, ?)"#)?;

    let mut modified = 0;
    for res in sde.load_type_materials()? {
        let type_materials = res?;
        for TypeMaterial { materialTypeID, quantity } in type_materials.materials {
            modified += statement.execute((
                type_materials.typeID,
                materialTypeID,
                quantity
            ))?;
        }
    }
    statement.finalize()?;
    tx.commit()?;

    Ok(modified)
}

// No invTypeReactions, merged into blueprints

pub fn export_types<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "invTypes" (
            "typeID" INTEGER NOT NULL,
            "groupID" INTEGER,
            "typeName" VARCHAR(100),
            description TEXT,
            mass FLOAT,
            volume FLOAT,
            capacity FLOAT,
            "portionSize" INTEGER,
            "raceID" INTEGER,
            "basePrice" DECIMAL(19, 4),
            published BOOLEAN,
            "marketGroupID" INTEGER,
            "iconID" INTEGER,
            "soundID" INTEGER,
            "graphicID" INTEGER,
            PRIMARY KEY ("typeID"),
            CONSTRAINT invtype_published CHECK (published IN (0, 1))
        )"#, ())?;
        db.execute(r#"CREATE TABLE IF NOT EXISTS "invMetaTypes" (
            "typeID" INTEGER NOT NULL,
            "parentTypeID" INTEGER,
            "metaGroupID" INTEGER,
            PRIMARY KEY ("typeID")
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "invTypes""#, ())?;
        db.execute(r#"DELETE FROM "invMetaTypes""#, ())?;
    }

    let tx = db.transaction()?;
    let mut st_type = tx.prepare(r#"INSERT INTO "invTypes" (
        "typeID",
        "groupID",
        "typeName",
        "description",
        "mass",
        "volume",
        "capacity",
        "portionSize",
        "raceID",
        "basePrice",
        "published",
        "marketGroupID",
        "iconID",
        "soundID",
        "graphicID"
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#)?;
    let mut st_metatype = tx.prepare(r#"INSERT INTO "invMetaTypes" (
        typeID,
        parentTypeID,
        metaGroupID
    ) VALUES (?, ?, ?)"#)?;

    let mut modified = 0;
    for res in sde.load_types()? {
        let inv_type = res?;

        modified += st_type.execute((
            inv_type.typeID,
            inv_type.groupID,
            inv_type.name.en,
            inv_type.description.map(|s| s.en),
            inv_type.mass,
            inv_type.volume,
            inv_type.capacity,
            inv_type.portionSize,
            inv_type.raceID,
            inv_type.basePrice,
            inv_type.published,
            inv_type.marketGroupID,
            inv_type.iconID,
            inv_type.soundID,
            inv_type.graphicID
        ))?;

        modified += st_metatype.execute((
            inv_type.typeID,
            inv_type.variationParentTypeID,
            inv_type.metaGroupID.unwrap_or(1)
        ))?;
    }
    st_type.finalize()?;
    st_metatype.finalize()?;
    tx.commit()?;

    Ok(modified)
}

// no invUniqueNames
// no invVolumes; TODO: Add from hoboleaks


// TODO: Document that mapLocationScenes has been deprecated in favor of mapRegions#nebula

// Merged map tables
pub fn export_map_full<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "mapDenormalize" (
            "itemID" INTEGER NOT NULL,
            "typeID" INTEGER,
            "groupID" INTEGER,
            "solarSystemID" INTEGER,
            "constellationID" INTEGER,
            "regionID" INTEGER,
            "orbitID" INTEGER,
            x FLOAT,
            y FLOAT,
            z FLOAT,
            radius FLOAT,
            "itemName" VARCHAR(100),
            security FLOAT,
            "celestialIndex" INTEGER,
            "orbitIndex" INTEGER,
            PRIMARY KEY ("itemID")
        )"#, ())?;
        db.execute(r#"CREATE TABLE IF NOT EXISTS "mapCelestialGraphics" (
            "celestialID" INTEGER NOT NULL,
            "heightMap1" INTEGER,
            "heightMap2" INTEGER,
            "shaderPreset" INTEGER,
            population BOOLEAN,
            PRIMARY KEY ("celestialID"),
            CHECK (population IN (0, 1))
        )"#, ())?;
        db.execute(r#"CREATE TABLE IF NOT EXISTS "mapCelestialStatistics" (
            "celestialID" INTEGER NOT NULL,
            temperature FLOAT,
            "spectralClass" VARCHAR(10),
            luminosity FLOAT,
            age FLOAT,
            life FLOAT,
            "orbitRadius" FLOAT,
            eccentricity FLOAT,
            "massDust" FLOAT,
            "massGas" FLOAT,
            density FLOAT,
            "surfaceGravity" FLOAT,
            "escapeVelocity" FLOAT,
            "orbitPeriod" FLOAT,
            "rotationRate" FLOAT,
            locked BOOLEAN,
            pressure FLOAT,
            radius FLOAT,
            PRIMARY KEY ("celestialID"),
            CONSTRAINT mapcelestialstats_locked CHECK (locked IN (0, 1))
        )"#, ())?;
        db.execute(r#"CREATE TABLE IF NOT EXISTS "mapConstellationJumps" (
            "fromRegionID" INTEGER,
            "fromConstellationID" INTEGER NOT NULL,
            "toConstellationID" INTEGER NOT NULL,
            "toRegionID" INTEGER,
            PRIMARY KEY ("fromConstellationID", "toConstellationID")
        )"#, ())?;
        db.execute(r#"CREATE TABLE IF NOT EXISTS "mapConstellations" (
            "regionID" INTEGER,
            "constellationID" INTEGER NOT NULL,
            "constellationName" VARCHAR(100),
            x FLOAT,
            y FLOAT,
            z FLOAT,
            "factionID" INTEGER,
            PRIMARY KEY ("constellationID")
        )"#, ())?;
        db.execute(r#"CREATE TABLE IF NOT EXISTS "mapJumps" (
            "stargateID" INTEGER NOT NULL,
            "destinationID" INTEGER,
            PRIMARY KEY ("stargateID")
        )"#, ())?;
        db.execute(r#"CREATE TABLE IF NOT EXISTS "mapLandmarks" (
            "landmarkID" INTEGER NOT NULL,
            "landmarkName" VARCHAR(100),
            description TEXT,
            "locationID" INTEGER,
            x FLOAT,
            y FLOAT,
            z FLOAT,
            "iconID" INTEGER,
            PRIMARY KEY ("landmarkID")
        )"#, ())?;
        db.execute(r#"CREATE TABLE IF NOT EXISTS "mapLocationWormholeClasses" (
            "locationID" INTEGER NOT NULL,
            "wormholeClassID" INTEGER,
            PRIMARY KEY ("locationID")
        )"#, ())?;
        db.execute(r#"CREATE TABLE IF NOT EXISTS "mapRegionJumps" (
            "fromRegionID" INTEGER NOT NULL,
            "toRegionID" INTEGER NOT NULL,
            PRIMARY KEY ("fromRegionID", "toRegionID")
        )"#, ())?;
        db.execute(r#"CREATE TABLE IF NOT EXISTS "mapRegions" (
            "regionID" INTEGER NOT NULL,
            "regionName" VARCHAR(100),
            x FLOAT,
            y FLOAT,
            z FLOAT,
            "factionID" INTEGER,
            nebula INTEGER,
            PRIMARY KEY ("regionID")
        )"#, ())?;
        db.execute(r#"CREATE TABLE IF NOT EXISTS "mapSolarSystemJumps" (
            "fromRegionID" INTEGER,
            "fromConstellationID" INTEGER,
            "fromSolarSystemID" INTEGER NOT NULL,
            "toSolarSystemID" INTEGER NOT NULL,
            "toConstellationID" INTEGER,
            "toRegionID" INTEGER,
            PRIMARY KEY ("fromSolarSystemID", "toSolarSystemID")
        )"#, ())?;
        db.execute(r#"CREATE TABLE IF NOT EXISTS "mapSolarSystems" (
            "regionID" INTEGER,
            "constellationID" INTEGER,
            "solarSystemID" INTEGER NOT NULL,
            "solarSystemName" VARCHAR(100),
            x FLOAT,
            y FLOAT,
            z FLOAT,
            luminosity FLOAT,
            border BOOLEAN,
            fringe BOOLEAN,
            corridor BOOLEAN,
            hub BOOLEAN,
            international BOOLEAN,
            regional BOOLEAN,
            constellation BOOLEAN,
            security FLOAT,
            "factionID" INTEGER,
            radius FLOAT,
            "securityClass" VARCHAR(2),
            PRIMARY KEY ("solarSystemID"),
            CONSTRAINT mapss_border CHECK (border IN (0, 1)),
            CONSTRAINT mapss_fringe CHECK (fringe IN (0, 1)),
            CONSTRAINT mapss_corridor CHECK (corridor IN (0, 1)),
            CONSTRAINT mapss_hub CHECK (hub IN (0, 1)),
            CONSTRAINT mapss_internat CHECK (international IN (0, 1)),
            CONSTRAINT mapss_regional CHECK (regional IN (0, 1)),
            CONSTRAINT mapss_constel CHECK (constellation IN (0, 1))
        )"#, ())?;
        db.execute(r#"CREATE TABLE IF NOT EXISTS "staStations" (
            "stationID" BIGINT NOT NULL,
            security FLOAT,
            "operationID" INTEGER,
            "stationTypeID" INTEGER,
            "corporationID" INTEGER,
            "solarSystemID" INTEGER,
            "constellationID" INTEGER,
            "regionID" INTEGER,
            "stationName" VARCHAR(100),
            x FLOAT,
            y FLOAT,
            z FLOAT,
            "reprocessingEfficiency" FLOAT,
            "reprocessingStationsTake" FLOAT,
            "reprocessingHangarFlag" INTEGER,
            PRIMARY KEY ("stationID")
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "mapDenormalize""#, ())?;
        db.execute(r#"DELETE FROM "mapCelestialGraphics""#, ())?;
        db.execute(r#"DELETE FROM "mapCelestialStatistics""#, ())?;
        db.execute(r#"DELETE FROM "mapConstellationJumps""#, ())?;
        db.execute(r#"DELETE FROM "mapConstellations""#, ())?;
        db.execute(r#"DELETE FROM "mapJumps""#, ())?;
        db.execute(r#"DELETE FROM "mapLandmarks""#, ())?;
        db.execute(r#"DELETE FROM "mapLocationWormholeClasses""#, ())?;
        db.execute(r#"DELETE FROM "mapRegionJumps""#, ())?;
        db.execute(r#"DELETE FROM "mapRegions""#, ())?;
        db.execute(r#"DELETE FROM "mapSolarSystemJumps""#, ())?;
        db.execute(r#"DELETE FROM "mapSolarSystems""#, ())?;
        db.execute(r#"DELETE FROM "staStations""#, ())?;
    }

    let mut modified = 0;
    let tx = db.transaction()?;
    let mut st_whclasses = tx.prepare("INSERT INTO mapLocationWormholeClasses (locationID, wormholeClassID) VALUES (?, ?)")?;
    let mut st_denormalize = tx.prepare(r#"INSERT INTO mapDenormalize ("itemID","typeID","groupID","solarSystemID","constellationID","regionID","orbitID","x","y","z","radius","itemName","security","celestialIndex","orbitIndex") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);"#)?;

    let mut st_regions = tx.prepare(r#"INSERT INTO mapRegions (
        regionID,
        regionName,
        x,
        y,
        z,
        factionID,
        nebula
    ) VALUES (?, ?, ?, ?, ?, ?, ?)"#)?;
    for res in sde.load_regions()? {
        let region = res?;
        modified += st_regions.execute((
            region.regionID,
            &region.name.en,
            region.position.x,
            region.position.y,
            region.position.z,
            region.factionID,
            region.nebulaID
        ))?;

        if let Some(wormhole_class_id) = region.wormholeClassID {
            modified += st_whclasses.execute((region.regionID, wormhole_class_id))?;
        }

        modified += st_denormalize.execute(rusqlite::params![
            region.regionID,
            3,
            3,
            Null,   // solarSystemID
            Null,   // constellationID
            Null,   // regionID
            Null,   // orbitID
            region.position.x,
            region.position.y,
            region.position.z,
            Null,   // radius
            region.name.en,
            Null,   // security
            Null,   // celestialIndex
            Null    // orbitIndex
        ])?;
    }
    st_regions.finalize()?;

    let mut st_constellations = tx.prepare(r#"INSERT INTO mapConstellations (
        regionID,
        constellationID,
        constellationName,
        x,
        y,
        z,
        factionID
    ) VALUES (?, ?, ?, ?, ?, ?, ?)"#)?;
    for res in sde.load_constellations()? {
        let constellation = res?;
        modified += st_constellations.execute((
            constellation.regionID,
            constellation.constellationID,
            &constellation.name.en,
            constellation.position.x,
            constellation.position.y,
            constellation.position.z,
            constellation.factionID
        ))?;

        if let Some(wormhole_class_id) = constellation.wormholeClassID {
            modified += st_whclasses.execute((constellation.constellationID, wormhole_class_id))?;   // TODO: Cascade from regions
        }

        modified += st_denormalize.execute(rusqlite::params![
            constellation.constellationID,
            4,
            4,
            Null,   // solarSystemID
            Null,   // constellationID
            constellation.regionID,
            Null,   // orbitID
            constellation.position.x,
            constellation.position.y,
            constellation.position.z,
            Null,   // radius
            constellation.name.en,
            Null,   // security
            Null,   // celestialIndex
            Null    // orbitIndex
        ])?;
    }
    st_constellations.finalize()?;

    st_whclasses.finalize()?;
    st_denormalize.finalize()?;
    tx.commit()?;
    let tx = db.transaction()?;
    let mut st_whclasses = tx.prepare("INSERT INTO mapLocationWormholeClasses (locationID, wormholeClassID) VALUES (?, ?)")?;
    let mut st_denormalize = tx.prepare(r#"INSERT INTO mapDenormalize ("itemID","typeID","groupID","solarSystemID","constellationID","regionID","orbitID","x","y","z","radius","itemName","security","celestialIndex","orbitIndex") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);"#)?;

    let mut system_map = HashMap::with_capacity(9000);

    let mut st_systems = tx.prepare(r#"INSERT INTO mapSolarSystems (
        regionID,
        constellationID,
        solarSystemID,
        solarSystemName,
        x,
        y,
        z,
        luminosity,
        border,
        fringe,
        corridor,
        hub,
        international,
        regional,
        security,
        factionID,
        radius,
        securityClass
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#)?;
    for res in sde.load_solarsystems()? {
        let solarsystem = res?;

        modified += st_systems.execute(rusqlite::params![
            solarsystem.regionID,
            solarsystem.constellationID,
            solarsystem.solarSystemID,
            &solarsystem.name.en,
            solarsystem.position.x,
            solarsystem.position.y,
            solarsystem.position.z,
            solarsystem.luminosity,
            solarsystem.border.unwrap_or(false),
            solarsystem.fringe.unwrap_or(false),
            solarsystem.corridor.unwrap_or(false),
            solarsystem.hub.unwrap_or(false),
            solarsystem.international.unwrap_or(false),
            solarsystem.regional.unwrap_or(false),
            solarsystem.securityStatus,
            solarsystem.factionID,  // TODO: Cascade from regions/constellations
            solarsystem.radius,
            solarsystem.securityClass
        ])?;

        if let Some(wormhole_class_id) = solarsystem.wormholeClassID {
            modified += st_whclasses.execute((solarsystem.solarSystemID, wormhole_class_id))?;  // TODO: Cascade from regions/constellations
        }

        modified += st_denormalize.execute(rusqlite::params![
            solarsystem.solarSystemID,
            5,
            5,
            Null,   // solarSystemID
            solarsystem.constellationID,
            solarsystem.regionID,
            Null,   // orbitID
            solarsystem.position.x,
            solarsystem.position.y,
            solarsystem.position.z,
            Null,   // radius
            &solarsystem.name.en,
            Null,   // security TODO: Why is this set to null, even though systems have a security status?
            Null,   // celestialIndex
            Null    // orbitIndex
        ])?;

        system_map.insert(solarsystem.solarSystemID, solarsystem);
    }
    st_systems.finalize()?;

    st_whclasses.finalize()?;
    st_denormalize.finalize()?;
    tx.commit()?;

    let mut type_groups = HashMap::new();
    for res in sde.load_types()? {
        #[allow(non_snake_case)]
        let Type { typeID, groupID, .. } = res?;
        type_groups.insert(typeID, groupID);
    }

    // TODO: Re-order celestials such that mapDenormalize/mapCelestialGraphics/mapCelestialStatistics are in-order

    let mut celestial_names: HashMap<ids::ItemID, LocalizedString> = HashMap::new();

    let tx = db.transaction()?;
    let st_denormalize = tx.prepare(r#"INSERT INTO mapDenormalize ("itemID","typeID","groupID","solarSystemID","constellationID","regionID","orbitID","x","y","z","radius","itemName","security","celestialIndex","orbitIndex") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);"#)?;
    let mut st_statistics = tx.prepare(r#"INSERT INTO mapCelestialStatistics (
        "celestialID",
        "temperature",
        "spectralClass",
        "luminosity",
        "age",
        "life",
        "orbitRadius",
        "eccentricity",
        "massDust",
        "massGas",
        "density",
        "surfaceGravity",
        "escapeVelocity",
        "orbitPeriod",
        "rotationRate",
        "locked",
        "pressure",
        "radius"
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);"#)?;

    for res in sde.load_stars()? {
        let star = res?;

        celestial_names.insert(star.starID, system_map.get(&star.solarSystemID).ok_or_else(|| SDEError::IntegrityError(format!("Star in unknown solarsystem: #{}", star.solarSystemID)))?.name.clone());

        modified += st_statistics.execute(rusqlite::params![
            star.starID,
            star.statistics.temperature,
            star.statistics.spectralClass,
            star.statistics.luminosity,
            star.statistics.age,
            star.statistics.life,
            Null,   // orbitRadius
            Null,   // eccentricity
            Null,   // massDust
            Null,   // massGas
            Null,   // density
            Null,   // surfaceGravity
            Null,   // escapeVelocity
            Null,   // orbitPeriod
            Null,   // rotationRate
            false,  // locked
            Null,   // pressure
            star.radius
        ])?;
    }

    st_denormalize.finalize()?;
    st_statistics.finalize()?;
    tx.commit()?;
    let tx = db.transaction()?;
    let mut st_denormalize = tx.prepare(r#"INSERT INTO mapDenormalize ("itemID","typeID","groupID","solarSystemID","constellationID","regionID","orbitID","x","y","z","radius","itemName","security","celestialIndex","orbitIndex") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);"#)?;
    let mut st_graphics = tx.prepare(r#"INSERT INTO mapCelestialGraphics ("celestialID","heightMap1","heightMap2","shaderPreset","population") VALUES (?, ?, ?, ?, ?);"#)?;
    let mut st_statistics = tx.prepare(r#"INSERT INTO mapCelestialStatistics (
        "celestialID",
        "temperature",
        "spectralClass",
        "luminosity",
        "age",
        "life",
        "orbitRadius",
        "eccentricity",
        "massDust",
        "massGas",
        "density",
        "surfaceGravity",
        "escapeVelocity",
        "orbitPeriod",
        "rotationRate",
        "locked",
        "pressure",
        "radius"
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);"#)?;

    for res in sde.load_planets()? {
        let planet = res?;

        debug_assert!(planet.celestialIndex < 40, "Roman numeral logic is only correct up to 39");
        let parent_system = system_map.get(&planet.solarSystemID).ok_or_else(|| SDEError::IntegrityError(format!("Planet with unknown solarsystem: #{} in #{}", planet.planetID, planet.solarSystemID)))?;
        let name = planet.name(|system_id| system_map.get(&system_id).map(|sys| &sys.name).ok_or_else(|| SDEError::IntegrityError(format!("Planet in unknown solarsystem: #{}", system_id))))?;

        modified += st_denormalize.execute(rusqlite::params![
            planet.planetID,
            planet.typeID,
            type_groups.get(&planet.typeID).ok_or_else(|| SDEError::IntegrityError(format!("Unknown planet typeID: #{}", planet.typeID)))?,
            parent_system.solarSystemID,
            parent_system.constellationID,
            parent_system.regionID,
            planet.orbitID,
            planet.position.x,
            planet.position.y,
            planet.position.z,
            planet.radius,
            &name.en,
            parent_system.securityStatus,
            planet.celestialIndex,
            Null    // orbitIndex
        ])?;

        modified += st_graphics.execute((
            planet.planetID,
            planet.attributes.heightMap1,
            planet.attributes.heightMap2,
            planet.attributes.shaderPreset,
            planet.attributes.population
        ))?;

        modified += st_statistics.execute(rusqlite::params![
            planet.planetID,
            planet.statistics.temperature,
            planet.statistics.spectralClass,
            Null,   // Luminosity
            Null,   // Age
            Null,   // Life
            planet.statistics.orbitRadius,
            planet.statistics.eccentricity,
            planet.statistics.massDust,
            planet.statistics.massGas,
            planet.statistics.density,
            planet.statistics.surfaceGravity,
            planet.statistics.escapeVelocity,
            planet.statistics.orbitPeriod,
            planet.statistics.rotationRate,
            planet.statistics.locked,
            planet.statistics.pressure,
            planet.radius
        ])?;

        celestial_names.insert(planet.planetID, name);
    }

    st_denormalize.finalize()?;
    st_graphics.finalize()?;
    st_statistics.finalize()?;
    tx.commit()?;
    let tx = db.transaction()?;
    let mut st_denormalize = tx.prepare(r#"INSERT INTO mapDenormalize ("itemID","typeID","groupID","solarSystemID","constellationID","regionID","orbitID","x","y","z","radius","itemName","security","celestialIndex","orbitIndex") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);"#)?;
    let mut st_graphics = tx.prepare(r#"INSERT INTO mapCelestialGraphics ("celestialID","heightMap1","heightMap2","shaderPreset","population") VALUES (?, ?, ?, ?, ?);"#)?;
    let mut st_statistics = tx.prepare(r#"INSERT INTO mapCelestialStatistics (
        "celestialID",
        "temperature",
        "spectralClass",
        "luminosity",
        "age",
        "life",
        "orbitRadius",
        "eccentricity",
        "massDust",
        "massGas",
        "density",
        "surfaceGravity",
        "escapeVelocity",
        "orbitPeriod",
        "rotationRate",
        "locked",
        "pressure",
        "radius"
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);"#)?;

    for res in sde.load_moons()? {
        let moon = res?;

        let parent_system = system_map.get(&moon.solarSystemID).ok_or_else(|| SDEError::IntegrityError(format!("Moon with unknown solarsystem: #{} in #{}", moon.moonID, moon.solarSystemID)))?;
        let name = moon.name(|id| celestial_names.get(&id).ok_or_else(|| SDEError::IntegrityError(format!("Unknown planet: #{}", id))))?;

        modified += st_denormalize.execute(rusqlite::params![
            moon.moonID,
            moon.typeID,
            type_groups.get(&moon.typeID).ok_or_else(|| SDEError::IntegrityError(format!("Unknown moon typeID: #{}", moon.typeID)))?,
            moon.solarSystemID,
            parent_system.constellationID,
            parent_system.regionID,
            moon.orbitID,
            moon.position.x,
            moon.position.y,
            moon.position.z,
            moon.radius,
            name.en,
            parent_system.securityStatus,
            moon.celestialIndex,
            moon.orbitIndex
        ])?;

        modified += st_graphics.execute((
            moon.moonID,
            moon.attributes.heightMap1,
            moon.attributes.heightMap2,
            moon.attributes.shaderPreset,
            false
        ))?;

        if let Some(statistics) = moon.statistics {
            modified += st_statistics.execute(rusqlite::params![
                moon.moonID,
                statistics.temperature,
                statistics.spectralClass,
                Null,   // Luminosity
                Null,   // Age
                Null,   // Life
                statistics.orbitRadius,
                statistics.eccentricity,
                statistics.massDust,
                statistics.massGas,
                statistics.density,
                statistics.surfaceGravity,
                statistics.escapeVelocity,
                statistics.orbitPeriod,
                statistics.rotationRate,
                statistics.locked,
                statistics.pressure,
                moon.radius
            ])?;
        }

        celestial_names.insert(moon.moonID, name);
    }

    st_denormalize.finalize()?;
    st_graphics.finalize()?;
    st_statistics.finalize()?;
    tx.commit()?;
    let tx = db.transaction()?;
    let mut st_denormalize = tx.prepare(r#"INSERT INTO mapDenormalize ("itemID","typeID","groupID","solarSystemID","constellationID","regionID","orbitID","x","y","z","radius","itemName","security","celestialIndex","orbitIndex") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);"#)?;
    let mut st_jumps = tx.prepare(r#"INSERT INTO mapJumps ("stargateID", "destinationID") VALUES (?, ?);"#)?;
    let mut st_region_jumps = tx.prepare(r#"INSERT INTO mapRegionJumps ("fromRegionID", "toRegionID") VALUES (?, ?);"#)?;
    let mut st_constellation_jumps = tx.prepare(r#"INSERT INTO mapConstellationJumps ("fromRegionID", "fromConstellationID", "toConstellationID", "toRegionID") VALUES (?, ?, ?, ?);"#)?;
    let mut st_system_jumps = tx.prepare(r#"INSERT INTO mapSolarsystemJumps ("fromRegionID", "fromConstellationID", "fromSolarSystemID", "toSolarSystemID", "toConstellationID", "toRegionID") VALUES (?, ?, ?, ?, ?, ?);"#)?;

    let mut seen_region_jumps = HashSet::new();
    let mut seen_constellation_jumps = HashSet::new();
    for res in sde.load_stargates()? {
        let stargate = res?;

        let from_system = system_map.get(&stargate.solarSystemID).ok_or_else(|| SDEError::IntegrityError(format!("Stargate with unknown solarsystem: #{} in #{}", stargate.stargateID, stargate.solarSystemID)))?;
        let to_system = system_map.get(&stargate.destination.solarSystemID).ok_or_else(|| SDEError::IntegrityError(format!("Stargate with unknown destination solarsystem: #{} in #{}", stargate.stargateID, stargate.destination.solarSystemID)))?;

        modified += st_jumps.execute((stargate.stargateID, stargate.destination.stargateID))?;

        if from_system.regionID != to_system.regionID && seen_region_jumps.insert((from_system.regionID, to_system.regionID)) {
            modified += st_region_jumps.execute((from_system.regionID, to_system.regionID))?;
        }

        if from_system.constellationID != to_system.constellationID && seen_constellation_jumps.insert((from_system.constellationID, to_system.constellationID)) {
            modified += st_constellation_jumps.execute((from_system.regionID, from_system.constellationID, to_system.constellationID, to_system.regionID))?;
        }

        modified += st_system_jumps.execute((from_system.regionID, from_system.constellationID, from_system.solarSystemID, to_system.solarSystemID, to_system.constellationID, to_system.regionID))?;

        modified += st_denormalize.execute(rusqlite::params![
            stargate.stargateID,
            stargate.typeID,
            type_groups.get(&stargate.typeID).ok_or_else(|| SDEError::IntegrityError(format!("Unknown stargate typeID: #{}", stargate.typeID)))?,
            stargate.solarSystemID,
            from_system.constellationID,
            from_system.regionID,
            Null,   // orbitID
            stargate.position.x,
            stargate.position.y,
            stargate.position.z,
            Null,   // radius
            stargate.name(|id| system_map.get(&id).map(|s| &s.name).ok_or_else(|| SDEError::IntegrityError(format!("Stargate in unknown solarsystem: #{}", id))))?.en,
            from_system.securityStatus,
            Null,   // celestialIndex
            Null,   // orbitIndex
        ])?;
    }

    st_denormalize.finalize()?;
    st_jumps.finalize()?;
    st_region_jumps.finalize()?;
    st_constellation_jumps.finalize()?;
    st_system_jumps.finalize()?;
    tx.commit()?;

    let mut corporation_names = HashMap::new();
    for res in sde.load_npc_corporations()? {
        let corporation = res?;

        corporation_names.insert(corporation.corporationID, corporation.name);
    }

    let mut operation_names = HashMap::new();
    for res in sde.load_station_operations()? {
        let operation = res?;

        operation_names.insert(operation.operationID, operation.operationName);
    }

    let tx = db.transaction()?;
    let mut st_denormalize = tx.prepare(r#"INSERT INTO mapDenormalize ("itemID","typeID","groupID","solarSystemID","constellationID","regionID","orbitID","x","y","z","radius","itemName","security","celestialIndex","orbitIndex") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);"#)?;
    let mut st_stations = tx.prepare(r#"INSERT INTO staStations (
        "stationID",
        "security",
        "operationID",
        "stationTypeID",
        "corporationID",
        "solarSystemID",
        "constellationID",
        "regionID",
        "stationName",
        "x",
        "y",
        "z",
        "reprocessingEfficiency",
        "reprocessingStationsTake",
        "reprocessingHangarFlag"
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);"#)?;

    for res in sde.load_npc_stations()? {
        let station = res?;

        let parent_system = system_map.get(&station.solarSystemID).ok_or_else(|| SDEError::IntegrityError(format!("Station with unknown solarsystem: #{} in #{}", station.stationID, station.solarSystemID)))?;
        let name = station.name(
            |id| celestial_names.get(&id).ok_or_else(|| SDEError::IntegrityError(format!("Unknown celestial: #{}", id))),
            |id| operation_names.get(&id).ok_or_else(|| SDEError::IntegrityError(format!("Unknown station operation: #{}", id))),
            |id| corporation_names.get(&id).ok_or_else(|| SDEError::IntegrityError(format!("Unknown corporation: #{}", id)))
        )?;

        modified += st_stations.execute(rusqlite::params![
            station.stationID,
            parent_system.securityStatus,
            station.operationID,
            station.typeID,
            station.ownerID,
            station.solarSystemID,
            parent_system.constellationID,
            parent_system.regionID,
            &name.en,
            station.position.x,
            station.position.y,
            station.position.z,
            station.reprocessingEfficiency,
            station.reprocessingStationsTake,
            station.reprocessingHangarFlag,
        ])?;

        modified += st_denormalize.execute(rusqlite::params![
            station.stationID,
            station.typeID,
            type_groups.get(&station.typeID).ok_or_else(|| SDEError::IntegrityError(format!("Unknown station typeID: #{}", station.typeID)))?,
            station.solarSystemID,
            parent_system.constellationID,
            parent_system.regionID,
            Null,   // orbitID
            station.position.x,
            station.position.y,
            station.position.z,
            Null,   // radius
            name.en,
            parent_system.securityStatus,
            Null,   // celestialIndex
            Null,   // orbitIndex
        ])?;
    }

    st_denormalize.finalize()?;
    st_stations.finalize()?;
    tx.commit()?;
    let tx = db.transaction()?;
    let mut st_landmarks = tx.prepare(r#"INSERT INTO mapLandmarks ("landmarkID", "landmarkName", "description", "locationID", "x", "y", "z", "iconID") VALUES (?, ?, ?, ?, ?, ?, ?, ?);"#)?;

    for res in sde.load_landmarks()? {
        let landmark = res?;

        modified += st_landmarks.execute((
            landmark.landmarkID,
            landmark.name.en,
            landmark.description.en,
            landmark.locationID,
            landmark.position.x,
            landmark.position.y,
            landmark.position.z,
            landmark.iconID
        ))?;
    }
    st_landmarks.finalize()?;
    tx.commit()?;

    Ok(modified)
}

pub fn export_planet_schematics<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "planetSchematics" (
            "schematicID" INTEGER NOT NULL,
            "schematicName" VARCHAR(255),
            "cycleTime" INTEGER,
            PRIMARY KEY ("schematicID")
        )"#, ())?;
        db.execute(r#"CREATE TABLE IF NOT EXISTS "planetSchematicsPinMap" (
            "schematicID" INTEGER NOT NULL,
            "pinTypeID" INTEGER NOT NULL,
            PRIMARY KEY ("schematicID", "pinTypeID")
        )"#, ())?;
        db.execute(r#"CREATE TABLE IF NOT EXISTS "planetSchematicsTypeMap" (
            "schematicID" INTEGER NOT NULL,
            "typeID" INTEGER NOT NULL,
            quantity INTEGER,
            "isInput" BOOLEAN,
            PRIMARY KEY ("schematicID", "typeID"),
            CONSTRAINT pstm_input CHECK ("isInput" IN (0, 1))
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "planetSchematics""#, ())?;
        db.execute(r#"DELETE FROM "planetSchematicsPinMap""#, ())?;
        db.execute(r#"DELETE FROM "planetSchematicsTypeMap""#, ())?;
    }

    let tx = db.transaction()?;
    let mut st_schematic = tx.prepare("INSERT INTO planetSchematics (schematicID, schematicName, cycleTime) VALUES (?, ?, ?)")?;
    let mut st_pin = tx.prepare("INSERT INTO planetSchematicsPinMap (schematicID, pinTypeID) VALUES (?, ?)")?;
    let mut st_type = tx.prepare("INSERT INTO planetSchematicsTypeMap (schematicID, typeID, quantity, isInput) VALUES (?, ?, ?, ?)")?;

    let mut modified = 0;
    for res in sde.load_planet_schematics()? {
        let schematic = res?;

        modified += st_schematic.execute((schematic.schematicID, schematic.name.en, schematic.cycleTime))?;

        for pin in schematic.pins {
            modified += st_pin.execute((schematic.schematicID, pin))?;
        }

        for (type_id, schematic_type) in schematic.types {
            modified += st_type.execute((schematic.schematicID, type_id, schematic_type.quantity, schematic_type.isInput))?;
        }
    }
    st_schematic.finalize()?;
    st_pin.finalize()?;
    st_type.finalize()?;
    tx.commit()?;

    Ok(modified)
}

pub fn export_skins<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "skinLicense" (
            "licenseTypeID" INTEGER NOT NULL,
            duration INTEGER,
            "skinID" INTEGER,
            PRIMARY KEY ("licenseTypeID")
        )"#, ())?;
        db.execute(r#"CREATE TABLE IF NOT EXISTS "skinMaterials" (
            "skinMaterialID" INTEGER NOT NULL,
            "displayNameID" INTEGER,
            "materialSetID" INTEGER,
            PRIMARY KEY ("skinMaterialID")
        )"#, ())?;
        db.execute(r#"CREATE TABLE IF NOT EXISTS "skinShip" (
            "skinID" INTEGER,
            "typeID" INTEGER
        )"#, ())?;
        db.execute(r#"CREATE TABLE IF NOT EXISTS skins (
            "skinID" INTEGER NOT NULL,
            "internalName" VARCHAR(70),
            "skinMaterialID" INTEGER,
            PRIMARY KEY ("skinID")
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "skinLicense""#, ())?;
        db.execute(r#"DELETE FROM "skinMaterials""#, ())?;
        db.execute(r#"DELETE FROM "skinShip""#, ())?;
        db.execute(r#"DELETE FROM "skins""#, ())?;
    }

    let tx = db.transaction()?;
    let mut st_license = tx.prepare("INSERT INTO skinLicense (licenseTypeID, duration, skinID) VALUES (?, ?, ?)")?;
    let mut st_material = tx.prepare("INSERT INTO skinMaterials (skinMaterialID, displayNameID, materialSetID) VALUES (?, ?, ?)")?;
    let mut st_ship = tx.prepare("INSERT INTO skinShip (skinID, typeID) VALUES (?, ?)")?;
    let mut st_skin = tx.prepare("INSERT INTO skins (skinID, internalName, skinMaterialID) VALUES (?, ?, ?)")?;

    let mut modified = 0;
    for res in sde.load_skins()? {
        let skin = res?;
        modified += st_skin.execute((skin.skinID, skin.internalName, skin.skinMaterialID))?;

        for ship_type_id in skin.types {
            modified += st_ship.execute((skin.skinID, ship_type_id))?;
        }
    }
    for res in sde.load_skin_materials()? {
        let material = res?;
        modified += st_material.execute((material.materialID, material.displayName.map(|s| s.en), material.materialSetID))?;
    }
    for res in sde.load_skin_licenses()? {
        let license = res?;
        modified += st_license.execute((license.licenseTypeID, license.duration, license.skinID))?;
    }
    st_license.finalize()?;
    st_material.finalize()?;
    st_ship.finalize()?;
    st_skin.finalize()?;
    tx.commit()?;

    Ok(modified)
}

pub fn export_station_services<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    if create_table {
        db.execute(r#"CREATE TABLE IF NOT EXISTS "staOperationServices" (
            "operationID" INTEGER NOT NULL,
            "serviceID" INTEGER NOT NULL,
            PRIMARY KEY ("operationID", "serviceID")
        )"#, ())?;
        db.execute(r#"CREATE TABLE IF NOT EXISTS "staOperations" (
            "activityID" INTEGER,
            "operationID" INTEGER NOT NULL,
            "operationName" VARCHAR(100),
            description VARCHAR(1000),
            fringe INTEGER,
            corridor INTEGER,
            hub INTEGER,
            border INTEGER,
            ratio INTEGER,
            "caldariStationTypeID" INTEGER,
            "minmatarStationTypeID" INTEGER,
            "amarrStationTypeID" INTEGER,
            "gallenteStationTypeID" INTEGER,
            "joveStationTypeID" INTEGER,
            PRIMARY KEY ("operationID")
        )"#, ())?;
        db.execute(r#"CREATE TABLE IF NOT EXISTS "staServices" (
            "serviceID" INTEGER NOT NULL,
            "serviceName" VARCHAR(100),
            description VARCHAR(1000),
            PRIMARY KEY ("serviceID")
        )"#, ())?;
    }
    if truncate {
        db.execute(r#"DELETE FROM "staOperationServices""#, ())?;
        db.execute(r#"DELETE FROM "staOperations""#, ())?;
        db.execute(r#"DELETE FROM "staServices""#, ())?;
    }

    let tx = db.transaction()?;
    let mut st_operation_services = tx.prepare("INSERT INTO staOperationServices (operationID, serviceID) VALUES (?, ?)")?;
    let mut st_operations = tx.prepare("INSERT INTO staOperations (activityID, operationID, operationName, description, fringe, corridor, hub, border, ratio, caldariStationTypeID, minmatarStationTypeID, amarrStationTypeID, gallenteStationTypeID, joveStationTypeID) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
    let mut st_services = tx.prepare("INSERT INTO staServices (serviceID, serviceName, description) VALUES (?, ?, ?)")?;

    let mut modified = 0;
    for res in sde.load_station_operations()? {
        let operation = res?;

        modified += st_operations.execute((
            operation.activityID,
            operation.operationID,
            operation.operationName.en,
            operation.description.map(|s| s.en),
            operation.fringe * 100.0,
            operation.corridor * 100.0,
            operation.hub * 100.0,
            operation.border * 100.0,
            operation.ratio * 100.0,
            operation.stationTypes.get(&1),
            operation.stationTypes.get(&2),
            operation.stationTypes.get(&4),
            operation.stationTypes.get(&8),
            operation.stationTypes.get(&16)
        ))?;

        for service_id in operation.services {
            modified += st_operation_services.execute((operation.operationID, service_id))?;
        }
    }
    for res in sde.load_station_services()? {
        let service = res?;

        st_services.execute((service.serviceID, service.serviceName.en, service.description.map(|s| s.en)))?;
    }
    st_operation_services.finalize()?;
    st_operations.finalize()?;
    st_services.finalize()?;
    tx.commit()?;

    Ok(modified)
}

pub fn export_all<R: Read + Seek>(sde: &mut SDELoader<R>, db: &mut rusqlite::Connection, create_table: bool, truncate: bool) -> Result<usize, ExportError> {
    Ok(
        export_agent_types(sde, db, create_table, truncate)? +
            export_agents_in_space(sde, db, create_table, truncate)? +
            export_agents(sde, db, create_table, truncate)? +
            export_certs(sde, db, create_table, truncate)? +
            export_masteries(sde, db, create_table, truncate)? +
            export_ancestries(sde, db, create_table, truncate)? +
            export_character_attributes(sde, db, create_table, truncate)? +
            export_bloodlines(sde, db, create_table, truncate)? +
            export_factions(sde, db, create_table, truncate)? +
            export_races(sde, db, create_table, truncate)? +
            export_corporation_activities(sde, db, create_table, truncate)? +
            export_npc_corporations(sde, db, create_table, truncate)? +
            export_attribute_categories(sde, db, create_table, truncate)? +
            export_attributes(sde, db, create_table, truncate)? +
            export_effects(sde, db, create_table, truncate)? +
            export_type_dogma(sde, db, create_table, truncate)? +
            export_graphics(sde, db, create_table, truncate)? +
            export_icons(sde, db, create_table, truncate)? +
            export_units(sde, db, create_table, truncate)? +
            export_industry(sde, db, create_table, truncate)? +
            export_categories(sde, db, create_table, truncate)? +
            export_contraband_types(sde, db, create_table, truncate)? +
            export_control_tower_resources(sde, db, create_table, truncate)? +
            export_invflags(db, create_table, truncate)? +
            export_groups(sde, db, create_table, truncate)? +
            export_market_groups(sde, db, create_table, truncate)? +
            export_meta_groups(sde, db, create_table, truncate)? +
            export_traits(sde, db, create_table, truncate)? +
            export_type_materials(sde, db, create_table, truncate)? +
            export_types(sde, db, create_table, truncate)? +
            export_map_full(sde, db, create_table, truncate)? +
            export_planet_schematics(sde, db, create_table, truncate)? +
            export_skins(sde, db, create_table, truncate)? +
            export_station_services(sde, db, create_table, truncate)? +
            0
    )
}
