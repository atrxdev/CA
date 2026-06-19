use crate::game::map::terrain::default_terrain_id;
use serde::{Deserialize, Serialize};

pub const CHUNK_SIZE: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cell {
    #[serde(default)]
    pub x: Option<u32>,
    #[serde(default)]
    pub y: Option<u32>,
    #[serde(default = "default_terrain_id")]
    pub terrain: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapChunk {
    pub x: i32,
    pub y: i32,
    pub cells: Vec<Cell>,
}
