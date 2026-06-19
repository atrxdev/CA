use crate::game::map::terrain::TerrainDefinition;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

/// All 11 armor types from RA2.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ArmorType {
    /// Unarmored infantry (Conscripts, GIs, Engineers)
    #[default]
    None,
    /// Lightly armored soldiers (Flak Troopers, Rocketeers, Tanya)
    Flak,
    /// Heavily armored infantry (Tesla Troopers, Chrono Legionnaires)
    Plate,
    /// Fast / lightly plated vehicles and aircraft (IFVs, Harriers)
    Light,
    /// Standard armored vehicles (Grizzly Tanks, Transports)
    Medium,
    /// Massive durable vehicles (Apocalypse Tanks, MCVs)
    Heavy,
    /// Basic / standard buildings (Power Plants, Barracks, War Factories)
    Wood,
    /// Base defenses (Pillboxes, Prism Towers, Tesla Coils)
    Steel,
    /// Critical infrastructure (Construction Yards, Superweapons)
    Concrete,
    /// Terror Drones
    Special1,
    /// Destructible projectiles (V3 Rockets, Dreadnought Missiles)
    Special2,
}

/// Warhead (damage) types from RA2.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum WarheadType {
    /// Small Arms – machine guns, rifles
    #[default]
    SA,
    /// Armor Piercing – tank cannons
    AP,
    /// High Explosive – artillery, rockets, grenades
    HE,
    /// Fire – desolator effects, flames
    Fire,
    /// Electric – Tesla weapons
    Electric,
    /// Hollow Point – sniper weapons
    HollowPoint,
    /// Super – chrono legionnaire, special weapons (100% vs everything)
    Super,
}

/// Hardcoded RA2-style Verses table: `damage_modifier = verses[warhead][armor]`.
#[derive(Resource)]
pub struct WarheadTable {
    /// Indexed by (WarheadType, ArmorType) → f32 multiplier.
    modifiers: HashMap<(WarheadType, ArmorType), f32>,
}

impl Default for WarheadTable {
    fn default() -> Self {
        Self::new()
    }
}

