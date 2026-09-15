pub mod ids {
    // TODO: Reorganize these into an order that makes sense

    // Unique IDs, these may overlap
    pub type TypeID = u32;
    pub type GroupID = u32;
    pub type CategoryID = u32;
    pub type MetaGroupID = u32;
    pub type MarketGroupID = u32;
    pub type IconID = u32;
    pub type GraphicID = u32;
    pub type AttributeID = u32;
    pub type AttributeCategoryID = u32;
    pub type EffectID = u32;
    pub type EffectCategoryID = u32;
    pub type StationOperationID = u32;
    pub type StationServiceID = u32;
    pub type DivisionID = u32;
    pub type FlagID = u32;
    pub type AgentTypeID = u32;
    pub type SkinID = u32;
    pub type MaterialSetID = u32;
    pub type SkinMaterialID = u32;
    pub type SoundID = u32;
    pub type WormholeClassID = u32;
    pub type LandmarkID = u32;
    pub type DynamicBuffID = u32;
    pub type CareerID = u32;
    pub type SchoolID = u32;
    pub type SpecialtyID = u32;
    pub type DungeonArchetypeID = u32;
    pub type DungeonID = u32;
    pub type SpawnPointID = u32;
    pub type AncestryID = u32;
    pub type BloodlineID = u32;
    pub type RaceID = u32;
    pub type CharacterAttributeID = u32;
    pub type CertificateID = u32;
    pub type CorporationActivityID = u32;
    pub type PlanetSchematicID = u32;
    pub type CloneGradeID = u32;
    pub type JobSchemaID = u32;
    pub type IndustryActivityID = u32;
    pub type TypeListID = u32;
    pub type EpicArcID = u32;
    pub type MissionID = u32;
    pub type ShipTreeElementID = u32;
    pub type ShipTreeGroupID = u32;
    pub type SKINRComponentCategoryID = u32;
    pub type SKINRComponentRarity = u32;
    pub type SKINRComponentID = u32;
    pub type SKINRSlotCategoryID = u32;
    pub type AccountingEntryTypeID = u32;
    pub type CorporationRoleID = u32;
    pub type CorporationRoleGroupID = u32;
    pub type FighterAbilityID = u32;
    pub type IndustryAssemblyLineID = u32;
    pub type IndustryFilterID = u32;
    pub type NotificationTypeID = u32;
    pub type CareerPathID = u32;

    // ItemIDs
    pub type ItemID = u32;
    pub type SolarSystemID = ItemID;
    pub type ConstellationID = ItemID;
    pub type RegionID = ItemID;
    pub type AsteroidBeltID = ItemID;
    pub type MoonID = ItemID;
    pub type PlanetID = ItemID;
    pub type StarID = ItemID;
    pub type StargateID = ItemID;
    pub type StationID = ItemID;
    pub type CorporationID = ItemID;
    pub type FactionID = ItemID;
    pub type LocationID = ItemID;
    pub type CharacterID = ItemID;
}

pub mod uuids {
    use uuid::Uuid;

    #[allow(non_camel_case_types)]
    #[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
    #[cfg_attr(feature="sde_load", derive(serde::Deserialize))]
    pub struct EVE_UUID(
        #[cfg_attr(feature="sde_load", serde(with = "uuid::serde::hyphenated"))]
        pub Uuid
    );

    pub type CharacterTitleID = EVE_UUID;
    pub type MilitaryCampaignID = EVE_UUID;
    pub type MilitaryCampaignObjectiveID = EVE_UUID;
}

pub mod values {
    /// Range of `1..=5`
    pub type SkillLevel = u8;

    /// Range of `1..???` TODO
    pub type MetaLevel = u8;

    /// Range of `1..=10`, nominally `1`, `2`, or `3`
    pub type TechLevel = u8;

    /// SharedCache resource
    pub type CacheResource = String;
}
