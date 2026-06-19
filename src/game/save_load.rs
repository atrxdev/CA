use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path as StdPath;

use crate::game::buildings::{
    Building, Constructing, PowerSystem, ProductionQueue, TeamBuildingQueues,
};
use crate::game::combat::{AttackTarget, Weapon};
use crate::game::data::{ArmorType, Definitions, WarheadType};
use crate::game::economy::{Harvester, OreField, Refinery};
use crate::game::fog_of_war::{FogOfWar, VisibilityState, Vision};
use crate::game::game_state::AppState;
use crate::game::pathfinding::{Grid, Path};
use crate::game::selection::Selected;
use crate::game::units::{Owner, Unit};

pub struct SaveLoadPlugin;

impl Plugin for SaveLoadPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SaveRequest>()
            .add_message::<LoadRequest>()
            .add_systems(
                Update,
                keyboard_trigger_save_load.run_if(in_state(AppState::InGame)),
            )
            .add_systems(Update, save_game_system.run_if(in_state(AppState::InGame)))
            .add_systems(Update, load_game_system.run_if(in_state(AppState::InGame)));
    }
}

#[derive(Message, Default)]
pub struct SaveRequest;

#[derive(Message, Default)]
pub struct LoadRequest;

// Key bindings for Quick Save (F5) and Quick Load (F9)
fn keyboard_trigger_save_load(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut save_events: MessageWriter<SaveRequest>,
    mut load_events: MessageWriter<LoadRequest>,
) {
    if keyboard.just_pressed(KeyCode::F5) {
        save_events.write(SaveRequest);
    }
    if keyboard.just_pressed(KeyCode::F9) {
        load_events.write(LoadRequest);
    }
}

