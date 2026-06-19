use crate::game::data::ArmorType;
use crate::game::data::Definitions;
use crate::game::game_state::{AppState, game_is_playing};
use crate::game::player::Players;
use crate::game::units::{Owner, spawn_unit_of_type};
use bevy::prelude::*;

pub struct BuildingsPlugin;

impl Plugin for BuildingsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PowerSystem {
            produced: 0,
            consumed: 0,
        })
        .insert_resource(TeamBuildingQueues::default())
        .add_systems(
            Update,
            (
                tick_building_queue,
                construction_animation,
                process_production_queues,
            )
                .run_if(game_is_playing)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

#[derive(Component)]
pub struct Building {
    pub building_id: String,
    pub health: f32,
    pub max_health: f32,
    pub armor: ArmorType,
}

#[derive(Component, Default)]
pub struct ProductionQueue {
    pub queue: Vec<String>,
    pub progress: f32,
}

#[derive(Component)]
pub struct RallyPoint(pub Vec2);

#[derive(Component)]
pub struct Constructing {
    pub timer: f32,
    pub duration: f32,
    pub target_scale: Vec3,
}

#[derive(Resource)]
pub struct PowerSystem {
    pub produced: i32,
    pub consumed: i32,
}

#[derive(Clone)]
pub struct BuildingQueueEntry {
    pub building_id: String,
    pub progress: f32,
    pub build_time: f32,
}

#[derive(Default, Clone)]
pub struct BuildingQueue {
    pub current: Option<BuildingQueueEntry>,
    pub ready: Option<String>,
}

#[derive(Resource, Default)]
pub struct TeamBuildingQueues(pub std::collections::HashMap<usize, BuildingQueue>);

fn tick_building_queue(mut queues: ResMut<TeamBuildingQueues>, time: Res<Time>) {
    for (_, queue) in queues.0.iter_mut() {
        if let Some(ref mut entry) = queue.current {
            entry.progress += time.delta_secs();
            if entry.progress >= entry.build_time {
                let id = entry.building_id.clone();
                queue.current = None;
                queue.ready = Some(id);
            }
        }
    }
}

fn construction_animation(
    mut commands: Commands,
    mut q_constructing: Query<(Entity, &mut Transform, &mut Constructing, &Building, &Owner)>,
    time: Res<Time>,
    definitions: Res<Definitions>,
    players: Res<Players>,
) {
    for (entity, mut transform, mut constr, building, owner) in q_constructing.iter_mut() {
        constr.timer += time.delta_secs();
        let t = (constr.timer / constr.duration).min(1.0);

        let mut current_scale = constr.target_scale * t;
        current_scale.y = current_scale.y.max(0.1 * constr.target_scale.y);
        transform.scale = current_scale;

        if t >= 1.0 {
            commands.entity(entity).try_remove::<Constructing>();

            let Some(building_def) = definitions.buildings.get(&building.building_id) else {
                continue;
            };

            if building_def.role.as_deref() == Some("barracks")
                || building_def.role.as_deref() == Some("war_factory")
            {
                commands.entity(entity).insert(ProductionQueue::default());
                let spawn_x = transform.translation.x + building_def.size.0 as f32 / 2.0 + 1.0;
                commands.entity(entity).insert(RallyPoint(Vec2::new(
                    spawn_x + 2.0,
                    transform.translation.z,
                )));
            } else if building_def.role.as_deref() == Some("refinery") {
                commands
                    .entity(entity)
                    .insert(crate::game::economy::Refinery);

                // Spawn a harvester for the refinery dynamically
                let faction_id = players
                    .players
                    .get(&owner.0)
                    .map(|p| p.faction.clone())
                    .unwrap_or_else(|| "alliance".to_string());
                if let Some(faction) = definitions.factions.get(&faction_id) {
                    if let Some(harv_id) = faction.units.iter().find(|u| {
                        definitions
                            .units
                            .get(*u)
                            .map_or(false, |def| def.role.as_deref() == Some("harvester"))
                    }) {
                        if let Some(_harv_def) = definitions.units.get(harv_id) {
                            let spawn_pos = transform.translation + Vec3::new(0.0, 0.0, 3.0);
                            if let Some(harv_ent) = spawn_unit_of_type(
                                &mut commands,
                                harv_id.clone(),
                                &definitions,
                                spawn_pos,
                                *owner,
                            ) {
                                commands
                                    .entity(harv_ent)
                                    .insert(crate::game::economy::Harvester {
                                        state:
                                            crate::game::economy::HarvesterState::SeekingNearestOre,
                                        ..default()
                                    });
                            }
                        }
                    }
                }
            }
        }
    }
}

fn process_production_queues(
    mut commands: Commands,
    mut q_queues: Query<(
        &mut ProductionQueue,
        &Transform,
        &Building,
        &Owner,
        Option<&RallyPoint>,
    )>,
    time: Res<Time>,
    definitions: Res<Definitions>,
    grid: Res<crate::game::pathfinding::Grid>,
) {
    for (mut queue, transform, building, owner, opt_rally_point) in q_queues.iter_mut() {
        if queue.queue.is_empty() {
            continue;
        }

        let unit_id = &queue.queue[0];
        let Some(unit_def) = definitions.units.get(unit_id) else {
            queue.queue.remove(0);
            continue;
        };

        queue.progress += time.delta_secs();

        if queue.progress >= unit_def.build_time {
            queue.progress = 0.0;
            let completed_unit_id = queue.queue.remove(0);

            let building_def = definitions.buildings.get(&building.building_id).unwrap();
            let size = building_def.size;
            let spawn_pos = transform.translation + Vec3::new(size.0 as f32 / 2.0 + 1.0, 0.0, 0.0);

            let spawned_entity = spawn_unit_of_type(
                &mut commands,
                completed_unit_id,
                &definitions,
                spawn_pos,
                *owner,
            );

            if let Some(entity) = spawned_entity {
                if let Some(rally_point) = opt_rally_point {
                    let start_pos = Vec2::new(spawn_pos.x, spawn_pos.z);
                    if let Some(path) =
                        crate::game::pathfinding::find_path(start_pos, rally_point.0, &grid)
                    {
                        commands
                            .entity(entity)
                            .insert(crate::game::pathfinding::Path { waypoints: path });
                    }
                }
            }
        }
    }
}
