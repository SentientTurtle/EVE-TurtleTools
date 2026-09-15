#![allow(unused)]

use std::collections::HashSet;
use std::error::Error;
use std::io::Write;
use serde::{Deserialize, Serialize};
use crate::sde::load::{SDELoader, TypeDogma};
use crate::types::ids;

#[derive(Debug, Default, Serialize, Deserialize)]
struct NPC {
    #[serde(rename="_key")]
    pub type_id: ids::TypeID,
    pub health: NPCHealth,
    #[serde(skip_serializing_if="NPCDamage::empty", default)]
    pub damage: NPCDamage,
    #[serde(skip_serializing_if="NPCEwar::empty", default)]
    pub ewar: NPCEwar,
    #[serde(skip_serializing_if="Option::is_none", default)]
    pub micro_jump: Option<NPCMicroJump>,
    #[serde(skip_serializing_if="NPCRepair::empty", default)]
    pub repair: NPCRepair,
    #[serde(skip_serializing_if="NPCRemoteRepair::empty", default)]
    pub remote_repair: NPCRemoteRepair,
    #[serde(skip_serializing_if="Option::is_none", default)]
    pub capacitor_boost: Option<NPCCapacitorBoost>,
    #[serde(skip_serializing_if="Option::is_none", default)]
    pub mining: Option<NPCMining>,
    #[serde(skip_serializing_if="Option::is_none", default)]
    pub siege: Option<NPCSiege>,
    #[serde(skip_serializing_if="NPCPropulsion::empty", default)]
    pub propulsion: NPCPropulsion,
    #[serde(skip_serializing_if="NPCSensors::empty", default)]
    pub sensors: NPCSensors,
    #[serde(skip_serializing_if="NPCBounty::empty", default)]
    pub bounty: NPCBounty,
    #[serde(skip_serializing_if="NPCStats::empty", default)]
    pub stats: NPCStats,
    #[serde(skip_serializing_if="std::ops::Not::not", default)]
    pub is_concord: bool
}

impl NPC {
    pub fn new_blank(type_id: ids::TypeID) -> NPC {
        NPC {
            type_id,
            ..NPC::default()
        }
    }