impl WarheadTable {
    pub fn new() -> Self {
        use ArmorType::*;
        use WarheadType::*;

        let mut m = HashMap::new();

        // Helper to insert a full row for one warhead type.
        let mut row = |wh: WarheadType, values: [f32; 11]| {
            let armors = [
                None, Flak, Plate, Light, Medium, Heavy, Wood, Steel, Concrete, Special1, Special2,
            ];
            for (armor, &val) in armors.iter().zip(values.iter()) {
                m.insert((wh, *armor), val);
            }
        };

        //                     None  Flak  Plate Light Med   Heavy Wood  Steel Conc  Sp1   Sp2
        row(
            SA,
            [1.0, 1.0, 1.0, 0.02, 0.02, 0.02, 0.02, 0.02, 0.02, 0.5, 0.0],
        );
        row(
            AP,
            [0.25, 0.5, 0.75, 1.0, 0.45, 1.0, 0.5, 0.4, 0.3, 0.6, 0.0],
        );
        row(
            HE,
            [1.5, 1.0, 0.5, 0.6, 0.1, 0.1, 0.25, 0.2, 0.15, 0.8, 0.0],
        );
        row(
            Fire,
            [1.5, 1.0, 0.0, 0.5, 0.25, 0.1, 0.75, 0.5, 0.25, 0.0, 0.0],
        );
        row(
            Electric,
            [1.0, 1.0, 0.5, 0.8, 0.8, 0.6, 0.5, 0.4, 0.3, 0.8, 0.0],
        );
        row(
            HollowPoint,
            [2.0, 1.0, 1.0, 0.01, 0.01, 0.01, 0.01, 0.01, 0.01, 0.5, 0.0],
        );
        row(
            Super,
            [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        );

        Self { modifiers: m }
    }

    /// Look up the damage multiplier for a warhead hitting a given armor type.
    /// Returns 1.0 if the combination is missing (shouldn't happen with a complete table).
    pub fn get_modifier(&self, warhead: WarheadType, armor: ArmorType) -> f32 {
        self.modifiers
            .get(&(warhead, armor))
            .copied()
            .unwrap_or(1.0)
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct WeaponDef {
    pub damage: f32,
    pub range: f32,
    pub cooldown: f32,
    #[serde(default)]
    pub warhead: Option<WarheadType>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct UnitDefinition {
    #[serde(skip)]
    pub id: String,
    pub name: String,
    pub health: f32,
    pub speed: f32,
    pub cost: u32,
    pub build_time: f32,
    pub produced_by: Vec<String>,
    pub weapon: Option<WeaponDef>,
    pub color: [f32; 3],
    pub requires: Option<Vec<String>>,
    pub sight_radius: f32,
    pub role: Option<String>,
    #[serde(default)]
    pub armor: Option<ArmorType>,
    pub model_path: Option<String>,
    pub model_scale: Option<f32>,
    pub model_scale_y: Option<f32>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct BuildingDefinition {
    #[serde(skip)]
    pub id: String,
    pub name: String,
    #[serde(default = "default_building_health")]
    pub health: f32,
    pub cost: u32,
    pub build_time: f32,
    pub size: (i32, i32),
    pub power_produced: i32,
    pub power_consumed: i32,
    pub color: [f32; 3],
    pub requires: Option<Vec<String>>,
    pub sight_radius: f32,
    pub role: Option<String>,
    pub model_path: Option<String>,
    pub model_scale: Option<f32>,
    pub model_scale_y: Option<f32>,
    #[serde(default = "default_building_armor")]
    pub armor: Option<ArmorType>,
    #[serde(default = "default_influence_radius")]
    pub influence_radius: u32,
}

fn default_building_health() -> f32 {
    1000.0
}

fn default_building_armor() -> Option<ArmorType> {
    Some(ArmorType::Wood)
}

fn default_influence_radius() -> u32 {
    4
}

#[derive(Deserialize, Debug, Clone)]
pub struct FactionDefinition {
    #[serde(skip)]
    pub id: String,
    pub name: String,
    pub buildings: Vec<String>,
    pub units: Vec<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ResourceDefinition {
    #[serde(skip)]
    pub id: String,
    pub name: String,
    pub amount: u32,
    pub color: [f32; 3],
    pub model_path: Option<String>,
    pub model_scale: Option<f32>,
}

#[derive(Resource, Default)]
pub struct Definitions {
    pub units: HashMap<String, UnitDefinition>,
    pub buildings: HashMap<String, BuildingDefinition>,
    pub factions: HashMap<String, FactionDefinition>,
    pub resources: HashMap<String, ResourceDefinition>,
    pub terrain: HashMap<String, TerrainDefinition>,
}

pub struct DataPlugin;

impl Plugin for DataPlugin {
    fn build(&self, app: &mut App) {
        let mut defs = Definitions::default();

        // Load Buildings
        if let Ok(entries) = fs::read_dir("assets/definitions/buildings") {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "json") {
                    let id = path.file_stem().unwrap().to_str().unwrap().to_string();
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(mut def) = serde_json::from_str::<BuildingDefinition>(&content) {
                            def.id = id.clone();
                            defs.buildings.insert(id, def);
                        } else {
                            eprintln!("Failed to parse building definition: {:?}", path);
                        }
                    }
                }
            }
        }

        // Load Units
        if let Ok(entries) = fs::read_dir("assets/definitions/units") {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "json") {
                    let id = path.file_stem().unwrap().to_str().unwrap().to_string();
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(mut def) = serde_json::from_str::<UnitDefinition>(&content) {
                            def.id = id.clone();
                            defs.units.insert(id, def);
                        } else {
                            eprintln!("Failed to parse unit definition: {:?}", path);
                        }
                    }
                }
            }
        }

        // Load Factions
        if let Ok(entries) = fs::read_dir("assets/definitions/factions") {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "json") {
                    let id = path.file_stem().unwrap().to_str().unwrap().to_string();
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(mut def) = serde_json::from_str::<FactionDefinition>(&content) {
                            def.id = id.clone();
                            defs.factions.insert(id, def);
                        } else {
                            eprintln!("Failed to parse faction definition: {:?}", path);
                        }
                    }
                }
            }
        }

        // Load Resources
        if let Ok(entries) = fs::read_dir("assets/definitions/resources") {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "json") {
                    let id = path.file_stem().unwrap().to_str().unwrap().to_string();
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(mut def) = serde_json::from_str::<ResourceDefinition>(&content) {
                            def.id = id.clone();
                            defs.resources.insert(id, def);
                        } else {
                            eprintln!("Failed to parse resource definition: {:?}", path);
                        }
                    }
                }
            }
        }

        // Load Terrain
        if let Ok(entries) = fs::read_dir("assets/definitions/terrain") {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "json") {
                    let id = path.file_stem().unwrap().to_str().unwrap().to_string();
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(mut def) = serde_json::from_str::<TerrainDefinition>(&content) {
                            def.id = id.clone();
                            defs.terrain.insert(id, def);
                        } else {
                            eprintln!("Failed to parse terrain definition: {:?}", path);
                        }
                    }
                }
            }
        }

        app.insert_resource(defs);
        app.insert_resource(WarheadTable::new());
    }
}
