use crate::game::map::map_chunk::MapChunk;
use crate::game::map::terrain::default_terrain_id;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapMetadata {
    pub name: String,
    pub author: String,
    pub description: String,
    pub recommended_players: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartingPosition {
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceNode {
    pub x: u32,
    pub y: u32,
    pub resource_id: String,
    #[serde(default)]
    pub amount: Option<u32>,
}

#[derive(Asset, TypePath, Debug, Clone, Serialize, Deserialize)]
pub struct Map {
    pub metadata: MapMetadata,
    pub width: u32,
    pub height: u32,
    #[serde(default = "default_terrain_id")]
    pub default_terrain: String,
    pub chunks: Vec<MapChunk>,
    pub starting_positions: Vec<StartingPosition>,
    pub resource_nodes: Vec<ResourceNode>,
}