    fn skip_zero(n: &f64) -> bool {
        *n == 0.0
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct NPCHealth {
    #[serde(skip_serializing_if="NPCHPResistsShield::empty", default)]
    shield: NPCHPResistsShield,
    #[serde(skip_serializing_if="NPCHPResists::empty", default)]
    armor: NPCHPResists,
    #[serde(skip_serializing_if="NPCHPResists::empty", default)]
    hull: NPCHPResists,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct NPCHPResists {
    hp: f64,
    resist_em: f64,
    resist_th: f64,
    resist_ki: f64,
    resist_ex: f64,
}

impl NPCHPResists {
    pub fn empty(&self) -> bool {
        self.hp == 0.0 && self.resist_em == 0.0 && self.resist_th == 0.0 && self.resist_ki == 0.0 && self.resist_ex == 0.0
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct NPCHPResistsShield {
    hp: f64,
    regeneration: f64,
    resist_em: f64,
    resist_th: f64,
    resist_ki: f64,
    resist_ex: f64,
}

impl NPCHPResistsShield {
    pub fn empty(&self) -> bool {
        self.hp == 0.0
            && self.resist_em == 0.0
            && self.resist_th == 0.0
            && self.resist_ki == 0.0
            && self.resist_ex == 0.0
            && (self.regeneration.is_nan() || self.regeneration == 0.0)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "qty", rename_all="snake_case")]
enum Discharge {
    Probability(f64),
    Capacitor(f64)
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct NPCDamage {
    #[serde(skip_serializing_if="Option::is_none", default)]
    pub turret: Option<NPCTurret>,
    #[serde(skip_serializing_if="Option::is_none", default)]
    pub launcher: Option<NPCLauncher>,
    #[serde(skip_serializing_if="Option::is_none", default)]
    pub smartbomb: Option<NPCSmartbomb>,
    #[serde(skip_serializing_if="Option::is_none", default)]
    pub vorton_projector: Option<NPCVortonProjector>,
    #[serde(skip_serializing_if="Option::is_none", default)]
    pub superweapon: Option<NPCSuperweapon>,
    #[serde(skip_serializing_if="Option::is_none", default)]
    pub drones: Option<NPCDrones>
}

impl NPCDamage {
    fn empty(&self) -> bool {
        self.turret.is_none() && self.launcher.is_none() && self.smartbomb.is_none() && self.vorton_projector.is_none() && self.superweapon.is_none()
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct NPCTurret {
    pub dmg_em: f64,
    pub dmg_th: f64,
    pub dmg_ki: f64,
    pub dmg_ex: f64,
    pub interval: f64,
    pub tracking: f64,
    pub optimal_range: f64,
    #[serde(skip_serializing_if="NPC::skip_zero", default)]
    pub falloff: f64,
    pub discharge: Discharge
}

#[derive(Debug, Serialize, Deserialize)]
struct NPCLauncher {
    pub dmg_em: f64,
    pub dmg_th: f64,
    pub dmg_ki: f64,
    pub dmg_ex: f64,
    pub interval: f64,
    pub velocity: f64,
    pub flight_time: f64,
    pub explosion_radius: f64,
    pub explosion_velocity: f64,
    pub damage_reduction_factor: f64
}

#[derive(Debug, Serialize, Deserialize)]
struct NPCSmartbomb {
    pub dmg_em: f64,
    pub dmg_th: f64,
    pub dmg_ki: f64,
    pub dmg_ex: f64,
    pub range: f64,
    pub interval: f64,
    pub discharge: Discharge
}

#[derive(Debug, Serialize, Deserialize)]
struct NPCVortonProjector {
    pub dmg_em: f64,
    pub dmg_th: f64,
    pub dmg_ki: f64,
    pub dmg_ex: f64,
    pub range: f64,
    pub arc_range: f64,
    pub arc_chain: f64,
    pub interval: f64,
    pub discharge: Discharge
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct NPCDrones {
    max_deployed: f64,
    max_hold: f64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all="snake_case")]
enum NPCSuperweapon {
    Drifter {
        dmg_em: f64,
        dmg_th: f64,
        dmg_ki: f64,
        dmg_ex: f64,
        tracking: f64,
        optimal_range: f64,
        falloff: f64
    },
    Lance {
        dmg_em: f64,
        dmg_th: f64,
        dmg_ki: f64,
        dmg_ex: f64,
        optimal_range: f64,
        falloff: f64,
        tracking: f64
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct NPCEwar {
    #[serde(skip_serializing_if="Option::is_none", default)]
    pub warp_disrupt: Option<NPCEwarTackle>,
    #[serde(skip_serializing_if="Option::is_none", default)]
    pub warp_scramble: Option<NPCEwarTackle>,
    #[serde(skip_serializing_if="Option::is_none", default)]
    pub webifier: Option<NPCEwarWeb>,
    #[serde(skip_serializing_if="Option::is_none", default)]
    pub sensor_damp: Option<NPCEwarSensorDamp>,
    #[serde(skip_serializing_if="Option::is_none", default)]
    pub target_paint: Option<NPCEwarTargetPaint>,
    #[serde(skip_serializing_if="Option::is_none", default)]
    pub tracking_disrupt: Option<NPCEwarTrackingDisrupt>,
    #[serde(skip_serializing_if="Option::is_none", default)]
    pub guidance_disrupt: Option<NPCEwarGuidanceDisrupt>,
    #[serde(skip_serializing_if="Option::is_none", default)]
    pub neutralize: Option<NPCEwarNeut>,
    #[serde(skip_serializing_if="Option::is_none", default)]
    pub ecm: Option<NPCEwarECM>,
    #[serde(skip_serializing_if="Option::is_none", default)]
    pub remote_ecm_burst: Option<NPCEwarRemoteECM>,
    #[serde(skip_serializing_if="Option::is_none", default)]
    pub group_shield_harden: Option<NPCEwarGroupShieldHarden>,
}

impl NPCEwar {
    fn empty(&self) -> bool {
        self.warp_disrupt.is_none()
            && self.warp_scramble.is_none()
            && self.webifier.is_none()
            && self.sensor_damp.is_none()
            && self.target_paint.is_none()
            && self.tracking_disrupt.is_none()
            && self.guidance_disrupt.is_none()
            && self.neutralize.is_none()
            && self.ecm.is_none()
            && self.remote_ecm_burst.is_none()
            && self.group_shield_harden.is_none()
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct NPCEwarTackle {
    pub strength: f64,
    pub range: f64,
    pub duration: f64,
    pub discharge: Discharge
}

#[derive(Debug, Serialize, Deserialize)]
struct NPCEwarWeb {
    pub strength: f64,
    pub range: f64,
    pub duration: f64,
    pub discharge: Discharge
}

#[derive(Debug, Serialize, Deserialize)]
struct NPCEwarSensorDamp {
    pub strength_range: f64,
    pub strength_resolution: f64,
    pub range: f64,
    pub falloff: f64,
    pub duration: f64,
    pub discharge: Discharge
}

#[derive(Debug, Serialize, Deserialize)]
struct NPCEwarTargetPaint {
    pub strength: f64,
    pub range: f64,
    pub falloff: f64,
    pub duration: f64,
    pub discharge: Discharge
}

#[derive(Debug, Serialize, Deserialize)]
struct NPCEwarNeut {
    pub amount: f64,
    pub range: f64,
    pub falloff: f64,
    pub interval: f64,
    pub discharge: Discharge,
    pub is_nosferatu: bool
}

#[derive(Debug, Serialize, Deserialize)]
struct NPCEwarTrackingDisrupt {
    pub strength_tracking: f64,
    pub strength_optimal: f64,
    pub strength_falloff: f64,
    pub range: f64,
    pub falloff: f64,
    pub duration: f64,
    pub discharge: Discharge
}

#[derive(Debug, Serialize, Deserialize)]
struct NPCEwarGuidanceDisrupt {
    pub strength_explosion_velocity: f64,
    pub strength_explosion_radius: f64,
    pub strength_flighttime: f64,
    pub strength_missile_velocity: f64,
    pub range: f64,
    pub falloff: f64,
    pub duration: f64,
    pub discharge: Discharge
}

#[derive(Debug, Serialize, Deserialize)]
struct NPCEwarECM {
    pub strength_radar: f64,
    pub strength_magnetometric: f64,
    pub strength_gravimetric: f64,
    pub strength_ladar: f64,
    pub range: f64,
    pub falloff: f64,
    pub duration: f64,
    pub discharge: Discharge
}

/// Incursion ECM Burst mechanic
#[derive(Debug, Serialize, Deserialize)]
struct NPCEwarRemoteECM {
    pub strength_radar: f64,
    pub strength_magnetometric: f64,
    pub strength_gravimetric: f64,
    pub strength_ladar: f64,
    pub duration: f64,
    pub discharge: Discharge,
    pub scaling_minimum: f64,
    pub scaling_factor: f64,
    pub scaling_stepsize: f64,
    pub scaling_startsize: f64
}

/// Incursion shield harden effect
#[derive(Debug, Serialize, Deserialize)]
struct NPCEwarGroupShieldHarden {
    pub resistance_bonus: f64,
    pub duration: f64,
    pub discharge: Discharge
}

#[derive(Debug, Serialize, Deserialize)]
struct NPCMicroJump {
    pub distance: f64,
    pub range: f64,
    pub duration: f64,
    pub discharge: Discharge
}


#[derive(Debug, Default, Serialize, Deserialize)]
struct NPCRepair {
    #[serde(skip_serializing_if="Option::is_none", default)]
    pub shield: Option<NPCRegen>,
    #[serde(skip_serializing_if="Option::is_none", default)]
    pub armor: Option<NPCRegen>,
    #[serde(skip_serializing_if="Option::is_none", default)]
    pub hull: Option<NPCRegen>,
}

impl NPCRepair {
    fn empty(&self) -> bool {
        self.shield.is_none() && self.armor.is_none() && self.hull.is_none()
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct NPCRegen {
    pub amount: f64,
    pub interval: f64,
    pub discharge: Discharge
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct NPCRemoteRepair {
    #[serde(skip_serializing_if="Option::is_none", default)]
    pub shield: Option<NPCRemoteRegen>,
    #[serde(skip_serializing_if="Option::is_none", default)]
    pub armor: Option<NPCRemoteRegen>,
    #[serde(skip_serializing_if="Option::is_none", default)]
    pub hull: Option<NPCRemoteRegen>,
}

impl NPCRemoteRepair {
    fn empty(&self) -> bool {
        self.shield.is_none() && self.armor.is_none() && self.hull.is_none()
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct NPCCapacitorBoost {
    pub amount: f64,
    pub interval: f64,
    pub range: f64,
    #[serde(skip_serializing_if="NPC::skip_zero", default)]
    pub falloff: f64,
    pub discharge: Discharge
}

#[derive(Debug, Serialize, Deserialize)]
struct NPCRemoteRegen {
    pub amount: f64,
    pub interval: f64,
    pub threshold: f64,
    pub range: f64,
    #[serde(skip_serializing_if="NPC::skip_zero", default)]
    pub falloff: f64,
    pub max_targets: f64,
    pub discharge: Discharge
}

#[derive(Debug, Serialize, Deserialize)]
struct NPCMining {
    pub is_fake: bool,
    pub amount: f64,
    pub interval: f64,
    pub range: f64,
    pub discharge: Discharge
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum NPCSiege {
    Dreadnought {
        duration: f64,
        discharge: Discharge,

        /// Received remote repairs
        received_remote_repair_multiplier: f64,
        /// Received remote assistance (Sensor boost, tracking, ECCM)
        received_remote_assist_multiplier: f64,
        /// Received remote sensor dampeners
        received_remote_sensor_damp_multiplier: f64,
        /// Received tracking disruption
        remote_tracking_disrupt_multiplier: f64,
        /// Received ECM
        received_remote_ecm_multiplier: f64,
        /// Velocity multiplier applied to self
        velocity_multiplier: f64,
        mass_multiplier: f64,
        /// (Absolute) Warp scramble applied to self when sieged
        warp_scramble: f64,
        // TODO: Ask if NPCs can tether to the NPC citadels/eng-complexes
        disallow_tether: bool,
        local_logistics_amount_multiplier: f64,
        local_logistics_duration_multiplier: f64,
        turret_damage_multiplier: f64,
        missile_damage_multiplier: f64
    },
    IndustrialCore {
        duration: f64,
        discharge: Discharge,
        velocity_multiplier: f64,
        mass_multiplier: f64,
        received_remote_repair_multiplier: f64,
        received_remote_assist_multiplier: f64,
        received_remote_sensor_damp_multiplier: f64,
        received_remote_ecm_multiplier: f64,
        local_logistics_amount_multiplier: f64,
        local_logistics_duration_multiplier: f64,
        sent_remote_repair_amount_multiplier: f64,
        sent_remote_repair_duration_multiplier: f64,
        sent_remote_repair_capacitor_multiplier: f64,
        sent_remote_repair_range_multiplier: f64,
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct NPCPropulsion {
    mass: f64,
    inertia: f64,
    max_velocity: f64,
    #[serde(skip_serializing_if="NPC::skip_zero", default)]
    warp_speed: f64,
    #[serde(skip_serializing_if="NPC::skip_zero", default)]
    warp_core_strength: f64,
}

impl NPCPropulsion {
    pub fn empty(&self) -> bool {
        self.mass == 0.0 && self.inertia == 0.0 && self.max_velocity == 0.0 && self.warp_speed == 0.0 && self.warp_core_strength == 0.0
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct NPCSensors {
    targeting_range: f64,
    scan_resolution: f64,
    maximum_targets: f64,
    radar: f64,
    magnetometric: f64,
    gravimetric: f64,
    ladar: f64,
}

impl NPCSensors {
    pub fn empty(&self) -> bool {
        self.targeting_range == 0.0 && self.scan_resolution == 0.0 && self.maximum_targets == 0.0 && self.radar == 0.0 && self.magnetometric == 0.0 && self.gravimetric == 0.0 && self.ladar == 0.0
    }
}


#[derive(Debug, Default, Serialize, Deserialize)]
struct NPCBounty {
    #[serde(skip_serializing_if="NPC::skip_zero", default)]
    bounty: f64,
    #[serde(skip_serializing_if="NPC::skip_zero", default)]
    security_status_modifier: f64,
    #[serde(skip_serializing_if="NPC::skip_zero", default)]
    faction_status_modifier: f64,
    #[serde(skip_serializing_if="NPC::skip_zero", default)]
    entity_security_max_gain: f64,
}

impl NPCBounty {
    pub fn empty(&self) -> bool {
        self.bounty == 0.0 && self.security_status_modifier == 0.0 && self.faction_status_modifier == 0.0 && self.entity_security_max_gain == 0.0
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct NPCStats {   // TODO: Manual default impl with sensible defaults!
    #[serde(skip_serializing_if="NPC::skip_zero", default)]
    signature_radius: f64,
    #[serde(skip_serializing_if="NPC::skip_zero", default)]
    capacitor_capacity: f64,
    #[serde(skip_serializing_if="NPC::skip_zero", default)]
    capacitor_charge_rate: f64,
    #[serde(skip_serializing_if="Option::is_none", default)]
    attack_range: Option<f64>,
    #[serde(skip_serializing_if="NPC::skip_zero", default)]
    orbit_range: f64,
    #[serde(skip_serializing_if="std::ops::Not::not", default)]
    has_defender_missiles: bool,
    #[serde(skip_serializing_if="NPC::skip_zero", default)]
    strength_score: f64,
    #[serde(skip_serializing_if="std::ops::Not::not", default)]
    disallow_assistance: bool,
    #[serde(skip_serializing_if="std::ops::Not::not", default)]
    ewar_immunity: bool,
    #[serde(skip_serializing_if="NPC::skip_zero", default)]
    capacitor_warfare_resistance: f64,
    #[serde(skip_serializing_if="NPC::skip_zero", default)]
    smartbomb_resistance: f64,
    #[serde(skip_serializing_if="NPC::skip_zero", default)]
    bomb_resistance: f64,
    #[serde(skip_serializing_if="NPC::skip_zero", default)]
    vorton_resistance: f64,
    #[serde(skip_serializing_if="NPC::skip_zero", default)]
    lance_resistance: f64,
    #[serde(skip_serializing_if="std::ops::Not::not", default)]
    interdictor_immunity: bool,
    #[serde(skip_serializing_if="std::ops::Not::not", default)]
    superweapon_immune: bool,
    #[serde(skip_serializing_if="std::ops::Not::not", default)]
    is_capital: bool,
    #[serde(skip_serializing_if="std::ops::Not::not", default)]
    uses_bloodraider_nosferatu: bool,
    #[serde(skip_serializing_if="NPCBracketColour::is_default", default)]
    bracket_colour: NPCBracketColour,
    #[serde(skip_serializing_if="Option::is_none", default)]
    overview_icon_group: Option<f64>
}

impl NPCStats {
    pub fn empty(&self) -> bool {
        self.signature_radius == 0.0
            && self.capacitor_capacity == 0.0
            && self.capacitor_charge_rate == 0.0
            && self.attack_range.is_none()
            && self.orbit_range == 0.0
            && !self.has_defender_missiles
            && self.strength_score == 0.0
            && !self.disallow_assistance
            && !self.ewar_immunity
            && self.capacitor_warfare_resistance == 0.0
            && self.smartbomb_resistance == 0.0
            && self.bomb_resistance == 0.0
            && self.vorton_resistance == 0.0
            && self.lance_resistance == 0.0
            && !self.interdictor_immunity
            && !self.superweapon_immune
            && !self.is_capital
            && !self.uses_bloodraider_nosferatu
            && self.bracket_colour == NPCBracketColour::WHITE
            && self.overview_icon_group.is_none()
    }
}


#[derive(Debug, Default, Serialize, Deserialize, Eq, PartialEq)]
enum NPCBracketColour {
    #[default]
    WHITE,
    RED,
    BLUE
}

impl NPCBracketColour {
    fn is_default(&self) -> bool {
        *self == NPCBracketColour::WHITE
    }
}


const KNOWN_ATTRIBUTES: &[ids::AttributeID] = &[
    3, 4, 6, 9, 18, 20, 37, 51, 54, 55, 64, 68, 70, 73, 76, 79, 84, 90, 97, 98, 99, 103, 104, 105, 109, 110, 111, 113, 114, 116, 117, 118, 136, 154, 156, 157, 158, 160, 161, 162, 182, 188, 192, 193, 208,
    209, 210, 211, 212, 213, 217, 237, 238, 239, 240, 241, 243, 244, 245, 246, 247, 248, 249, 250, 251, 252, 253, 257, 263, 264, 265, 266, 267, 268, 269, 270, 271, 272, 273, 274, 277, 306, 309, 349, 351,
    416, 456, 457, 465, 466, 470, 475, 476, 479, 481, 482, 484, 497, 504, 505, 506, 507, 508, 512, 513, 514, 524, 525, 542, 544, 545, 547, 552, 554, 562, 563, 564, 565, 566, 580, 581, 582, 583, 594, 596,
    600, 620, 630, 631, 636, 637, 638, 639, 640, 645, 646, 653, 654, 661, 662, 665, 723, 725, 767, 797, 798, 831, 840, 841, 847, 848, 854, 858, 859, 860, 862, 872, 901, 903, 928, 929, 930, 931, 932, 933,
    935, 936, 937, 938, 941, 942, 943, 945, 946, 950, 953, 954, 974, 975, 976, 977, 1006, 1007, 1008, 1009, 1010, 1011, 1133, 1136, 1158, 1281, 1328, 1330, 1331, 1337, 1341, 1350, 1353, 1414, 1416, 1453,
    1454, 1455, 1456, 1458, 1459, 1460, 1462, 1464, 1470, 1489, 1501, 1502, 1538, 1579, 1582, 1648, 1649, 1650, 1651, 1652, 1654, 1655, 1656, 1658, 1659, 1660, 1661, 1662, 1663, 1664, 1671, 1672, 1673,
    1674, 1675, 1676, 1677, 1678, 1679, 1680, 1681, 1682, 1766, 1768, 1785, 1839, 1855, 1892, 1893, 1917, 1919, 1927, 1945, 2009, 2010, 2011, 2012, 2013, 2019, 2045, 2046, 2047, 2048, 2049, 2116, 2189,
    2244, 2259, 2260, 2261, 2266, 2433, 2452, 2489, 2490, 2491, 2492, 2493, 2494, 2495, 2496, 2497, 2498, 2499, 2500, 2501, 2502, 2503, 2504, 2505, 2506, 2507, 2508, 2509, 2510, 2511, 2512, 2513, 2514,
    2515, 2516, 2517, 2518, 2519, 2520, 2521, 2522, 2523, 2524, 2525, 2526, 2527, 2528, 2529, 2530, 2531, 2532, 2533, 2534, 2604, 2605, 2609, 2614, 2615, 2616, 2617, 2618, 2619, 2629, 2630, 2631, 2632,
    2633, 2634, 2635, 2636, 2637, 2638, 2639, 2640, 2641, 2642, 2643, 2644, 2645, 2646, 2647, 2648, 2649, 2654, 2673, 2674, 2723, 2724, 2725, 2730, 2733, 2734, 2784, 2785, 2786, 2788, 2812, 2814, 2815,
    2816, 2818, 2819, 2822, 2827, 3036, 3037, 3039, 3176, 5206, 5602, 5657, 5658, 5659, 5660, 5970, 5971, 5972, 6184, 6185, 6186, 6271, 6363
];

pub fn build_npc_data<W: Write>(loader: &mut SDELoader, out: &mut W) -> Result<(Vec<ids::AttributeID>, Vec<ids::EffectID>), Box<dyn Error>> {
    let attribute_info = loader.load_dogma_attributes_map()?;
    let effect_info = loader.load_dogma_effects_map()?;
    let npc_groups = loader.load_groups()?
        .filter_map(|r| match r { Ok(g) => if g.categoryID == 11 { Some(Ok(g.groupID)) } else { None } Err(err) => Some(Err(err)) })
        .collect::<Result<Vec<ids::GroupID>, _>>()?;
    let type_dogma = loader.load_type_dogma_map()?;

    let mut attributes: HashSet<ids::AttributeID> = HashSet::new();
    let mut effects: Vec<ids::EffectID> = Vec::new();

    for inv_type in loader.load_types()? {
        let inv_type = inv_type?;
        if !npc_groups.contains(&inv_type.groupID) { continue; }

        let mut npc = NPC::new_blank(inv_type.typeID);
        let npc_dogma = match type_dogma.get(&inv_type.typeID) {
            None => continue,   // Some unused NPCs have no dogma set
            Some(dogma) => dogma
        };

        attributes.extend(npc_dogma.dogmaAttributes.keys());

        // TODO: Review which attributes use a default value, and otherwise reject effect if attributes are missing
        let attribute_value = |type_dogma: &TypeDogma, attribute_id: ids::AttributeID| {
            Ok::<f64, String>(type_dogma.dogmaAttributes.get(&attribute_id).copied().unwrap_or(attribute_info.get(&attribute_id).ok_or_else(|| format!("missing Attribute info: {}", attribute_id))?.defaultValue))
        };

        if let Some(mass) = npc_dogma.dogmaAttributes.get(&4) {
            npc.propulsion.mass = *mass;
        }
        if let Some(hp_structure) = npc_dogma.dogmaAttributes.get(&9) {
            npc.health.hull.hp = *hp_structure;
        }
        if let Some(base_velocity) = npc_dogma.dogmaAttributes.get(&37) {
            npc.propulsion.max_velocity = npc.propulsion.max_velocity.max(*base_velocity);
        }
        // Capacitor recharge time is out of numerical order, as it's applied after capacitor capacity is known
        if let Some(inertia_modifier) = npc_dogma.dogmaAttributes.get(&70) {
            npc.propulsion.inertia = *inertia_modifier;
        }
        if let Some(max_target_range) = npc_dogma.dogmaAttributes.get(&76) {
            npc.sensors.targeting_range = *max_target_range;
        }
        if let Some(scan_speed_ms) = npc_dogma.dogmaAttributes.get(&79) {
            // TODO: Check what used for
        }
        if let Some(warp_core_status) = npc_dogma.dogmaAttributes.get(&104) {
            // Inverted warp core strength
            npc.propulsion.warp_core_strength = -*warp_core_status;
        }
        if let Some(hull_ki_resistance) = npc_dogma.dogmaAttributes.get(&109) {
            npc.health.hull.resist_ki = *hull_ki_resistance;
        }
        if let Some(hull_th_resistance) = npc_dogma.dogmaAttributes.get(&110) {
            npc.health.hull.resist_th = *hull_th_resistance;
        }
        if let Some(hull_ex_resistance) = npc_dogma.dogmaAttributes.get(&111) {
            npc.health.hull.resist_ex = *hull_ex_resistance;
        }
        if let Some(hull_em_resistance) = npc_dogma.dogmaAttributes.get(&113) {
            npc.health.hull.resist_em = *hull_em_resistance;
        }
        // 136 'uniformity' ignored; Is this really relevant to players?
        if let Some(proximity_range) = npc_dogma.dogmaAttributes.get(&154) {
            // TODO: Good defaults or remove
        }
        if let Some(orbit_range) = npc_dogma.dogmaAttributes.get(&157) {
            // Orbit range for a few NPCs, possibly remove as data seems corrupt
        }
        if let Some(max_locked_targets) = npc_dogma.dogmaAttributes.get(&192) {
            // Currently ignored as not-relevant
        }
        if let Some(max_attack_targets) = npc_dogma.dogmaAttributes.get(&193) {
            npc.sensors.maximum_targets = *max_attack_targets;
        }
        // TODO: Document which NPCs can be jammed
        if let Some(radar) = npc_dogma.dogmaAttributes.get(&208) {
            npc.sensors.radar = *radar;
        }
        if let Some(ladar) = npc_dogma.dogmaAttributes.get(&209) {
            npc.sensors.ladar = *ladar;
        }
        if let Some(magnetometric) = npc_dogma.dogmaAttributes.get(&210) {
            npc.sensors.magnetometric = *magnetometric;
        }
        if let Some(gravimetric) = npc_dogma.dogmaAttributes.get(&211) {
            npc.sensors.gravimetric = *gravimetric;
        }
        if let Some(attack_range) = npc_dogma.dogmaAttributes.get(&247) {
            // Not turret range
            npc.stats.attack_range = Some(*attack_range);
        }
        if let Some(security_status_modifier) = npc_dogma.dogmaAttributes.get(&252) {
            npc.bounty.security_status_modifier = *security_status_modifier;
        }
        if let Some(shield_hp) = npc_dogma.dogmaAttributes.get(&263) {
            npc.health.shield.hp = *shield_hp;
        }
        if let Some(armor_hp) = npc_dogma.dogmaAttributes.get(&265) {
            npc.health.armor.hp = *armor_hp;
        }
        if let Some(armor_resist_em) = npc_dogma.dogmaAttributes.get(&267) {
            npc.health.armor.resist_em = *armor_resist_em;
        }
        if let Some(armor_resist_ex) = npc_dogma.dogmaAttributes.get(&268) {
            npc.health.armor.resist_ex = *armor_resist_ex;
        }
        if let Some(armor_resist_ki) = npc_dogma.dogmaAttributes.get(&269) {
            npc.health.armor.resist_ki = *armor_resist_ki;
        }
        if let Some(armor_resist_th) = npc_dogma.dogmaAttributes.get(&270) {
            npc.health.armor.resist_th = *armor_resist_th;
        }
        if let Some(shield_resist_em) = npc_dogma.dogmaAttributes.get(&271) {
            npc.health.shield.resist_em = *shield_resist_em;
        }
        if let Some(shield_resist_ex) = npc_dogma.dogmaAttributes.get(&272) {
            npc.health.shield.resist_ex = *shield_resist_ex;
        }
        if let Some(shield_resist_ki) = npc_dogma.dogmaAttributes.get(&273) {
            npc.health.shield.resist_ki = *shield_resist_ki;
        }
        if let Some(shield_resist_th) = npc_dogma.dogmaAttributes.get(&274) {
            npc.health.shield.resist_th = *shield_resist_th;
        }
        if let Some(orbit_range) = npc_dogma.dogmaAttributes.get(&416) {
            npc.stats.orbit_range = *orbit_range;
        }
        if let Some(reaction_factor) = npc_dogma.dogmaAttributes.get(&466) {
            // The chance of an entity attacking the same person as its group members. Scales delay in joining in on fights too.
        }
        if let Some(attack_delay_min) = npc_dogma.dogmaAttributes.get(&475) {
            // div by 1000
        }
        if let Some(attack_delay_max) = npc_dogma.dogmaAttributes.get(&476) {
            // div by 1000
        }
        if let Some(shield_recharge_time) = npc_dogma.dogmaAttributes.get(&479) {
            // div by 1000

            npc.health.shield.regeneration = 2500.0 * npc.health.shield.hp / *shield_recharge_time;
        }
        if let Some(bounty) = npc_dogma.dogmaAttributes.get(&481) {
            npc.bounty.bounty = *bounty;
        }
        if let Some(capacitor_capacity) = npc_dogma.dogmaAttributes.get(&482) {
            npc.stats.capacitor_capacity = *capacitor_capacity;
        }
        if let Some(capacitor_recharge_time_ms) = npc_dogma.dogmaAttributes.get(&55) {
            npc.stats.capacitor_charge_rate = 2500.0 * npc.stats.capacitor_capacity / *capacitor_recharge_time_ms;
        }

        if let Some(missile_defender_chance) = npc_dogma.dogmaAttributes.get(&497) {
            npc.stats.has_defender_missiles = true;
        }
        if let Some(cruise_speed) = npc_dogma.dogmaAttributes.get(&508) {
            // Check if this is non MWD speed?
            npc.propulsion.max_velocity = npc.propulsion.max_velocity.max(*cruise_speed);
        }
        if let Some(strength_danger_score) = npc_dogma.dogmaAttributes.get(&542) {
            // Check what adjectives this translates to in-game
            npc.stats.strength_score = *strength_danger_score;
        }
        if let Some(signature_radius) = npc_dogma.dogmaAttributes.get(&552) {
            npc.stats.signature_radius = *signature_radius;
        }
        if let Some(faction_status_modifier) = npc_dogma.dogmaAttributes.get(&562) {
            npc.bounty.faction_status_modifier = *faction_status_modifier;
        }
        if let Some(entity_security_max_gain) = npc_dogma.dogmaAttributes.get(&563) {
            // TODO: Figure out how this is used
            npc.bounty.entity_security_max_gain = *entity_security_max_gain;
        }
        if let Some(scan_resolution) = npc_dogma.dogmaAttributes.get(&564) {
            npc.sensors.scan_resolution = *scan_resolution;
        }
        if let Some(warp_speed) = npc_dogma.dogmaAttributes.get(&600) {
            npc.propulsion.warp_speed = *warp_speed;
        }
        if let Some(mwd_chase_distance) = npc_dogma.dogmaAttributes.get(&665) {
            // Later
        }
        if let Some(max_targeting_range_cap) = npc_dogma.dogmaAttributes.get(&797) {
            // TODO: Maybe discard as it's used by only a few NPCs, mostly exists for structures/etc.
        }
        if let Some(entity_bracket_colour) = npc_dogma.dogmaAttributes.get(&798) {
            // 0: white (default)
            // 1: red (hostile NPC)
            // 2: blue (Neutral NPC)
            npc.stats.bracket_colour = if *entity_bracket_colour == 2.0 {
                NPCBracketColour::BLUE
            } else if *entity_bracket_colour == 1.0 {
                NPCBracketColour::RED
            } else {
                NPCBracketColour::WHITE
            }
        }
        if let Some(disallow_assistance) = npc_dogma.dogmaAttributes.get(&854) {
            // 0: false, 1: true
            npc.stats.disallow_assistance = *disallow_assistance != 0.0;
        }
        if let Some(ewar_immunity) = npc_dogma.dogmaAttributes.get(&872) {
            // If this module is in use and this attribute is 1, then offensive modules cannot be used on the ship if they apply modifiers for the duration of their effect. If this is put on a ship or NPC with value of 1, then the ship or NPC are immune to offensive modifiers (target jamming, tracking disruption etc.)
            npc.stats.ewar_immunity = *ewar_immunity != 0.0;
        }
        if let Some(hull_resonance_em) = npc_dogma.dogmaAttributes.get(&974) {
            npc.health.hull.resist_em = 1.0 - hull_resonance_em;
        }
        if let Some(hull_resonance_ex) = npc_dogma.dogmaAttributes.get(&975) {
            npc.health.hull.resist_ex = 1.0 - hull_resonance_ex;
        }
        if let Some(hull_resonance_ki) = npc_dogma.dogmaAttributes.get(&976) {
            npc.health.hull.resist_ki = 1.0 - hull_resonance_ki;
        }
        if let Some(hull_resonance_th) = npc_dogma.dogmaAttributes.get(&977) {
            npc.health.hull.resist_th = 1.0 - hull_resonance_th;
        }
        if let Some(mwd_signature_multiplier) = npc_dogma.dogmaAttributes.get(&1133) {
            // Later
        }
        if let Some(untargetable) = npc_dogma.dogmaAttributes.get(&1158) {
            // 0: false, 1: true
            // Only used by like 2 ships
        }
        if let Some(is_hacking) = npc_dogma.dogmaAttributes.get(&1330) {
            // 0: false, 1: true
        }
        if let Some(is_relic) = npc_dogma.dogmaAttributes.get(&1331) {
            // 0: false, 1: true
        }
        if let Some(warp_bubble_immune) = npc_dogma.dogmaAttributes.get(&1538) {
            // 0: false, 1: true
            npc.stats.interdictor_immunity = *warp_bubble_immune != 0.0;
        }
        if let Some(effect_deactivation_delay) = npc_dogma.dogmaAttributes.get(&1579) {
            // millisec, some drifter nonsense
        }
        if let Some(superweapon_immune) = npc_dogma.dogmaAttributes.get(&1654) {
            // 0: false, 1: true
            npc.stats.superweapon_immune = *superweapon_immune != 0.0;
        }
        if let Some(overview_icon_group) = npc_dogma.dogmaAttributes.get(&1766) {
            npc.stats.overview_icon_group = Some(*overview_icon_group);
        }
        if let Some(is_capital) = npc_dogma.dogmaAttributes.get(&1785) {
            // 0: false, 1: true
            npc.stats.is_capital = *is_capital != 0.0;
        }
        if let Some(ignore_drone_size) = npc_dogma.dogmaAttributes.get(&1855) {
            // Really specific behaviour, maybe later
        }
        if let Some(use_blood_nosferatu) = npc_dogma.dogmaAttributes.get(&1945) {
            // 0: false, 1: true
            npc.stats.uses_bloodraider_nosferatu = *use_blood_nosferatu != 0.0;
        }
        if let Some(capacitor_warfare_resistance) = npc_dogma.dogmaAttributes.get(&2045) {
            npc.stats.capacitor_warfare_resistance = 1.0 - *capacitor_warfare_resistance;
        }
        if let Some(drone_capacity_count) = npc_dogma.dogmaAttributes.get(&2784) {
            if let Some(drones) = &mut npc.damage.drones {
                drones.max_hold = *drone_capacity_count;
            } else {
                npc.damage.drones = Some(NPCDrones {
                    max_hold: *drone_capacity_count,
                    ..NPCDrones::default()
                })
            }
        }
        if let Some(drone_deployed_count) = npc_dogma.dogmaAttributes.get(&2785) {
            if let Some(drones) = &mut npc.damage.drones {
                drones.max_deployed = *drone_deployed_count;
            } else {
                npc.damage.drones = Some(NPCDrones {
                    max_deployed: *drone_deployed_count,
                    ..NPCDrones::default()
                })
            }
        }
        if let Some(smartbomb_resistance) = npc_dogma.dogmaAttributes.get(&6184) {
            npc.stats.smartbomb_resistance = 1.0 - *smartbomb_resistance;
        }
        if let Some(bomb_resistance) = npc_dogma.dogmaAttributes.get(&6185) {
            npc.stats.bomb_resistance = 1.0 - *bomb_resistance;
        }
        if let Some(vorton_resistance) = npc_dogma.dogmaAttributes.get(&6186) {
            npc.stats.vorton_resistance = 1.0 - *vorton_resistance;
        }
        if let Some(lance_resistance) = npc_dogma.dogmaAttributes.get(&6363) {
            npc.stats.lance_resistance = 1.0 - *lance_resistance;
        }


        for (effect_id, _) in &npc_dogma.dogmaEffects {
            match *effect_id {
                // Type-A NPC Turret
                10 => {
                    assert!(npc.damage.turret.is_none(), "duplicate turret effect for type: {}", inv_type.typeID);
                    assert!(npc.damage.smartbomb.is_none());
                    let dmg_mult = attribute_value(npc_dogma, 64)?;
                    npc.damage.turret = Some(NPCTurret {
                        dmg_em: attribute_value(npc_dogma, 114)? * dmg_mult,
                        dmg_th: attribute_value(npc_dogma, 118)? * dmg_mult,
                        dmg_ki: attribute_value(npc_dogma, 117)? * dmg_mult,
                        dmg_ex: attribute_value(npc_dogma, 116)? * dmg_mult,
                        interval: attribute_value(npc_dogma, 51)? / 1000.0,
                        tracking: attribute_value(npc_dogma, 160)?,
                        optimal_range: attribute_value(npc_dogma, 54)?,
                        falloff: attribute_value(npc_dogma, 158)?,
                        discharge: Discharge::Capacitor(attribute_value(npc_dogma, 6)?),
                    });
                },

                // Type-B NPC Turret (Triglavian disintegrator)
                6995 => {
                    // Duplicate effect allowed, override #10 (But #10 is not allowed to override this effect!)
                    let dmg_mult = attribute_value(npc_dogma, 64)?
                        // Multiply by maximum ramp-up, with a default value of 1.0 if unset
                        * npc_dogma.dogmaAttributes.get(&2734).copied().unwrap_or(1.0);
                    npc.damage.turret = Some(NPCTurret {
                        dmg_em: attribute_value(npc_dogma, 114)? * dmg_mult,
                        dmg_th: attribute_value(npc_dogma, 118)? * dmg_mult,
                        dmg_ki: attribute_value(npc_dogma, 117)? * dmg_mult,
                        dmg_ex: attribute_value(npc_dogma, 116)? * dmg_mult,
                        interval: attribute_value(npc_dogma, 51)? / 1000.0,
                        tracking: attribute_value(npc_dogma, 160)?,
                        optimal_range: attribute_value(npc_dogma, 54)?,
                        falloff: attribute_value(npc_dogma, 158)?,
                        discharge: Discharge::Capacitor(attribute_value(npc_dogma, 6)?),
                    });
                },

                // NPC Missile
                569 => {
                    assert!(npc.damage.launcher.is_none());
                    let missile_id = attribute_value(npc_dogma, 507)? as ids::TypeID;
                    let missile_dogma = match type_dogma.get(&missile_id) {
                        None => continue,   // Some unused NPCs have junk data
                        Some(dogma) => dogma
                    };

                    let dmg_mult = attribute_value(npc_dogma, 212)?;
                    npc.damage.launcher = Some(NPCLauncher {
                        dmg_em: attribute_value(missile_dogma, 114)? * dmg_mult,
                        dmg_th: attribute_value(missile_dogma, 118)? * dmg_mult,
                        dmg_ki: attribute_value(missile_dogma, 117)? * dmg_mult,
                        dmg_ex: attribute_value(missile_dogma, 116)? * dmg_mult,
                        interval: attribute_value(npc_dogma, 506)? / 1000.0,
                        velocity: attribute_value(missile_dogma, 37)? * attribute_value(npc_dogma, 645)?,
                        flight_time: attribute_value(missile_dogma, 281)? * attribute_value(npc_dogma, 646)? / 1000.0,
                        explosion_radius: attribute_value(missile_dogma, 654)? * attribute_value(npc_dogma, 858)?,
                        explosion_velocity: attribute_value(missile_dogma, 653)? * attribute_value(npc_dogma, 859)?,
                        damage_reduction_factor: attribute_value(missile_dogma, 1353)?,
                    });
                },

                // NPC Smartbomb
                7188 => {
                    // TODO: Verify that this overrides effect #10 turrets
                    assert!(npc.damage.smartbomb.is_none());
                    if npc_dogma.dogmaEffects.contains_key(&10) { npc.damage.turret = None; }
                    npc.damage.smartbomb = Some(NPCSmartbomb {
                        dmg_em: attribute_value(npc_dogma, 114)?,
                        dmg_th: attribute_value(npc_dogma, 118)?,
                        dmg_ki: attribute_value(npc_dogma, 117)?,
                        dmg_ex: attribute_value(npc_dogma, 116)?,
                        range: attribute_value(npc_dogma, 99)?,
                        interval: attribute_value(npc_dogma, 2812)? / 1000.0,
                        discharge: Discharge::Capacitor(attribute_value(npc_dogma, 2814)?),
                    });
                }

                // NPC Vorton Projector
                8088 => {
                    assert!(npc.damage.vorton_projector.is_none());
                    npc.damage.vorton_projector = Some(NPCVortonProjector {
                        dmg_em: attribute_value(npc_dogma, 114)?,
                        dmg_th: attribute_value(npc_dogma, 118)?,
                        dmg_ki: attribute_value(npc_dogma, 117)?,
                        dmg_ex: attribute_value(npc_dogma, 116)?,
                        range: attribute_value(npc_dogma, 54)?,
                        arc_range: attribute_value(npc_dogma, 3036)?,
                        arc_chain: attribute_value(npc_dogma, 3037)?,
                        interval: attribute_value(npc_dogma, 51)? / 1000.0,
                        discharge: Discharge::Capacitor(attribute_value(npc_dogma, 6)?),
                    });
                }

                // Type-A Superweapon (Drifter)
                6042 => {
                    assert!(npc.damage.superweapon.is_none());
                    npc.damage.superweapon = Some(NPCSuperweapon::Drifter {
                        dmg_em: attribute_value(npc_dogma, 2010)?,
                        dmg_th: attribute_value(npc_dogma, 2012)?,
                        dmg_ki: attribute_value(npc_dogma, 2011)?,
                        dmg_ex: attribute_value(npc_dogma, 2013)?,
                        tracking: attribute_value(npc_dogma, 2048)?,
                        optimal_range: attribute_value(npc_dogma, 2046)?,
                        falloff: attribute_value(npc_dogma, 2047)?
                    });
                }

                // Type-B Superweapon (Dreadnought Lance)
                11716 => {
                    assert!(npc.damage.superweapon.is_none());
                    npc.damage.superweapon = Some(NPCSuperweapon::Lance {
                        dmg_em: attribute_value(npc_dogma, 2010)?,
                        dmg_th: attribute_value(npc_dogma, 2012)?,
                        dmg_ki: attribute_value(npc_dogma, 2011)?,
                        dmg_ex: attribute_value(npc_dogma, 2013)?,
                        tracking: attribute_value(npc_dogma, 2048)?,
                        optimal_range: attribute_value(npc_dogma, 2046)?,
                        falloff: attribute_value(npc_dogma, 2047)?
                    });
                }


                // Type-A NPC Shield Rep
                effect_id @ (2192 | 2193 | 2194) => {
                    assert!(npc.repair.shield.is_none());
                    let effect_info = effect_info.get(&effect_id).ok_or_else(|| format!("missing Effect info for effect: {}", effect_id))?;
                    npc.repair.shield = Some(NPCRegen {
                        amount: attribute_value(npc_dogma, 637)?,
                        interval: attribute_value(npc_dogma, 636)? / 1000.0,
                        discharge: Discharge::Probability(1.0 - attribute_value(npc_dogma, effect_info.npcActivationChanceAttributeID.ok_or_else(|| format!("missing activation chance on effect: {}", effect_id))?)?),
                    });
                }

                // Type-B NPC shield rep
                876 => {
                    assert!(npc.repair.shield.is_none());
                    let interval = attribute_value(npc_dogma, 636)? / 1000.0;
                    npc.repair.shield = Some(NPCRegen {
                        amount: attribute_value(npc_dogma, 637)? * interval,
                        interval,
                        discharge: Discharge::Probability(1.0 - attribute_value(npc_dogma, 639)?),
                    });
                }
                5371 => {
                    assert!(npc.repair.shield.is_none());
                    let interval = attribute_value(npc_dogma, 636)? / 1000.0;
                    npc.repair.shield = Some(NPCRegen {
                        amount: attribute_value(npc_dogma, 1893)? * interval,
                        interval,
                        discharge: Discharge::Probability(1.0 - attribute_value(npc_dogma, 639)?),
                    });
                }

                // Type-C NPC Shield Rep
                6990 => {
                    assert!(npc.repair.shield.is_none());
                    npc.repair.shield = Some(NPCRegen {
                        amount: attribute_value(npc_dogma, 2723)?,
                        interval: attribute_value(npc_dogma, 2725)? / 1000.0,
                        discharge: Discharge::Capacitor(attribute_value(npc_dogma, 2724)?),
                    });
                }

                // Type-A NPC Armor rep
                effect_id @ (2195 | 2196 | 2197) => {
                    assert!(npc.repair.armor.is_none());
                    let effect_info = effect_info.get(&effect_id).ok_or_else(|| format!("missing Effect info for effect: {}", effect_id))?;
                    npc.repair.armor = Some(NPCRegen {
                        amount: attribute_value(npc_dogma, 631)?,
                        interval: attribute_value(npc_dogma, 630)? / 1000.0,
                        discharge: Discharge::Probability(1.0 - attribute_value(npc_dogma, effect_info.npcActivationChanceAttributeID.ok_or_else(|| format!("missing activation chance on effect: {}", effect_id))?)?),
                    });
                }

                // Type-B NPC Armor rep
                878 => {
                    assert!(npc.repair.armor.is_none());
                    let interval = attribute_value(npc_dogma, 630)? / 1000.0;
                    npc.repair.armor = Some(NPCRegen {
                        amount: attribute_value(npc_dogma, 631)? * interval,
                        interval,
                        discharge: Discharge::Probability( 1.0 - attribute_value(npc_dogma, 638)?),
                    });
                }
                5370 => {
                    assert!(npc.repair.armor.is_none());
                    let interval = attribute_value(npc_dogma, 630)? / 1000.0;
                    npc.repair.armor = Some(NPCRegen {
                        amount: attribute_value(npc_dogma, 1892)? * interval,
                        interval,
                        discharge: Discharge::Probability(1.0 - attribute_value(npc_dogma, 638)?),
                    });
                }

                // Type-C NPC Armor rep
                6884 => {
                    assert!(npc.repair.armor.is_none());
                    npc.repair.armor = Some(NPCRegen {
                        amount: attribute_value(npc_dogma, 2635)?,
                        interval: attribute_value(npc_dogma, 2633)? / 1000.0,
                        discharge: Discharge::Capacitor(attribute_value(npc_dogma, 2634)?),
                    });
                }

                // Type-A NPC Remote Shield Rep
                3855 => {
                    assert!(npc.remote_repair.shield.is_none());
                    npc.remote_repair.shield = Some(NPCRemoteRegen {
                        amount: attribute_value(npc_dogma, 1460)?,
                        interval: attribute_value(npc_dogma, 1458)? / 1000.0,
                        threshold: 1.0 - attribute_value(npc_dogma, 1462)?,
                        range: attribute_value(npc_dogma, 1464)?,
                        falloff: 0.0,
                        max_targets: attribute_value(npc_dogma, 1502)?,
                        discharge: Discharge::Probability(attribute_value(npc_dogma, 1459)?),
                    });
                }

                // Type-B NPC Remote Shield Rep
                6742 => {
                    assert!(npc.remote_repair.shield.is_none());
                    npc.remote_repair.shield = Some(NPCRemoteRegen {
                        amount: attribute_value(npc_dogma, 68)?,
                        interval: attribute_value(npc_dogma, 2495)? / 1000.0,
                        threshold: 1.0,
                        range: attribute_value(npc_dogma, 2496)?,
                        falloff: attribute_value(npc_dogma, 2497)?,
                        max_targets: 1.0,
                        discharge: Discharge::Capacitor(attribute_value(npc_dogma, 2498)?),
                    });
                }

                // Type-A NPC Remote Armor Rep
                3852 | 6165 => {
                    assert!(npc.remote_repair.armor.is_none());
                    npc.remote_repair.armor = Some(NPCRemoteRegen {
                        amount: attribute_value(npc_dogma, 1455)?,
                        interval: attribute_value(npc_dogma, 1454)? / 1000.0,
                        threshold: 1.0 - attribute_value(npc_dogma, 1456)?,
                        range: attribute_value(npc_dogma, 1464)?,
                        falloff: 0.0,
                        max_targets: attribute_value(npc_dogma, 1501)?,
                        discharge: Discharge::Probability(attribute_value(npc_dogma, 1453)?),
                    });
                }

                // Type-B NPC Remote Armor Rep
                6741 => {
                    assert!(npc.remote_repair.armor.is_none());
                    npc.remote_repair.armor = Some(NPCRemoteRegen {
                        amount: attribute_value(npc_dogma, 84)?,
                        interval: attribute_value(npc_dogma, 2491)? / 1000.0,
                        threshold: 1.0,
                        range: attribute_value(npc_dogma, 2492)?,
                        falloff: attribute_value(npc_dogma, 2493)?,
                        max_targets: 1.0,
                        discharge: Discharge::Capacitor(attribute_value(npc_dogma, 2494)?),
                    });
                }

                // Type-C NPC Remote Armor Rep
                6687 => {
                    assert!(npc.remote_repair.armor.is_none());
                    npc.remote_repair.armor = Some(NPCRemoteRegen {
                        amount: attribute_value(npc_dogma, 84)?,
                        interval: attribute_value(npc_dogma, 73)? / 1000.0,
                        threshold: 1.0,
                        range: attribute_value(npc_dogma, 54)?,
                        falloff: 0.0,
                        max_targets: 1.0,
                        discharge: Discharge::Capacitor(attribute_value(npc_dogma, 6)?),
                    });
                }

                // NPC Remote Capacitor Boost
                12073 => {
                    assert!(npc.capacitor_boost.is_none());
                    npc.capacitor_boost = Some(NPCCapacitorBoost {
                        amount: attribute_value(npc_dogma, 90)?,
                        interval: attribute_value(npc_dogma, 5657)? / 1000.0,
                        range: attribute_value(npc_dogma, 5658)?,
                        falloff: attribute_value(npc_dogma, 5659)?,
                        discharge: Discharge::Capacitor(attribute_value(npc_dogma, 5660)?),
                    });
                }

                // Type-A NPC Disrupt
                563 => {
                    assert!(npc.ewar.warp_disrupt.is_none());
                    assert!(npc.ewar.warp_scramble.is_none());
                    npc.ewar.warp_disrupt = Some(NPCEwarTackle {
                        strength: attribute_value(npc_dogma, 105)?,
                        range: attribute_value(npc_dogma, 103)?,
                        duration: attribute_value(npc_dogma, 505)?,
                        discharge: Discharge::Probability(attribute_value(npc_dogma, 504)?),
                    });
                }

                // Type-B NPC Disrupt
                6744 => {
                    assert!(npc.ewar.warp_disrupt.is_none());
                    assert!(npc.ewar.warp_scramble.is_none());
                    npc.ewar.warp_disrupt = Some(NPCEwarTackle {
                        strength: attribute_value(npc_dogma, 2510)?,
                        range: attribute_value(npc_dogma, 2504)?,
                        duration: attribute_value(npc_dogma, 2503)?,
                        discharge: Discharge::Capacitor(attribute_value(npc_dogma, 2505)?),
                    });
                }

                // Type-A NPC Scramble
                5928 => {
                    assert!(npc.ewar.warp_disrupt.is_none());
                    assert!(npc.ewar.warp_scramble.is_none());
                    npc.ewar.warp_scramble = Some(NPCEwarTackle {
                        strength: attribute_value(npc_dogma, 105)?,
                        range: attribute_value(npc_dogma, 103)?,
                        duration: attribute_value(npc_dogma, 505)?,
                        discharge: Discharge::Probability(attribute_value(npc_dogma, 504)?),
                    });
                }
                39 => {
                    assert!(npc.ewar.warp_disrupt.is_none());
                    assert!(npc.ewar.warp_scramble.is_none());
                    npc.ewar.warp_scramble = Some(NPCEwarTackle {
                        strength: attribute_value(npc_dogma, 105)?,
                        range: attribute_value(npc_dogma, 54)?,
                        duration: attribute_value(npc_dogma, 73)?,
                        discharge: Discharge::Capacitor(attribute_value(npc_dogma, 6)?)
                    });
                }

                // Type-B NPC Scramble
                6745 => {
                    assert!(npc.ewar.warp_disrupt.is_none());
                    assert!(npc.ewar.warp_scramble.is_none());
                    npc.ewar.warp_scramble = Some(NPCEwarTackle {
                        strength: attribute_value(npc_dogma, 2509)?,
                        range: attribute_value(npc_dogma, 2507)?,
                        duration: attribute_value(npc_dogma, 2506)?,
                        discharge: Discharge::Capacitor(attribute_value(npc_dogma, 2508)?),
                    });
                }

                // Type-A NPC Web
                575 => {
                    assert!(npc.ewar.webifier.is_none());
                    npc.ewar.webifier = Some(NPCEwarWeb {
                        strength: (100.0 + attribute_value(npc_dogma, 20)?) / 100.0,
                        range: attribute_value(npc_dogma, 514)?,
                        duration: attribute_value(npc_dogma, 513)? / 1000.0,
                        discharge: Discharge::Probability(attribute_value(npc_dogma, 512)?),
                    });
                },

                // Type-B NPC Web
                6743 => {
                    assert!(npc.ewar.webifier.is_none());
                    npc.ewar.webifier = Some(NPCEwarWeb {
                        strength: (100.0 + attribute_value(npc_dogma, 20)?) / 100.0,
                        range: attribute_value(npc_dogma, 2500)?,
                        duration: attribute_value(npc_dogma, 2499)? / 1000.0,
                        discharge: Discharge::Capacitor(attribute_value(npc_dogma, 2502)?)
                    });
                }

                // Type-A NPC Sensor Damp
                1878 => {
                    assert!(npc.ewar.sensor_damp.is_none());
                    npc.ewar.sensor_damp = Some(NPCEwarSensorDamp {
                        strength_range: (100.0 + attribute_value(npc_dogma, 237)?) / 100.0,
                        strength_resolution: (100.0 + attribute_value(npc_dogma, 565)?) / 100.0,
                        range: attribute_value(npc_dogma, 938)?,
                        falloff: attribute_value(npc_dogma, 950)?,
                        duration: attribute_value(npc_dogma, 943)? / 1000.0,
                        discharge: Discharge::Probability(attribute_value(npc_dogma, 932)?),
                    });
                }

                // Type-B NPC Sensor Damp
                6755 => {
                    assert!(npc.ewar.sensor_damp.is_none());
                    npc.ewar.sensor_damp = Some(NPCEwarSensorDamp {
                        strength_range: (100.0 + attribute_value(npc_dogma, 309)?) / 100.0,
                        strength_resolution: (100.0 + attribute_value(npc_dogma, 566)?) / 100.0,
                        range: attribute_value(npc_dogma, 2528)?,
                        falloff: attribute_value(npc_dogma, 2529)?,
                        duration: attribute_value(npc_dogma, 2527)? / 1000.0,
                        discharge: Discharge::Capacitor(attribute_value(npc_dogma, 2530)?),
                    });
                }

                // Type-A NPC Target paint
                1879 => {
                    assert!(npc.ewar.target_paint.is_none());
                    npc.ewar.target_paint = Some(NPCEwarTargetPaint {
                        strength: (100.0 + attribute_value(npc_dogma, 554)?) / 100.0,
                        range: attribute_value(npc_dogma, 941)?,
                        falloff: attribute_value(npc_dogma, 954)?,
                        duration: attribute_value(npc_dogma, 945)? / 1000.0,
                        discharge: Discharge::Probability(attribute_value(npc_dogma, 935)?),
                    });
                }

                // Type-B NPC Target paint
                6754 => {
                    assert!(npc.ewar.target_paint.is_none());
                    npc.ewar.target_paint = Some(NPCEwarTargetPaint {
                        strength: (100.0 + attribute_value(npc_dogma, 554)?) / 100.0,
                        range: attribute_value(npc_dogma, 2524)?,
                        falloff: attribute_value(npc_dogma, 2525)?,
                        duration: attribute_value(npc_dogma, 2523)? / 1000.0,
                        discharge: Discharge::Capacitor(attribute_value(npc_dogma, 2526)?),
                    });
                }

                // NPC Guidance Disruptor
                6746 => {
                    assert!(npc.ewar.guidance_disrupt.is_none());
                    npc.ewar.guidance_disrupt = Some(NPCEwarGuidanceDisrupt {
                        strength_explosion_velocity: (100.0 + attribute_value(npc_dogma, 847)?) / 100.0,
                        strength_explosion_radius: (100.0 + attribute_value(npc_dogma, 848)?) / 100.0,
                        strength_flighttime: (100.0 + attribute_value(npc_dogma, 596)?) / 100.0,
                        strength_missile_velocity: (100.0 + attribute_value(npc_dogma, 547)?) / 100.0,
                        range: attribute_value(npc_dogma, 2512)?,
                        falloff: attribute_value(npc_dogma, 2513)?,
                        duration: attribute_value(npc_dogma, 2511)? / 1000.0,
                        discharge: Discharge::Capacitor(attribute_value(npc_dogma, 2514)?),
                    });
                }

                // Type-A NPC Tracking Disruptor
                6846 => {
                    assert!(npc.ewar.tracking_disrupt.is_none());
                    npc.ewar.tracking_disrupt = Some(NPCEwarTrackingDisrupt {
                        strength_tracking: (100.0 + attribute_value(npc_dogma, 767)?) / 100.0,
                        strength_optimal: (100.0 + attribute_value(npc_dogma, 351)?) / 100.0,
                        strength_falloff: (100.0 + attribute_value(npc_dogma, 349)?) / 100.0,
                        range: attribute_value(npc_dogma, 2516)?,
                        falloff: 0.0,
                        duration: attribute_value(npc_dogma, 2515)? / 1000.0,
                        discharge: Discharge::Probability(attribute_value(npc_dogma, 933)?),
                    });
                }

                // Type-B NPC Tracking Disruptor
                6747 => {
                    assert!(npc.ewar.tracking_disrupt.is_none());
                    npc.ewar.tracking_disrupt = Some(NPCEwarTrackingDisrupt {
                        strength_tracking: (100.0 + attribute_value(npc_dogma, 767)?) / 100.0,
                        strength_optimal: (100.0 + attribute_value(npc_dogma, 351)?) / 100.0,
                        strength_falloff: (100.0 + attribute_value(npc_dogma, 349)?) / 100.0,
                        range: attribute_value(npc_dogma, 2516)?,
                        falloff: attribute_value(npc_dogma, 2517)?,
                        duration: attribute_value(npc_dogma, 2515)? / 1000.0,
                        discharge: Discharge::Capacitor(attribute_value(npc_dogma, 2518)?),
                    });
                }

                // Type-A NPC Neut
                6691 => {
                    assert!(npc.ewar.neutralize.is_none());
                    npc.ewar.neutralize = Some(NPCEwarNeut {
                        amount: attribute_value(npc_dogma, 97)?,
                        range: attribute_value(npc_dogma, 98)?,
                        falloff: 0.0,
                        interval: attribute_value(npc_dogma, 942)? / 1000.0,
                        discharge: Discharge::Probability(attribute_value(npc_dogma, 931)?),
                        is_nosferatu: false,
                    });
                }

                // Type-B NPC Neut
                6756 => {
                    assert!(npc.ewar.neutralize.is_none());
                    npc.ewar.neutralize = Some(NPCEwarNeut {
                        amount: attribute_value(npc_dogma, 97)?,
                        range: attribute_value(npc_dogma, 2520)?,
                        falloff: attribute_value(npc_dogma, 2521)?,
                        interval: attribute_value(npc_dogma, 2519)? / 1000.0,
                        discharge: Discharge::Capacitor(attribute_value(npc_dogma, 2522)?),
                        is_nosferatu: false,
                    });
                }

                // Type-C NPC Neut
                6187 => {
                    assert!(npc.ewar.neutralize.is_none());
                    // TODO: This might be scuffed as the effect declares old attributes but NPC has new attributes set?!
                    // This effect exists as broken data, so only apply to NPCs with the attributes set
                    if npc_dogma.dogmaAttributes.contains_key(&97) {
                        npc.ewar.neutralize = Some(NPCEwarNeut {
                            amount: attribute_value(npc_dogma, 97)?,
                            range: attribute_value(npc_dogma, 54)?,
                            falloff: attribute_value(npc_dogma, 2044)?,
                            interval: attribute_value(npc_dogma, 73)? / 1000.0,
                            discharge: Discharge::Capacitor(attribute_value(npc_dogma, 6)?),
                            is_nosferatu: false,
                        });
                    }
                }

                // NPC Nosferatu
                6882 => {
                    assert!(npc.ewar.neutralize.is_none());
                    npc.ewar.neutralize = Some(NPCEwarNeut {
                        amount: attribute_value(npc_dogma, 90)?,
                        range: attribute_value(npc_dogma, 2632)?,
                        falloff: attribute_value(npc_dogma, 2631)?,
                        interval: attribute_value(npc_dogma, 2630)? / 1000.0,
                        discharge: Discharge::Capacitor(attribute_value(npc_dogma, 2629)?),
                        is_nosferatu: true,
                    });
                }

                // Type-A NPC ECM
                6695 => {
                    assert!(npc.ewar.ecm.is_none());
                    npc.ewar.ecm = Some(NPCEwarECM {
                        strength_radar: attribute_value(npc_dogma, 241)?,
                        strength_magnetometric: attribute_value(npc_dogma, 240)?,
                        strength_gravimetric: attribute_value(npc_dogma, 238)?,
                        strength_ladar: attribute_value(npc_dogma, 239)?,
                        range: attribute_value(npc_dogma, 936)?,
                        falloff: 0.0,
                        duration: attribute_value(npc_dogma, 929)?  / 1000.0,
                        discharge: Discharge::Probability(attribute_value(npc_dogma, 930)?),
                    });
                }

                // Type-B NPC ECM
                6757 => {
                    if npc.ewar.ecm.is_some() {
                        continue;   // Type-A overrides Type-B, some NPCs have both set
                    }
                    npc.ewar.ecm = Some(NPCEwarECM {
                        strength_radar: attribute_value(npc_dogma, 241)?,
                        strength_magnetometric: attribute_value(npc_dogma, 240)?,
                        strength_gravimetric: attribute_value(npc_dogma, 238)?,
                        strength_ladar: attribute_value(npc_dogma, 239)?,
                        range: attribute_value(npc_dogma, 2532)?,
                        falloff: attribute_value(npc_dogma, 2533)?,
                        duration: attribute_value(npc_dogma, 2531)?  / 1000.0,
                        discharge: Discharge::Capacitor(attribute_value(npc_dogma, 2534)?),
                    });
                }

                // NPC Micro-jump "boosh"
                7187 => {
                    assert!(npc.micro_jump.is_none());
                    npc.micro_jump = Some(NPCMicroJump {
                        distance: attribute_value(npc_dogma, 2818)?,
                        range: attribute_value(npc_dogma, 2816)?,
                        duration: attribute_value(npc_dogma, 2819)? / 1000.0,
                        discharge: Discharge::Capacitor(attribute_value(npc_dogma, 2815)?),
                    });
                }

                // NPC self velocity modifier
                5931 => { /* TODO: Just Burner Worm */ }

                // NPC self orbit velocity modifier
                5933 => { /* TODO: Just Burner Worm */ }

                // NPC MWD
                6864 => { /* TODO */ }

                // (Incursion) NPC Remote ECM Burst
                4656 => {
                    assert!(npc.ewar.remote_ecm_burst.is_none());
                    npc.ewar.remote_ecm_burst = Some(NPCEwarRemoteECM {
                        strength_radar: attribute_value(npc_dogma, 241)?,
                        strength_magnetometric: attribute_value(npc_dogma, 240)?,
                        strength_gravimetric: attribute_value(npc_dogma, 238)?,
                        strength_ladar: attribute_value(npc_dogma, 239)?,
                        duration: attribute_value(npc_dogma, 1658)?  / 1000.0,
                        discharge: Discharge::Probability(attribute_value(npc_dogma, 1664)?),
                        scaling_minimum: attribute_value(npc_dogma, 1659)?,
                        scaling_factor: attribute_value(npc_dogma, 1660)?,
                        scaling_stepsize: attribute_value(npc_dogma, 1662)?,
                        scaling_startsize: attribute_value(npc_dogma, 1663)?,
                    });
                }

                // (Incursion) NPC Group Shield Hardener
                4686 => {
                    assert!(npc.ewar.group_shield_harden.is_none());
                    npc.ewar.group_shield_harden = Some(NPCEwarGroupShieldHarden {
                        resistance_bonus: attribute_value(npc_dogma, 1671)?,
                        duration: attribute_value(npc_dogma, 1672)? / 1000.0,
                        discharge: Discharge::Probability(attribute_value(npc_dogma, 1673)?),
                    });
                }

                // NPC Mining
                effect_id @ (6901 | 11453) => {
                    assert!(npc.mining.is_none());
                    npc.mining = Some(NPCMining {
                        is_fake: effect_id == 11453,
                        amount: attribute_value(npc_dogma, 2489)?,
                        interval: attribute_value(npc_dogma, 2490)? / 1000.0,
                        range: attribute_value(npc_dogma, 2673)?,
                        discharge: Discharge::Capacitor(attribute_value(npc_dogma, 2474)?),
                    });
                }

                // NPC Siege (Dreadnought)
                6885 => {
                    assert!(npc.siege.is_none());
                    npc.siege = Some(NPCSiege::Dreadnought {
                        duration: attribute_value(npc_dogma, 2636)? / 1000.0,
                        discharge: Discharge::Capacitor(attribute_value(npc_dogma, 2637)?),
                        received_remote_repair_multiplier: (100.0 + attribute_value(npc_dogma, 2638)?) / 100.0,
                        received_remote_assist_multiplier: (100.0 + attribute_value(npc_dogma, 2639)?) / 100.0,
                        received_remote_sensor_damp_multiplier: (100.0 + attribute_value(npc_dogma, 2640)?) / 100.0,
                        remote_tracking_disrupt_multiplier: (100.0 + attribute_value(npc_dogma, 2641)?) / 100.0,
                        received_remote_ecm_multiplier: (100.0 + attribute_value(npc_dogma, 2642)?) / 100.0,
                        velocity_multiplier: (100.0 + attribute_value(npc_dogma, 2643)?) / 100.0,
                        mass_multiplier: attribute_value(npc_dogma, 2646)?,
                        warp_scramble: attribute_value(npc_dogma, 2644)?,
                        disallow_tether: attribute_value(npc_dogma, 2645)? != 0.0,
                        local_logistics_amount_multiplier: (100.0 + attribute_value(npc_dogma, 2647)?) / 100.0,
                        local_logistics_duration_multiplier: (100.0 + attribute_value(npc_dogma, 2648)?) / 100.0,
                        turret_damage_multiplier: (100.0 + attribute_value(npc_dogma, 2649)?) / 100.0,
                        missile_damage_multiplier: (100.0 + attribute_value(npc_dogma, 2630)?) / 100.0,
                    });
                }

                // NPC Siege (Industrial Core)
                12002 => {
                    assert!(npc.siege.is_none());
                    npc.siege = Some(NPCSiege::IndustrialCore {
                        duration: attribute_value(npc_dogma, 2636)? / 1000.0,
                        discharge: Discharge::Capacitor(attribute_value(npc_dogma, 2637)?),
                        velocity_multiplier: (100.0 + attribute_value(npc_dogma, 2643)?) / 100.0,
                        mass_multiplier: attribute_value(npc_dogma, 2646)?,
                        received_remote_repair_multiplier: (100.0 + attribute_value(npc_dogma, 2638)?) / 100.0,
                        received_remote_assist_multiplier: (100.0 + attribute_value(npc_dogma, 2639)?) / 100.0,
                        received_remote_sensor_damp_multiplier: (100.0 + attribute_value(npc_dogma, 2640)?) / 100.0,
                        received_remote_ecm_multiplier: (100.0 + attribute_value(npc_dogma, 2642)?) / 100.0,
                        local_logistics_amount_multiplier: (100.0 + attribute_value(npc_dogma, 2647)?) / 100.0,
                        local_logistics_duration_multiplier: (100.0 + attribute_value(npc_dogma, 2648)?) / 100.0,
                        sent_remote_repair_amount_multiplier: (100.0 + attribute_value(npc_dogma, 2647)?) / 100.0,
                        sent_remote_repair_duration_multiplier: (100.0 + attribute_value(npc_dogma, 2648)?) / 100.0,
                        sent_remote_repair_capacitor_multiplier: (100.0 + attribute_value(npc_dogma, 2648)?) / 100.0,
                        sent_remote_repair_range_multiplier: (100.0 + attribute_value(npc_dogma, 2604)?) / 100.0,
                    });
                }


                // Test-only & unused data is ignored
                // (TEST)
                2662 => { /* Only used by a test NPC, not implemented */ }
                // (Incursion) NPC Group Speed Boost
                4687 => { /* Only used by a test NPC, not implemented */ }
                // (Incursion) NPC Group Propulsion Jammer Boost
                4688 => { /* Only used by a test NPC, not implemented */ }
                // (Incursion) NPC Group Armor Hardener
                4689 => { /* Only used by a test NPC, not implemented */ }

                // TODO: Large Collidable Structure warp disrupt
                2481 => { /* */ }

                // CONCORD ships are by GM rules undefeatable, so info about them is not provided. The player ship is immediately immobilized and destroyed.
                // (CONCORD) NPC Drone Bandwidth Reduction
                3661 => { npc.is_concord = true; }
                // (CONCORD) ECM
                3710 => { npc.is_concord = true; }
                // (CONCORD) Scramble
                3713 => { npc.is_concord = true; }
                // (CONCORD) Web
                3714 => { npc.is_concord = true; }
                // (CONCORD) Sensor Damp
                1752 => { npc.is_concord = true; }

                // Other effects, no relevance to NPC data
                6752 => { /* 'Drifter Controlled' NPE flag; Do nothing */ }
                12022 => { /* Visual effect; Do nothing */ }
                12586 => { /* Warfare Link Visual effect; Do nothing */ }

                id => effects.push(id),
            }
        }

        serde_json::to_writer(&mut *out, &npc)?;
        writeln!(&mut *out)?;
    }

    attributes.retain(|id| !KNOWN_ATTRIBUTES.binary_search(&id).is_ok());
    let mut attributes = attributes.into_iter().collect::<Vec<_>>();
    attributes.sort();
    effects.sort();

    Ok((attributes, effects))
}