// Serializable representations

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SavedTransform {
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SavedUnit {
    pub health: f32,
    pub max_health: f32,
    pub speed: f32,
    pub unit_id: String,
    #[serde(default)]
    pub armor: Option<ArmorType>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SavedBuilding {
    pub building_id: String,
    #[serde(default)]
    pub health: f32,
    #[serde(default)]
    pub max_health: f32,
    #[serde(default)]
    pub armor: Option<ArmorType>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SavedWeapon {
    pub damage: f32,
    pub range: f32,
    pub cooldown: f32,
    pub timer: f32,
    #[serde(default)]
    pub warhead: Option<WarheadType>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum SavedHarvesterState {
    Idle,
    SeekingNearestOre,
    MovingToOre(usize),
    Harvesting(usize),
    ReturningToRefinery(Option<usize>),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SavedHarvester {
    pub state: SavedHarvesterState,
    pub carrying_ore: u32,
    pub capacity: u32,
    pub timer: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SavedProductionQueue {
    pub queue: Vec<String>,
    pub progress: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SavedConstructing {
    pub timer: f32,
    pub duration: f32,
    pub target_scale: [f32; 3],
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SavedOreField {
    pub resource_id: String,
    pub amount: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SavedVision {
    pub range: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SavedEntity {
    pub id: usize,
    pub transform: SavedTransform,
    pub team: Option<usize>,
    pub vision: Option<SavedVision>,

    // Optional Components
    pub unit: Option<SavedUnit>,
    pub building: Option<SavedBuilding>,
    pub weapon: Option<SavedWeapon>,
    pub harvester: Option<SavedHarvester>,
    pub production_queue: Option<SavedProductionQueue>,
    pub constructing: Option<SavedConstructing>,
    pub ore_field: Option<SavedOreField>,

    // Flags
    pub is_refinery: bool,
    pub selected: bool,

    // References & Path finding
    pub attack_target_id: Option<usize>,
    pub path_waypoints: Option<Vec<[f32; 2]>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SavedQueueEntry {
    pub building_id: String,
    pub progress: f32,
    pub build_time: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SavedBuildingQueue {
    pub current: Option<SavedQueueEntry>,
    pub ready: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SavedGameState {
    pub player_credits: std::collections::HashMap<usize, u32>,
    pub power_produced: i32,
    pub power_consumed: i32,
    pub building_queues: Vec<(usize, SavedBuildingQueue)>,
    pub camera_transform: Option<SavedTransform>,
    pub entities: Vec<SavedEntity>,
    pub fog_width: i32,
    pub fog_height: i32,
    pub fog_states: Vec<VisibilityState>,
}

fn save_game_system(
    mut save_events: MessageReader<SaveRequest>,
    players: Res<crate::game::player::Players>,

    power_system: Res<PowerSystem>,
    team_queues: Res<TeamBuildingQueues>,
    fog_of_war: Res<FogOfWar>,
    q_camera: Query<&Transform, With<crate::game::camera::RtsCamera>>,
    q_units: Query<(
        Entity,
        &Transform,
        &Unit,
        &Owner,
        Option<&Weapon>,
        Option<&Vision>,
        Option<&Harvester>,
        Option<&AttackTarget>,
        Option<&Path>,
        Has<Selected>,
    )>,
    q_buildings: Query<(
        Entity,
        &Transform,
        &Building,
        &Owner,
        Option<&Vision>,
        Option<&ProductionQueue>,
        Option<&Constructing>,
        Has<Refinery>,
    )>,
    q_ore: Query<(Entity, &Transform, &OreField)>,
) {
    if save_events.read().next().is_none() {
        return;
    }

    println!("Saving game...");

    // Create unique serialization IDs for all relevant entities
    let mut entity_to_idx = HashMap::new();
    let mut current_idx = 0;

    for (entity, _, _, _, _, _, _, _, _, _) in q_units.iter() {
        entity_to_idx.insert(entity, current_idx);
        current_idx += 1;
    }
    for (entity, _, _, _, _, _, _, _) in q_buildings.iter() {
        entity_to_idx.insert(entity, current_idx);
        current_idx += 1;
    }
    for (entity, _, _) in q_ore.iter() {
        entity_to_idx.insert(entity, current_idx);
        current_idx += 1;
    }

    let mut saved_entities = Vec::new();

    // 1. Serialize Units
    for (
        entity,
        transform,
        unit,
        team,
        opt_weapon,
        opt_vision,
        opt_harvester,
        opt_attack_target,
        opt_path,
        is_selected,
    ) in q_units.iter()
    {
        let id = entity_to_idx[&entity];

        let saved_transform = SavedTransform {
            translation: transform.translation.to_array(),
            rotation: [
                transform.rotation.x,
                transform.rotation.y,
                transform.rotation.z,
                transform.rotation.w,
            ],
            scale: transform.scale.to_array(),
        };

        let saved_unit = SavedUnit {
            health: unit.health,
            max_health: unit.max_health,
            speed: unit.speed,
            unit_id: unit.unit_id.clone(),
            armor: Some(unit.armor),
        };

        let saved_weapon = opt_weapon.map(|w| SavedWeapon {
            damage: w.damage,
            range: w.range,
            cooldown: w.cooldown,
            timer: w.timer,
            warhead: Some(w.warhead),
        });

        let saved_vision = opt_vision.map(|v| SavedVision { range: v.range });

        let saved_harvester = opt_harvester.map(|h| {
            let state = match h.state {
                crate::game::economy::HarvesterState::Idle => SavedHarvesterState::Idle,
                crate::game::economy::HarvesterState::SeekingNearestOre => {
                    SavedHarvesterState::SeekingNearestOre
                }
                crate::game::economy::HarvesterState::MovingToOre(ore_ent) => {
                    SavedHarvesterState::MovingToOre(
                        entity_to_idx.get(&ore_ent).copied().unwrap_or(0),
                    )
                }
                crate::game::economy::HarvesterState::Harvesting(ore_ent) => {
                    SavedHarvesterState::Harvesting(
                        entity_to_idx.get(&ore_ent).copied().unwrap_or(0),
                    )
                }
                crate::game::economy::HarvesterState::ReturningToRefinery(opt_ore) => {
                    SavedHarvesterState::ReturningToRefinery(
                        opt_ore.and_then(|e| entity_to_idx.get(&e).copied()),
                    )
                }
            };
            SavedHarvester {
                state,
                carrying_ore: h.carrying_ore,
                capacity: h.capacity,
                timer: h.timer,
            }
        });

        let attack_target_id = opt_attack_target.and_then(|at| entity_to_idx.get(&at.0).copied());

        let path_waypoints = opt_path.map(|p| p.waypoints.iter().map(|wp| [wp.x, wp.y]).collect());

        saved_entities.push(SavedEntity {
            id,
            transform: saved_transform,
            team: Some(team.0),
            vision: saved_vision,
            unit: Some(saved_unit),
            building: None,
            weapon: saved_weapon,
            harvester: saved_harvester,
            production_queue: None,
            constructing: None,
            ore_field: None,
            is_refinery: false,
            selected: is_selected,
            attack_target_id,
            path_waypoints,
        });
    }

    // 2. Serialize Buildings
    for (
        entity,
        transform,
        building,
        team,
        opt_vision,
        opt_prod_queue,
        opt_constructing,
        is_refinery,
    ) in q_buildings.iter()
    {
        let id = entity_to_idx[&entity];

        let saved_transform = SavedTransform {
            translation: transform.translation.to_array(),
            rotation: [
                transform.rotation.x,
                transform.rotation.y,
                transform.rotation.z,
                transform.rotation.w,
            ],
            scale: transform.scale.to_array(),
        };

        let saved_building = SavedBuilding {
            building_id: building.building_id.clone(),
            health: building.health,
            max_health: building.max_health,
            armor: Some(building.armor),
        };

        let saved_vision = opt_vision.map(|v| SavedVision { range: v.range });

        let saved_prod_queue = opt_prod_queue.map(|pq| SavedProductionQueue {
            queue: pq.queue.clone(),
            progress: pq.progress,
        });

        let saved_constructing = opt_constructing.map(|c| SavedConstructing {
            timer: c.timer,
            duration: c.duration,
            target_scale: c.target_scale.to_array(),
        });

        saved_entities.push(SavedEntity {
            id,
            transform: saved_transform,
            team: Some(team.0),
            vision: saved_vision,
            unit: None,
            building: Some(saved_building),
            weapon: None,
            harvester: None,
            production_queue: saved_prod_queue,
            constructing: saved_constructing,
            ore_field: None,
            is_refinery,
            selected: false,
            attack_target_id: None,
            path_waypoints: None,
        });
    }

    // 3. Serialize Ore Fields
    for (entity, transform, ore_field) in q_ore.iter() {
        let id = entity_to_idx[&entity];

        let saved_transform = SavedTransform {
            translation: transform.translation.to_array(),
            rotation: [
                transform.rotation.x,
                transform.rotation.y,
                transform.rotation.z,
                transform.rotation.w,
            ],
            scale: transform.scale.to_array(),
        };

        saved_entities.push(SavedEntity {
            id,
            transform: saved_transform,
            team: None,
            vision: None,
            unit: None,
            building: None,
            weapon: None,
            harvester: None,
            production_queue: None,
            constructing: None,
            ore_field: Some(SavedOreField {
                resource_id: ore_field.resource_id.clone(),
                amount: ore_field.amount,
            }),
            is_refinery: false,
            selected: false,
            attack_target_id: None,
            path_waypoints: None,
        });
    }

    // Camera transform
    let camera_transform = q_camera.iter().next().map(|t| SavedTransform {
        translation: t.translation.to_array(),
        rotation: [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w],
        scale: t.scale.to_array(),
    });

    // Building queues
    let mut building_queues = Vec::new();
    for (&team_id, q) in team_queues.0.iter() {
        let current = q.current.as_ref().map(|c| SavedQueueEntry {
            building_id: c.building_id.clone(),
            progress: c.progress,
            build_time: c.build_time,
        });
        building_queues.push((
            team_id,
            SavedBuildingQueue {
                current,
                ready: q.ready.clone(),
            },
        ));
    }

    let mut all_credits = std::collections::HashMap::new();
    for (&id, p) in &players.players {
        all_credits.insert(id, p.credits);
    }

    let saved_state = SavedGameState {
        player_credits: all_credits,
        power_produced: power_system.produced,
        power_consumed: power_system.consumed,
        building_queues,
        camera_transform,
        entities: saved_entities,
        fog_width: fog_of_war.width,
        fog_height: fog_of_war.height,
        fog_states: fog_of_war.states.clone(),
    };

    // Save to file
    fs::create_dir_all("data/saves").unwrap_or_default();
    match serde_json::to_string_pretty(&saved_state) {
        Ok(json) => {
            if let Err(e) = fs::write("data/saves/save.json", json) {
                eprintln!("Failed to write save file: {}", e);
            } else {
                println!("Game saved successfully to data/saves/save.json");
            }
        }
        Err(e) => {
            eprintln!("Failed to serialize game state: {}", e);
        }
    }
}

fn load_game_system(
    mut load_events: MessageReader<LoadRequest>,
    mut commands: Commands,
    definitions: Res<Definitions>,
    mut players: ResMut<crate::game::player::Players>,

    mut power_system: ResMut<PowerSystem>,
    mut team_queues: ResMut<TeamBuildingQueues>,
    mut fog_of_war: ResMut<FogOfWar>,
    mut grid: ResMut<Grid>,
    mut q_camera: Query<&mut Transform, With<crate::game::camera::RtsCamera>>,
    // Combined Despawn query to stay under Bevy's 16 system parameters limit
    q_despawn: Query<
        Entity,
        Or<(
            With<Unit>,
            With<Building>,
            With<OreField>,
            With<crate::game::combat::Projectile>,
            With<crate::game::combat::Explosion>,
        )>,
    >,
) {
    if load_events.read().next().is_none() {
        return;
    }

    let save_path = StdPath::new("data/saves/save.json");
    if !save_path.exists() {
        println!("No save file found at data/saves/save.json");
        return;
    }

    println!("Loading game...");

    let content = match fs::read_to_string(save_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to read save file: {}", e);
            return;
        }
    };

    let saved: SavedGameState = match serde_json::from_str(&content) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to parse save file: {}", e);
            return;
        }
    };

    // 1. Despawn existing gameplay entities
    for entity in q_despawn.iter() {
        commands.entity(entity).despawn();
    }

    // 2. Restore resources
    for (id, credits) in saved.player_credits {
        players
            .players
            .entry(id)
            .and_modify(|p| p.credits = credits);
    }
    power_system.produced = saved.power_produced;
    power_system.consumed = saved.power_consumed;

    team_queues.0.clear();
    for (team_id, q) in &saved.building_queues {
        let current = q
            .current
            .as_ref()
            .map(|c| crate::game::buildings::BuildingQueueEntry {
                building_id: c.building_id.clone(),
                progress: c.progress,
                build_time: c.build_time,
            });
        team_queues.0.insert(
            (*team_id).into(),
            crate::game::buildings::BuildingQueue {
                current,
                ready: q.ready.clone(),
            },
        );
    }

    // Reset Fog of war state
    fog_of_war.states = saved.fog_states.clone();

    // Reset A* pathfinding grid and static map obstacles
    grid.static_blocked.fill(false);
    grid.blocked.fill(false);

    // 3. Restore camera
    if let Some(saved_cam) = &saved.camera_transform {
        if let Some(mut cam_transform) = q_camera.iter_mut().next() {
            cam_transform.translation = Vec3::from_array(saved_cam.translation);
            cam_transform.rotation = Quat::from_xyzw(
                saved_cam.rotation[0],
                saved_cam.rotation[1],
                saved_cam.rotation[2],
                saved_cam.rotation[3],
            );
            cam_transform.scale = Vec3::from_array(saved_cam.scale);
        }
    }

    // 4. Spawn entities
    let mut entity_id_map = HashMap::new();

    for saved_entity in &saved.entities {
        let pos = Vec3::from_array(saved_entity.transform.translation);
        let rot = Quat::from_xyzw(
            saved_entity.transform.rotation[0],
            saved_entity.transform.rotation[1],
            saved_entity.transform.rotation[2],
            saved_entity.transform.rotation[3],
        );
        let scale = Vec3::from_array(saved_entity.transform.scale);

        let transform = Transform {
            translation: pos,
            rotation: rot,
            scale,
        };

        if let Some(saved_unit) = &saved_entity.unit {
            let team_component = Owner(saved_entity.team.unwrap_or(0));

            let unit_transform = Transform {
                translation: pos,
                rotation: rot,
                scale,
            };

            let mut entity_cmds = commands.spawn((
                unit_transform,
                Unit {
                    health: saved_unit.health,
                    max_health: saved_unit.max_health,
                    speed: saved_unit.speed,
                    unit_id: saved_unit.unit_id.clone(),
                    armor: saved_unit.armor.unwrap_or_default(),
                },
            ));

            if let Some(_team_id) = saved_entity.team {
                entity_cmds.insert(team_component);
            }

            if let Some(vision) = &saved_entity.vision {
                entity_cmds.insert(Vision {
                    range: vision.range,
                });
            }

            if let Some(weapon) = &saved_entity.weapon {
                entity_cmds.insert(Weapon {
                    damage: weapon.damage,
                    range: weapon.range,
                    cooldown: weapon.cooldown,
                    timer: weapon.timer,
                    warhead: weapon.warhead.unwrap_or_default(),
                });
            }

            if saved_entity.selected {
                entity_cmds.insert(Selected);
            }

            entity_id_map.insert(saved_entity.id, entity_cmds.id());
        } else if let Some(saved_building) = &saved_entity.building {
            let (health, max_health) =
                if let Some(def) = definitions.buildings.get(&saved_building.building_id) {
                    let h = if saved_building.health > 0.0 {
                        saved_building.health
                    } else {
                        def.health
                    };
                    let mh = if saved_building.max_health > 0.0 {
                        saved_building.max_health
                    } else {
                        def.health
                    };
                    (h, mh)
                } else {
                    (saved_building.health, saved_building.max_health)
                };

            let armor = saved_building.armor.unwrap_or(ArmorType::Wood);

            let mut entity_cmds = commands.spawn((
                transform,
                Building {
                    building_id: saved_building.building_id.clone(),
                    health,
                    max_health,
                    armor,
                },
            ));

            if let Some(team_id) = saved_entity.team {
                entity_cmds.insert(Owner(team_id));
            }

            if let Some(vision) = &saved_entity.vision {
                entity_cmds.insert(Vision {
                    range: vision.range,
                });
            }

            if let Some(pq) = &saved_entity.production_queue {
                entity_cmds.insert(ProductionQueue {
                    queue: pq.queue.clone(),
                    progress: pq.progress,
                });
            }

            if let Some(constr) = &saved_entity.constructing {
                entity_cmds.insert(Constructing {
                    timer: constr.timer,
                    duration: constr.duration,
                    target_scale: Vec3::from_array(constr.target_scale),
                });
            }

            if saved_entity.is_refinery {
                entity_cmds.insert(Refinery);
            }

            // Block cells in the grid for this building
            if let Some(def) = definitions.buildings.get(&saved_building.building_id) {
                let size = def.size;
                let min_x = (pos.x - size.0 as f32 / 2.0 + 0.5).round() as i32;
                let min_z = (pos.z - size.1 as f32 / 2.0 + 0.5).round() as i32;
                for dz in 0..size.1 {
                    for dx in 0..size.0 {
                        grid.set_blocked(min_x + dx, min_z + dz, true);
                    }
                }
            }

            entity_id_map.insert(saved_entity.id, entity_cmds.id());
        } else if let Some(saved_ore) = &saved_entity.ore_field {
            let entity_cmds = commands.spawn((
                transform,
                OreField {
                    resource_id: saved_ore.resource_id.clone(),
                    amount: saved_ore.amount,
                },
            ));

            entity_id_map.insert(saved_entity.id, entity_cmds.id());
        }
    }

    // 5. Second pass to add entity-referencing components and path waypoints
    for saved_entity in &saved.entities {
        if let Some(&new_ent) = entity_id_map.get(&saved_entity.id) {
            // Attack Target
            if let Some(target_idx) = saved_entity.attack_target_id {
                if let Some(&target_ent) = entity_id_map.get(&target_idx) {
                    commands
                        .entity(new_ent)
                        .try_insert(AttackTarget(target_ent));
                }
            }

            // Path
            if let Some(wps) = &saved_entity.path_waypoints {
                let waypoints = wps.iter().map(|wp| Vec2::new(wp[0], wp[1])).collect();
                commands.entity(new_ent).try_insert(Path { waypoints });
            }

            // Harvester State
            if let Some(saved_harv) = &saved_entity.harvester {
                let state = match saved_harv.state {
                    SavedHarvesterState::Idle => crate::game::economy::HarvesterState::Idle,
                    SavedHarvesterState::SeekingNearestOre => {
                        crate::game::economy::HarvesterState::SeekingNearestOre
                    }
                    SavedHarvesterState::MovingToOre(idx) => {
                        if let Some(&ore_ent) = entity_id_map.get(&idx) {
                            crate::game::economy::HarvesterState::MovingToOre(ore_ent)
                        } else {
                            crate::game::economy::HarvesterState::Idle
                        }
                    }
                    SavedHarvesterState::Harvesting(idx) => {
                        if let Some(&ore_ent) = entity_id_map.get(&idx) {
                            crate::game::economy::HarvesterState::Harvesting(ore_ent)
                        } else {
                            crate::game::economy::HarvesterState::Idle
                        }
                    }
                    SavedHarvesterState::ReturningToRefinery(idx_opt) => {
                        let opt_ent = idx_opt.and_then(|idx| entity_id_map.get(&idx).copied());
                        crate::game::economy::HarvesterState::ReturningToRefinery(opt_ent)
                    }
                };

                commands.entity(new_ent).insert(Harvester {
                    state,
                    carrying_ore: saved_harv.carrying_ore,
                    capacity: saved_harv.capacity,
                    timer: saved_harv.timer,
                    path_retry_timer: 0.0,
                });
            }
        }
    }

    println!("Game loaded successfully from data/saves/save.json");
}
