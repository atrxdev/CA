use crate::game::game_state::{AppState, game_is_playing};
use crate::game::pathfinding::{Grid, Path, find_path};
use bevy::prelude::*;

use crate::game::player::Players;

pub struct EconomyPlugin;

impl Plugin for EconomyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            harvester_logic
                .run_if(game_is_playing)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

#[derive(Component)]
pub struct OreField {
    pub resource_id: String,
    pub amount: u32,
}

#[derive(Component)]
pub struct Refinery;

#[derive(Component)]
pub struct Harvester {
    pub state: HarvesterState,
    pub carrying_ore: u32,
    pub capacity: u32,
    pub timer: f32,
    /// Cooldown before retrying pathfinding after a failure.
    /// Prevents running A* every frame when the path is blocked.
    pub path_retry_timer: f32,
}

impl Default for Harvester {
    fn default() -> Self {
        Self {
            state: HarvesterState::Idle,
            carrying_ore: 0,
            capacity: 250,
            timer: 0.0,
            path_retry_timer: 0.0,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HarvesterState {
    Idle,
    SeekingNearestOre,
    MovingToOre(Entity),
    Harvesting(Entity),
    ReturningToRefinery(Option<Entity>),
}

use crate::game::units::Owner;

fn harvester_logic(
    mut commands: Commands,
    mut q_harvesters: Query<(
        Entity,
        &Transform,
        &mut Harvester,
        Option<&mut Path>,
        Option<&Owner>,
    )>,
    mut q_ore: Query<(Entity, &mut OreField, &Transform), Without<Harvester>>,
    q_refinery: Query<(Entity, &Transform, &Owner), With<Refinery>>,
    grid: Res<Grid>,
    time: Res<Time>,
    mut players: ResMut<Players>,
) {
    for (entity, transform, mut harvester, opt_path, opt_owner) in q_harvesters.iter_mut() {
        match harvester.state {
            HarvesterState::Idle => {}
            HarvesterState::SeekingNearestOre => {
                let mut closest_ore = None;
                let mut closest_dist = f32::MAX;
                for (ore_ent, _, ore_transform) in q_ore.iter() {
                    let dist = transform.translation.distance(ore_transform.translation);
                    if dist < closest_dist {
                        closest_dist = dist;
                        closest_ore = Some(ore_ent);
                    }
                }
                if let Some(ore_ent) = closest_ore {
                    harvester.state = HarvesterState::MovingToOre(ore_ent);
                } else {
                    harvester.state = HarvesterState::Idle;
                }
            }
            HarvesterState::MovingToOre(ore_entity) => {
                if let Ok((_, _, ore_transform)) = q_ore.get(ore_entity) {
                    let dist = transform.translation.distance(ore_transform.translation);
                    if dist < 2.5 {
                        if let Some(mut path) = opt_path {
                            path.waypoints.clear();
                        }
                        harvester.state = HarvesterState::Harvesting(ore_entity);
                        harvester.timer = 0.5;
                        harvester.path_retry_timer = 0.0;
                    } else {
                        // Tick down retry cooldown
                        harvester.path_retry_timer -= time.delta_secs();
                        let needs_path = opt_path.as_ref().map_or(true, |p| p.waypoints.is_empty());
                        if needs_path && harvester.path_retry_timer <= 0.0 {
                            let start_pos =
                                Vec2::new(transform.translation.x, transform.translation.z);
                            let end_pos =
                                Vec2::new(ore_transform.translation.x, ore_transform.translation.z);
                            if let Some(path_waypoints) = find_path(start_pos, end_pos, &grid) {
                                commands.entity(entity).try_insert(Path {
                                    waypoints: path_waypoints,
                                });
                                harvester.path_retry_timer = 0.0;
                            } else {
                                // Back off for 1 second before retrying
                                harvester.path_retry_timer = 1.0;
                                println!("Harvester could not find path to ore field!");
                            }
                        }
                    }
                } else {
                    harvester.state = HarvesterState::SeekingNearestOre;
                    harvester.path_retry_timer = 0.0;
                }
            }
            HarvesterState::Harvesting(ore_entity) => {
                if let Ok((_, mut ore, _)) = q_ore.get_mut(ore_entity) {
                    harvester.timer -= time.delta_secs();
                    if harvester.timer <= 0.0 {
                        let amount = 25.min(ore.amount);
                        ore.amount -= amount;
                        harvester.carrying_ore += amount;

                        if ore.amount == 0 {
                            if let Ok(mut cmds) = commands.get_entity(ore_entity) {
                                cmds.try_despawn();
                            }
                        }

                        if harvester.carrying_ore >= harvester.capacity {
                            harvester.state = HarvesterState::ReturningToRefinery(Some(ore_entity));
                        } else if ore.amount == 0 {
                            harvester.state = HarvesterState::SeekingNearestOre;
                            harvester.timer = 0.5;
                        } else {
                            harvester.timer = 0.5;
                        }
                    }
                } else {
                    harvester.state = HarvesterState::SeekingNearestOre;
                }
            }
            HarvesterState::ReturningToRefinery(opt_ore_entity) => {
                let mut closest_refinery = None;
                let mut closest_dist = f32::MAX;
                for (_, ref_transform, ref_owner) in q_refinery.iter() {
                    if let Some(harv_owner) = opt_owner {
                        if ref_owner.0 != harv_owner.0 {
                            continue;
                        }
                    }
                    let dist = transform.translation.distance(ref_transform.translation);
                    if dist < closest_dist {
                        closest_dist = dist;
                        closest_refinery = Some(ref_transform.translation);
                    }
                }

                if let Some(ref_pos) = closest_refinery {
                    if closest_dist < 3.5 {
                        if let Some(mut path) = opt_path {
                            path.waypoints.clear();
                        }

                        if let Some(owner) = opt_owner {
                            if let Some(player) = players.players.get_mut(&owner.0) {
                                player.credits += harvester.carrying_ore;
                            }
                        } else {
                            println!("Deposited! (No owner)");
                        }
                        harvester.carrying_ore = 0;

                        if let Some(ore_entity) = opt_ore_entity {
                            harvester.state = HarvesterState::MovingToOre(ore_entity);
                        } else {
                            harvester.state = HarvesterState::SeekingNearestOre;
                        }
                    } else {
                        harvester.path_retry_timer -= time.delta_secs();
                        let needs_path = opt_path.as_ref().map_or(true, |p| p.waypoints.is_empty());
                        if needs_path && harvester.path_retry_timer <= 0.0 {
                            let start_pos =
                                Vec2::new(transform.translation.x, transform.translation.z);
                            let end_pos = Vec2::new(ref_pos.x, ref_pos.z);
                            if let Some(path_waypoints) = find_path(start_pos, end_pos, &grid) {
                                commands.entity(entity).try_insert(Path {
                                    waypoints: path_waypoints,
                                });
                                harvester.path_retry_timer = 0.0;
                            } else {
                                harvester.path_retry_timer = 1.0;
                                println!("Harvester could not find path to refinery!");
                            }
                        }
                    }
                } else {
                    harvester.state = HarvesterState::Idle;
                }
            }
        }
    }
}
