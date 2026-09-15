//! CAVEAT EMPTOR
//!
//! These are manually put together data-lists, to which the following caveats apply:
//! * Updates are manual and users must download the latest version of this crate to receive them.
//! * Data is likely to be outdated.
//! * Data may be erroneous.
//! * Data ignores game data marked as "un-published" unless explicitly stated otherwise.
//!
//! We are not responsible if your space pixels explode.


#[cfg(feature = "export_hardcoded")]
#[allow(deprecated)]
pub fn export<W: std::io::Write>(out: W) {
    use serde::{Serialize, Serializer};
    use crate::util::reflist::RefList;

    #[derive(serde::Serialize)]
    struct Exports<const N1: usize, const N2: usize> {
        #[serde(serialize_with="serialize_tuple_array")]
        constants: [(&'static str, f64); N1],
        #[serde(serialize_with="serialize_tuple_array")]
        holds: [(&'static str, cargo::CargoHoldType<'static>); N2],
        game_rules: GameRules
    }
    #[derive(serde::Serialize)]
    struct GameRules {
        cargo_in_ship_in_bay: RefList<'static>
    }

    fn serialize_tuple_array<K: Serialize, V: Serialize, S: Serializer, const N: usize>(array: &[(K, V); N], serializer: S) -> Result<S::Ok, S::Error> {
        // Delegate to collect_map, and convert ref-of-tuple to tuple of ref
        serializer.collect_map(array.into_iter().map(|(k, v): &(K, V)| (k, v)))
    }

    serde_json::to_writer_pretty(out, &Exports {
        constants: [
            ("MAX_TARGETING_RANGE", magic_constants::MAX_TARGETING_RANGE)
        ],
        holds: [
            ("SMB", cargo::SHIP_MAINTENANCE_BAY),
            ("SMB_RORQ", cargo::SHIP_MAINTENANCE_BAY_RORQUAL),
            ("FLEET", cargo::FLEET_HANGAR),
            ("FUEL", cargo::FUEL_BAY),
            ("MINING", cargo::MINING_HOLD),
            ("GAS", cargo::GAS_HOLD),
            ("MINERAL", cargo::MINERAL_HOLD),
            ("AMMO", cargo::AMMO_HOLD),
            ("COMMAND_CENTER", cargo::COMMAND_CENTER_HOLD),
            ("PI", cargo::PLANETARY_COMMODITIES_HOLD),
            ("QUAFE", cargo::QUAFE_HOLD),
            ("CORPSE", cargo::CORPSE_HOLD),
            ("BOOSTER", cargo::BOOSTER_HOLD),
            ("SUBSYSTEM", cargo::SUBSYSTEM_HOLD),
            ("ICE", cargo::ICE_HOLD),
            ("DEPOT", cargo::MOBILE_DEPOT_HOLD),
            ("INFRASTRUCTURE", cargo::INFRASTRUCTURE_HOLD),
            ("EXPEDITION", cargo::EXPEDITION_HOLD),
            ("FRIG_ESCAPE_BAY", cargo::FRIGATE_ESCAPE_BAY)
        ],
        game_rules: GameRules { cargo_in_ship_in_bay: cargo::CARGO_IN_SHIP_IN_BAY },
    }).unwrap();
}

pub mod magic_constants {
    pub const MAX_TARGETING_RANGE: f64 = 300_000.0;
}

pub mod id_ranges {
    use std::ops::RangeInclusive;

    pub const VARIOUS: RangeInclusive<u32> = 0..=499_999;
    pub const FACTIONS: RangeInclusive<u32> = 500_000..=599_999;
    pub const NPC_CORPS: RangeInclusive<u32> = 1_000_000..=1_999_999;
    pub const NPC_CHARS: RangeInclusive<u32> = 3_000_000..=3_999_999;
    pub const UNIVERSES: RangeInclusive<u32> = 9_000_000..=9_999_999;
    pub const REGIONS: RangeInclusive<u32> = 10_000_000..=19_999_999;
    pub const CONSTELLATIONS: RangeInclusive<u32> = 20_000_000..=29_999_999;
    pub const SOLARSYSTEMS: RangeInclusive<u32> = 30_000_000..=39_999_999;
    pub const CELESTIALS: RangeInclusive<u32> = 40_000_000..=49_999_999;
    pub const STARGATES: RangeInclusive<u32> = 50_000_000..=59_999_999;
    pub const STATIONS: RangeInclusive<u32> = 60_000_000..=69_999_999;
    /// Note: *NOT* Asteroid Belts, ids::AsteroidBeltID is under CELESTIALS
    pub const ASTEROIDS: RangeInclusive<u32> = 70_000_000..=79_999_999;
    pub const CONTROL_BUNKERS: RangeInclusive<u32> = 80_000_000..=80_099_999;
    pub const WIS_PROMENADES: RangeInclusive<u32> = 81_000_000..=81_999_999;    // Press 'F' to pay respects
    pub const PLANETARY_DISTRICTS: RangeInclusive<u32> = 82_000_000..=84_999_999;
    pub const EVE_CHARS_2: RangeInclusive<u32> = 90_000_000..=97_999_999;
    pub const EVE_CORPS_2: RangeInclusive<u32> = 98_000_000..=98_999_999;
    pub const EVE_ALLIANCES_2: RangeInclusive<u32> = 99_000_000..=99_999_999;
    pub const EVE_MIXED_CHARS_CORPS_ALLIANCES_1: RangeInclusive<u32> = 100_000_000..=2_099_999_999;
    pub const EVE_CHARS_3: RangeInclusive<u32> = 2_100_000_000..=2_111_999_999;
    pub const EVE_CHARS_4: RangeInclusive<u32> = 2_112_000_000..=2_129_999_999;
}

/// Information about cargo holds and their restrictions
///
/// Does not include information about unused holds (e.g. those on the Cockroach dev-only ship)
pub mod cargo {
    use crate::util::reflist::RefList;
    use crate::types::ids::AttributeID;

    #[cfg_attr(feature = "export_hardcoded", derive(serde::Serialize))]
    pub struct CargoHoldType<'a> {
        pub attribute_id: Option<AttributeID>,
        #[cfg_attr(feature = "export_hardcoded", serde(skip_serializing_if="Option::is_none"))]
        pub filter: Option<RefList<'a>>,
        pub packaged_ships: bool,
        pub assembled_ships: bool,
    }

    pub const SHIP_MAINTENANCE_BAY: CargoHoldType<'static> = CargoHoldType {
        attribute_id: Some(908),
        filter: Some(RefList {
            included_categories: &[6],  // Ships
            ..RefList::with_name("Ship Maintenance Bay Filter")
        }),
        packaged_ships: false,
        assembled_ships: true,
    };

    /// Canonical source: Attribute 1891 on ships that may be contained
    pub const SHIP_MAINTENANCE_BAY_RORQUAL: CargoHoldType<'static> = CargoHoldType {
        attribute_id: Some(908),
        filter: Some(RefList {
            included_types: &[
                32880,  // Venture
                89648,  // Venture Consortium Issue
                89240,  // Pioneer
                89647,  // Pioneer Consortium Issue
                89649,  // Outrider
                91174,  // Perseverance
                42244,  // Porpoise
            ],
            included_groups: &[
                28,     // Hauler
                380,    // Deep Space Transport
                1202,   // Blockade Runner
                463,    // Mining Barge
                543,    // Exhumer
                1283,   // Expedition Frigate
            ],
            ..RefList::with_name("Rorqual Ship Maintenance Bay Filter")
        }),
        packaged_ships: false,
        assembled_ships: true,
    };

    pub const FLEET_HANGAR: CargoHoldType<'static> = CargoHoldType {
        attribute_id: Some(912),
        filter: None,
        packaged_ships: true,
        assembled_ships: true,
    };

    pub const FUEL_BAY: CargoHoldType<'static> = CargoHoldType {
        attribute_id: Some(1549),
        filter: Some(RefList {
            included_groups: &[423],    // Ice product
            ..RefList::with_name("Fuel Bay Filter")
        }),
        packaged_ships: false,
        assembled_ships: false,
    };

    pub const MINING_HOLD: CargoHoldType<'static> = CargoHoldType {
        attribute_id: Some(1556),
        filter: Some(RefList { // TODO: Verify this list
            included_groups: &[711],    // Gas cloud
            included_categories: &[25], // Asteroid (= Ore types)
            ..RefList::with_name("Mining Hold Filter")
        }),
        packaged_ships: false,
        assembled_ships: false,
    };

    pub const GAS_HOLD: CargoHoldType<'static> = CargoHoldType {
        attribute_id: Some(1557),
        filter: Some(RefList {
            included_groups: &[711],    // Gas cloud
            ..RefList::with_name("Gas Hold Filter")
        }),
        packaged_ships: false,
        assembled_ships: false,
    };

    pub const MINERAL_HOLD: CargoHoldType<'static> = CargoHoldType {
        attribute_id: Some(1558),
        filter: Some(RefList {
            included_groups: &[18],    // Mineral
            ..RefList::with_name("Mineral Hold Filter")
        }),
        packaged_ships: false,
        assembled_ships: false,
    };
    
    pub const AMMO_HOLD: CargoHoldType<'static> = CargoHoldType {
        attribute_id: Some(1573),
        filter: Some(RefList {
            included_categories: &[8],    // Charge
            ..RefList::with_name("Ammo Hold Filter")
        }),
        packaged_ships: false,
        assembled_ships: false,
    };
    
    pub const COMMAND_CENTER_HOLD: CargoHoldType<'static> = CargoHoldType {
        attribute_id: Some(1646),
        filter: Some(RefList {
            included_groups: &[1027],   // Command Center
            ..RefList::with_name("Command Center Hold Filter")
        }),
        packaged_ships: false,
        assembled_ships: false,
    };
    
    pub const PLANETARY_COMMODITIES_HOLD: CargoHoldType<'static> = CargoHoldType {
        attribute_id: Some(1653),
        filter: Some(RefList {
            included_categories: &[
                42,     // Planetary Resources (T0/Raw resources)
                43      // Planetary Commodities
            ],
            ..RefList::with_name("Planetary Commodity Hold Filter")
        }),
        packaged_ships: false,
        assembled_ships: false,
    };
    
    // TODO: Possibly remove as the Quafe-edition ships with this have been converted into a SKIN?
    #[deprecated(note = "Quafe hold ships have been converted to SKINs")]
    pub const QUAFE_HOLD: CargoHoldType<'static> = CargoHoldType {
        attribute_id: Some(1804),
        filter: Some(RefList {
            included_types: &[
                3699,
                12865,
                57422,
                21661,
                3898,
                60575,
                12994,
            ],
            ..RefList::with_name("Quafe Hold Filter")
        }),
        packaged_ships: false,
        assembled_ships: false,
    };
    
    pub const CORPSE_HOLD: CargoHoldType<'static> = CargoHoldType {
        attribute_id: Some(2467),
        filter: Some(RefList {
            included_groups: &[14], // Biomass (corpses)
            ..RefList::with_name("Corpse Hold Filter")
        }),
        packaged_ships: false,
        assembled_ships: false,
    };

    pub const BOOSTER_HOLD: CargoHoldType<'static> = CargoHoldType {
        attribute_id: Some(2657),
        filter: Some(RefList {
            included_groups: &[303], // Booster
            ..RefList::with_name("Booster Hold Filter")
        }),
        packaged_ships: false,
        assembled_ships: false,
    };

    pub const SUBSYSTEM_HOLD: CargoHoldType<'static> = CargoHoldType {
        attribute_id: Some(2675),
        filter: Some(RefList {
            included_categories: &[32], // Subsystem
            ..RefList::with_name("Subsystem Hold Filter")
        }),
        packaged_ships: false,
        assembled_ships: false,
    };

    pub const ICE_HOLD: CargoHoldType<'static> = CargoHoldType {
        attribute_id: Some(3136),
        filter: Some(RefList {
            included_groups: &[465], // Ice
            ..RefList::with_name("Ice Hold Filter")
        }),
        packaged_ships: false,
        assembled_ships: false,
    };

    pub const MOBILE_DEPOT_HOLD: CargoHoldType<'static> = CargoHoldType {
        attribute_id: Some(5325),
        filter: Some(RefList {
            included_groups: &[1246], // Mobile Depot
            ..RefList::with_name("Mobile Depot Hold Filter")
        }),
        packaged_ships: false,
        assembled_ships: false,
    };

    pub const INFRASTRUCTURE_HOLD: CargoHoldType<'static> = CargoHoldType {
        attribute_id: Some(5646),
        filter: Some(RefList { // TODO Verify this list, in particular: PI control centers
            included_categories: &[
                42,     // Planetary Resources (T0/Raw resources)
                43,     // Planetary Commodities
                65,     // (Upwell) Structure
                66,     // Structure Module
                40,     // Sovereignty Structures (TODO (low priority): This category includes TCUs, verify if those are allowed)
                39,     // Infrastructure Upgrades
                22,     // Deployable
            ],
            included_groups: &[
                4729,   // Colony Reagents
                1546,   // Structure Anti-Capital Missile
                1547,   // Structure Anti-Subcapital Missile
                1548,   // (Structure) Guided Bomb
                1549,   // Structure ECM script
                1551,   // Structure Warp Disruptor Script
                1976,   // Structure Festival Charges
                4186,   // Structure Area Denial Ammunition
                4777,   // Structure Light Fighter
                4778,   // Structure Support Fighter
                4779,   // Structure Heavy Fighter
                4736,   // Skyhook
                1106,   // Orbital Construction Platform (Custom's Gantry)
                427,    // Moon Materials
                1136,   // Fuel Block
                42,     // Planetary Resources (T0/Raw resources)
                43,     // Planetary Commodities
                423,    // Ice product
            ],
            ..RefList::with_name("Infrastructure Hold Filter")
        }),
        packaged_ships: false,
        assembled_ships: false,
    };

    /// Canonical source: TypeList #1000
    pub const EXPEDITION_HOLD: CargoHoldType<'static> = CargoHoldType {
        attribute_id: Some(5944),
        filter: Some(RefList {
            included_categories: &[
                9,      // Blueprints
                16,     // Skillbook
                17,     // Commodity
                22,     // Deployable
                30,     // Apparel
                34,     // Ancient Relics
                63,     // Special Edition Assets
                91,     // SKINs
                2118    // Personalization (SKINR)
            ],
            included_groups: &[
                87,     // Capacitor Booster Charge
                303,    // Booster
                479,    // Scanner Probe (Regular + Combat)
                492,    // Survey Probe
                500,    // Festival Charges
                711,    // Harvestable Cloud (Gas)
                754,    // Salvaged Materials
                886,    // Rogue Drone Components
                966,    // Ancient Salvage (WH Space salvage)
                1304,   // Generic Decryptor (Invention decryptor)
                1676,   // Named Components (Exploration data site "Junk")
                1769,   // Shield Command Burst Charges
                1771,   // Mining Foreman Burst Charges
                1772,   // Skirmish Command Burst Charges
                1773,   // Information Command Burst Charges
                1774,   // Armor Command Burst Charges
                1976,   // Structure Festival Charges
                4168    // Compressed Gas
            ],
            included_types: &[28668],   // Nanite Repair Paste
            ..RefList::with_name("Expedition Hold Filter")
        }),
        packaged_ships: false,
        assembled_ships: false,
    };

    pub const FRIGATE_ESCAPE_BAY: CargoHoldType<'static> = CargoHoldType {
        attribute_id: Some(3020),
        filter: Some(RefList {
            included_groups: &[
                25,     // (T1) Frigate
                324,    // Assault Frigate
                893,    // Electronic Attack Ship
                1527,   // Logistics Frigate
            ],
            ..RefList::with_name("Frigate Escape Bay Filter")
        }),
        packaged_ships: false,
        assembled_ships: true,
    };

    /// Canonical source: TypeList #11
    pub const CARGO_IN_SHIP_IN_BAY: RefList<'static> = RefList {
        // TODO: Test these
        included_categories: &[
            7,          // Module
            8,          // Charge
            18,         // Drone
            20,         // Implant
            22,         // Deployable
            32,         // Subsystem
        ],
        included_groups: &[
            303,        // Booster
            361,        // Mobile Warp Disruptor    TODO: This is covered by deployable category
            1979,       // Abyssal Filaments
            4041,       // Jump Filaments
            4050,       // Abyssal Proving Filaments
            4087,       // Triglavian Space Filaments
        ],
        included_types: &[
            16273,      // Liquid Ozone
            16275       // Strontium Clathrates
        ],
        ..RefList::with_name("Cargo allowed in ships that are in SMBs/Frigate Bays")
    };
}