use serde::{Deserialize, Serialize};

pub const DEFAULT_TERRAIN_ID: &str = "grass";

pub fn default_terrain_id() -> String {
    DEFAULT_TERRAIN_ID.to_string()
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TerrainDefinition {
    #[serde(skip)]
    pub id: String,
    pub name: String,
    pub color: [f32; 3],
    #[serde(default = "default_passable")]
    pub passable: bool,
    #[serde(default = "default_movement_cost")]
    pub movement_cost: f32,
    #[serde(default)]
    pub buildable: bool,
}

fn default_passable() -> bool {
    true
}

fn default_movement_cost() -> f32 {
    1.0
}
