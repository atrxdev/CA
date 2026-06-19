use crate::game::map::map_asset::Map;
use bevy::prelude::*;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct MapInfo {
    pub name: String,
    pub description: String,
    pub file_name: String,
    pub recommended_players: usize,
}

#[derive(Resource, Default)]
pub struct MapRegistry {
    pub maps: Vec<MapInfo>,
}

impl MapRegistry {
    pub fn scan(&mut self) {
        let maps_dir = Path::new("assets/maps");
        self.maps.clear();

        if let Ok(entries) = fs::read_dir(maps_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("ron") {
                    if let Ok(contents) = fs::read_to_string(&path) {
                        if let Ok(map_data) = ron::from_str::<Map>(&contents) {
                            let file_name = path.file_name().unwrap().to_string_lossy().to_string();
                            self.maps.push(MapInfo {
                                name: map_data.metadata.name,
                                description: map_data.metadata.description,
                                file_name,
                                recommended_players: map_data.metadata.recommended_players,
                            });
                        } else {
                            warn!("Failed to parse map file: {:?}", path);
                        }
                    }
                }
            }
        } else {
            warn!("Could not read maps directory: {:?}", maps_dir);
        }
    }
}
