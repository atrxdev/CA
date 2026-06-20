use crate::game::buildings::Building;
use crate::game::combat::Weapon;
use crate::game::data::{ArmorType, Definitions};
use crate::game::economy::Harvester;
use crate::game::fog_of_war::Vision;
use crate::game::game_state::AppState;
use crate::game::pathfinding::{Grid, Path, nearest_available_destination};
use crate::game::spatial_hash::{SpatialHash, rebuild_spatial_hash};
use bevy::prelude::*;
use std::collections::HashMap;

use crate::game::player::Players;

const UNIT_MIN_SEPARATION: f32 = 1.0;

pub struct UnitsPlugin;

impl Plugin for UnitsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SpatialHash::new())
            .init_resource::<MovementRepathCooldowns>()
            .init_resource::<MovementStuckTracking>()
            .add_systems(
                PreUpdate,
                rebuild_spatial_hash.run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    move_units,
                    resolve_unit_collisions.after(move_units),
                    resolve_obstacle_collisions_system.after(resolve_unit_collisions),
                    track_stuck_unit_moves.after(resolve_obstacle_collisions_system),
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

#[derive(Component)]
pub struct Unit {
    pub health: f32,
    pub max_health: f32,
    pub speed: f32,
    pub unit_id: String,
    pub armor: ArmorType,
}

#[derive(Default, Resource)]
struct MovementRepathCooldowns(HashMap<Entity, f32>);

#[derive(Default, Resource)]
struct MovementStuckTracking(HashMap<Entity, MovementProgress>);

struct MovementProgress {
    destination: Vec2,
    best_distance: f32,
    stalled_for: f32,
}

#[derive(Component, PartialEq, Eq, Clone, Copy, Hash)]
pub struct Owner(pub usize);

pub fn spawn_faction_base(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    definitions: &Definitions,
    grid: &mut Grid,
    players: &Players,
    _asset_server: &AssetServer,
    player_id: usize,
    base_pos: Vec3,
    spawn_offsets_infantry: Vec<Vec3>,
    spawn_offsets_tanks: Vec<Vec3>,
) {
    let Some(player) = players.players.get(&player_id) else {
        println!("Warning: player {} not found", player_id);
        return;
    };
    let faction_id = &player.faction;

    let Some(faction) = definitions.factions.get(faction_id) else {
        println!(
            "Warning: faction not found for player {}: {}",
            player_id, faction_id
        );
        return;
    };

    // Find construction yard building ID from the faction
    let cyard_id = faction
        .buildings
        .iter()
        .find(|b| {
            definitions.buildings.get(*b).map_or(false, |def| {
                def.role.as_deref() == Some("construction_yard")
            })
        })
        .cloned()
        .unwrap_or_else(|| "construction_yard".to_string());

    let Some(cyard_def) = definitions.buildings.get(&cyard_id) else {
        println!(
            "Warning: construction yard definition not found: {}",
            cyard_id
        );
        return;
    };

    let _cyard_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(cyard_def.color[0], cyard_def.color[1], cyard_def.color[2]),
        ..default()
    });

    let cyard_size = cyard_def.size;
    let min_x = (base_pos.x - cyard_size.0 as f32 / 2.0 + 0.5).round() as i32;
    let min_z = (base_pos.z - cyard_size.1 as f32 / 2.0 + 0.5).round() as i32;
    let snapped_x = min_x as f32 + cyard_size.0 as f32 / 2.0 - 0.5;
    let snapped_z = min_z as f32 + cyard_size.1 as f32 / 2.0 - 0.5;
    let snapped_base_pos = Vec3::new(snapped_x, base_pos.y, snapped_z);

    let mut entity_cmds = commands.spawn((
        Building {
            building_id: cyard_id,
            health: cyard_def.health,
            max_health: cyard_def.health,
            armor: cyard_def.armor.unwrap_or(ArmorType::Wood),
        },
        Owner(player_id),
        Vision {
            range: cyard_def.sight_radius,
        },
    ));

    if let Some(ref _model_path) = cyard_def.model_path {
        let scale_x = cyard_def.model_scale.unwrap_or(1.0);
        let scale_y = cyard_def.model_scale_y.unwrap_or(scale_x);
        let scale_z = cyard_def.model_scale.unwrap_or(1.0);
        entity_cmds.insert(
            Transform::from_translation(Vec3::new(snapped_base_pos.x, 0.0, snapped_base_pos.z))
                .with_scale(Vec3::new(scale_x, scale_y, scale_z)),
        );
    } else {
        entity_cmds.insert(
            Transform::from_translation(snapped_base_pos + Vec3::new(0.0, 0.5, 0.0))
                .with_scale(Vec3::new(cyard_size.0 as f32, 2.0, cyard_size.1 as f32)),
        );
    }

    // Block grid for construction yard
    for dz in 0..cyard_size.1 {
        for dx in 0..cyard_size.0 {
            grid.set_blocked(min_x + dx, min_z + dz, true);
        }
    }

    // Spawn starting infantry (role "infantry")
    if let Some(inf_id) = faction.units.iter().find(|u| {
        definitions
            .units
            .get(*u)
            .map_or(false, |def| def.role.as_deref() == Some("infantry"))
    }) {
        for offset in spawn_offsets_infantry {
            spawn_unit_of_type(
                commands,
                inf_id.clone(),
                definitions,
                snapped_base_pos + offset,
                Owner(player_id),
            );
        }
    }

    // Spawn starting tanks (role "tank")
    if let Some(tank_id) = faction.units.iter().find(|u| {
        definitions
            .units
            .get(*u)
            .map_or(false, |def| def.role.as_deref() == Some("tank"))
    }) {
        for offset in spawn_offsets_tanks {
            spawn_unit_of_type(
                commands,
                tank_id.clone(),
                definitions,
                snapped_base_pos + offset,
                Owner(player_id),
            );
        }
    }
}

pub fn spawn_unit_of_type(
    commands: &mut Commands,
    unit_id: String,
    definitions: &Definitions,
    position: Vec3,
    owner: Owner,
) -> Option<Entity> {
    let Some(def) = definitions.units.get(&unit_id) else {
        println!("Unknown unit id: {}", unit_id);
        return None;
    };

    let damage = def.weapon.as_ref().map_or(0.0, |w| w.damage);
    let range = def.weapon.as_ref().map_or(0.0, |w| w.range);
    let cooldown = def.weapon.as_ref().map_or(1.0, |w| w.cooldown);
    let warhead = def
        .weapon
        .as_ref()
        .and_then(|w| w.warhead)
        .unwrap_or_default();
    let armor = def.armor.unwrap_or_default();

    let mut entity_cmds = commands.spawn((
        Transform::from_translation(position),
        Unit {
            health: def.health,
            max_health: def.health,
            speed: def.speed,
            unit_id: unit_id.clone(),
            armor,
        },
        Weapon {
            damage,
            range,
            cooldown,
            timer: 0.0,
            warhead,
        },
        owner,
        Vision {
            range: def.sight_radius,
        },
    ));

    if def.role.as_deref() == Some("harvester") {
        entity_cmds.insert(Harvester::default());
    }

    Some(entity_cmds.id())
}

fn move_units(
    mut commands: Commands,
    mut q_units: ParamSet<(
        Query<(Entity, &Transform, Has<Path>), With<Unit>>,
        Query<(Entity, &mut Transform, &Unit, &mut Path), With<Unit>>,
    )>,
    time: Res<Time>,
    grid: Res<Grid>,
    mut repath_cooldowns: ResMut<MovementRepathCooldowns>,
    stuck_tracking: Res<MovementStuckTracking>,
) {
    const DYNAMIC_REPATH_COOLDOWN: f32 = 0.3;
    let dt = time.delta_secs();
    let unit_positions: Vec<(Entity, Vec3, bool)> = q_units
        .p0()
        .iter()
        .map(|(entity, transform, has_path)| (entity, transform.translation, has_path))
        .collect();
    repath_cooldowns.0.retain(|entity, _| {
        unit_positions
            .iter()
            .any(|(moving_entity, _, has_path)| moving_entity == entity && *has_path)
    });
    for cooldown in repath_cooldowns.0.values_mut() {
        *cooldown = (*cooldown - dt).max(0.0);
    }

    for (entity, mut transform, unit, mut path) in q_units.p1().iter_mut() {
        if path.waypoints.is_empty() {
            commands.entity(entity).try_remove::<Path>();
            continue;
        }

        let mut target_vec2 = path.waypoints[0];
        let gx = target_vec2.x.round() as i32;
        let gz = target_vec2.y.round() as i32;
        let destination = *path.waypoints.last().unwrap();
        let destination_cell = (destination.x.round() as i32, destination.y.round() as i32);
        let destination_is_blocked = grid.is_blocked(destination_cell.0, destination_cell.1);
        let waypoint_is_static_blocker = grid.is_statically_blocked(gx, gz);
        let waypoint_is_dynamic_blocker = grid.is_blocked(gx, gz) && !waypoint_is_static_blocker;
        let dynamic_repath_ready = repath_cooldowns.0.get(&entity).copied().unwrap_or(0.0) <= 0.0;

        if waypoint_is_static_blocker
            || ((waypoint_is_dynamic_blocker || destination_is_blocked) && dynamic_repath_ready)
        {
            let start_pos = Vec2::new(transform.translation.x, transform.translation.z);
            let resolved_destination = nearest_available_destination(start_pos, destination, &grid);
            let new_path = resolved_destination.and_then(|available_destination| {
                crate::game::pathfinding::find_path(start_pos, available_destination, &grid)
            });

            if waypoint_is_dynamic_blocker || destination_is_blocked {
                repath_cooldowns.0.insert(entity, DYNAMIC_REPATH_COOLDOWN);
            }

            if let Some(new_path) = new_path {
                path.waypoints = new_path;
                if path.waypoints.is_empty() {
                    commands.entity(entity).try_remove::<Path>();
                    continue;
                }
                target_vec2 = path.waypoints[0];
            } else {
                commands.entity(entity).try_remove::<Path>();
                continue;
            }
        }

        let target_pos = Vec3::new(target_vec2.x, transform.translation.y, target_vec2.y);

        let direction = target_pos - transform.translation;
        // Only consider XZ distance (ignore Y differences)
        let distance = Vec2::new(direction.x, direction.z).length();

        // Increased threshold so collision pushback doesn't trap units oscillating
        // around a waypoint they can never quite reach
        if distance < 0.5 {
            path.waypoints.remove(0);
            continue;
        }

        let desired_move_dir = Vec3::new(direction.x, 0.0, direction.z).normalize();
        let is_stalled = stuck_tracking
            .0
            .get(&entity)
            .is_some_and(|progress| progress.stalled_for >= 0.6);
        let move_dir = steer_around_units(
            entity,
            transform.translation,
            desired_move_dir,
            &unit_positions,
            is_stalled,
        );

        // Turn towards movement direction
        let look_target = transform.translation + move_dir;
        let desired = Transform::from_translation(transform.translation)
            .looking_at(look_target, Vec3::Y)
            .rotation;
        let turn_speed = 8.0; // radians per second
        transform.rotation = transform
            .rotation
            .slerp(desired, (turn_speed * dt).min(1.0));

        // Calculate how well the unit is facing the target direction
        let current_forward = transform.rotation * Vec3::NEG_Z;
        let dot = current_forward.dot(move_dir);

        // Turn on its center: don't move forward until somewhat facing the target
        let move_factor = if dot > 0.95 {
            1.0 // Almost perfectly facing
        } else if dot > 0.5 {
            (dot - 0.5) * 2.0 // Slow down while turning
        } else {
            0.0 // completely turn on its center
        };

        let move_dist = (unit.speed * move_factor * dt).min(distance);
        let move_vec = move_dir * move_dist;

        transform.translation += move_vec;
    }
}

fn steer_around_units(
    entity: Entity,
    position: Vec3,
    desired_move_dir: Vec3,
    unit_positions: &[(Entity, Vec3, bool)],
    is_stalled: bool,
) -> Vec3 {
    let mut avoidance = Vec3::ZERO;
    let awareness_radius = 1.6;

    for &(other_entity, other_pos, other_has_path) in unit_positions {
        if other_entity == entity {
            continue;
        }

        let offset = other_pos - position;
        let flat_offset = Vec3::new(offset.x, 0.0, offset.z);
        let distance = flat_offset.length();
        if distance <= 0.001 || distance > awareness_radius {
            continue;
        }

        let to_other = flat_offset / distance;
        let ahead = desired_move_dir.dot(to_other);
        if ahead < -0.15 {
            continue;
        }

        let side = desired_move_dir.cross(Vec3::Y).normalize_or_zero();
        let side_sign = if side.dot(to_other) >= 0.0 { -1.0 } else { 1.0 };
        let lateral = side * side_sign;
        let separation = -to_other;
        let proximity = 1.0 - (distance / awareness_radius);

        // Once progress stalls, consider neighbors on every side. The regular
        // forward-looking steering can cancel out perfectly when a unit is
        // pinched between two others.
        if is_stalled {
            avoidance += separation * proximity * 0.9;
        }

        let blocker_weight = if other_has_path { 0.45 } else { 0.9 };
        let ahead_weight = ((ahead + 0.15) / 1.15).clamp(0.0, 1.0);

        avoidance += (lateral * 0.7 + separation * 0.3) * proximity * ahead_weight * blocker_weight;
    }

    if is_stalled && avoidance.length_squared() < 0.04 {
        // Break a symmetrical deadlock consistently so adjacent units do not
        // choose a new side every frame and oscillate.
        let side = desired_move_dir.cross(Vec3::Y).normalize_or_zero();
        let sign = if entity.index().index() % 2 == 0 {
            1.0
        } else {
            -1.0
        };
        avoidance += side * sign * 0.8;
    }

    // Dense groups can have many neighbors inside the awareness radius. Keep
    // normal avoidance subordinate to the order, but allow a stalled unit a
    // stronger escape vector for a short time.
    let avoidance = avoidance.clamp_length_max(if is_stalled { 1.1 } else { 0.75 });
    let steered_dir = desired_move_dir + avoidance;
    if steered_dir.length_squared() > 0.001 {
        steered_dir.normalize()
    } else {
        desired_move_dir
    }
}

fn resolve_unit_collisions(
    mut q_units: Query<(Entity, &mut Transform, Has<Path>), With<Unit>>,
    spatial_hash: Res<SpatialHash>,
) {
    // Collect positions first to allow safe mutable access during resolution.
    // We snapshot positions so neighbor lookups use consistent data.
    let positions: Vec<(Entity, Vec3, bool)> = q_units
        .iter()
        .map(|(e, t, has_path)| (e, t.translation, has_path))
        .collect();

    // Build a quick index from Entity -> index for O(1) neighbor lookup
    let mut entity_index: std::collections::HashMap<Entity, usize> =
        std::collections::HashMap::with_capacity(positions.len());
    for (i, &(entity, _, _)) in positions.iter().enumerate() {
        entity_index.insert(entity, i);
    }

    for &(entity_a, pos_a, has_path_a) in &positions {
        // Query only nearby entities from spatial hash (typically 3x3 cells)
        for neighbor in spatial_hash.query_radius(pos_a.x, pos_a.z, UNIT_MIN_SEPARATION) {
            // Skip self and only process each pair once (lower entity index first)
            if neighbor <= entity_a {
                continue;
            }

            // Look up neighbor's position from our snapshot via index
            let Some(&idx) = entity_index.get(&neighbor) else {
                continue;
            };
            let (_, pos_b, has_path_b) = positions[idx];

            let diff = pos_a - pos_b;
            let dist_sq = diff.x * diff.x + diff.z * diff.z;
            let min_dist = UNIT_MIN_SEPARATION;
            if dist_sq < min_dist * min_dist {
                let dist = dist_sq.sqrt();
                let push_dir = if dist > 0.001 {
                    Vec3::new(diff.x, 0.0, diff.z) / dist
                } else {
                    Vec3::new(1.0, 0.0, 0.0)
                };
                let overlap = min_dist - dist;
                let (push_a, push_b) = match (has_path_a, has_path_b) {
                    (true, true) => {
                        // Split the full correction so moving units cannot settle
                        // closer than the minimum separation.
                        let push = push_dir * (overlap * 0.5);
                        (push, -push)
                    }
                    (true, false) => {
                        // Idle units yield a little so a moving unit cannot be
                        // permanently pinned inside a stationary crowd.
                        (push_dir * (overlap * 0.7), -push_dir * (overlap * 0.3))
                    }
                    (false, true) => (push_dir * (overlap * 0.3), -push_dir * (overlap * 0.7)),
                    (false, false) => {
                        let push = push_dir * (overlap * 0.5);
                        (push, -push)
                    }
                };

                if push_a != Vec3::ZERO {
                    if let Ok((_, mut t, _)) = q_units.get_mut(entity_a) {
                        t.translation += push_a;
                    }
                }
                if push_b != Vec3::ZERO {
                    if let Ok((_, mut t, _)) = q_units.get_mut(neighbor) {
                        t.translation += push_b;
                    }
                }
            }
        }
    }
}

fn track_stuck_unit_moves(
    q_units: Query<(Entity, &Transform, Option<&Path>), With<Unit>>,
    time: Res<Time>,
    mut tracking: ResMut<MovementStuckTracking>,
) {
    const MIN_PROGRESS: f32 = 0.1;
    const STUCK_DURATION: f32 = 2.0;
    const UNIT_CONTACT_DISTANCE: f32 = 1.1;
    const NEW_DESTINATION_DISTANCE: f32 = 4.0;

    let dt = time.delta_secs();
    let unit_positions: Vec<(Entity, Vec2)> = q_units
        .iter()
        .map(|(entity, transform, _)| {
            (
                entity,
                Vec2::new(transform.translation.x, transform.translation.z),
            )
        })
        .collect();
    let mut moving_units = Vec::new();

    for (entity, transform, opt_path) in q_units.iter() {
        let Some(path) = opt_path else {
            continue;
        };
        let Some(&destination) = path.waypoints.last() else {
            continue;
        };

        let position = Vec2::new(transform.translation.x, transform.translation.z);
        let distance = position.distance(destination);
        let progress = tracking.0.entry(entity).or_insert(MovementProgress {
            destination,
            best_distance: distance,
            stalled_for: 0.0,
        });

        // Repathing frequently changes the next waypoint, so measure progress
        // against the final destination. Only a large destination jump is
        // treated as a genuinely new movement intent.
        if progress.destination.distance(destination) > NEW_DESTINATION_DISTANCE {
            progress.destination = destination;
            progress.best_distance = distance;
            progress.stalled_for = 0.0;
        } else {
            let tracked_distance = position.distance(progress.destination);
            if tracked_distance <= progress.best_distance - MIN_PROGRESS {
                progress.best_distance = tracked_distance;
                progress.stalled_for = 0.0;
            } else {
                progress.stalled_for += dt;
            }
        }

        moving_units.push((entity, position, progress.stalled_for));
    }

    tracking
        .0
        .retain(|entity, _| moving_units.iter().any(|(moving, _, _)| moving == entity));

    let mut temporarily_blocked = Vec::new();
    for &(entity, position, stalled_for) in &moving_units {
        if stalled_for < STUCK_DURATION {
            continue;
        }

        let trapped_by_unit = unit_positions.iter().any(|&(other, other_position)| {
            other != entity
                && position.distance_squared(other_position)
                    <= UNIT_CONTACT_DISTANCE * UNIT_CONTACT_DISTANCE
        });
        if trapped_by_unit {
            temporarily_blocked.push(entity);
        }
    }

    // A traffic jam is not a cancelled order. Let avoidance and collision
    // resolution keep working instead of silently abandoning group members.
    // Reset the timer so a persistent obstruction is checked again later.
    temporarily_blocked.sort_unstable();
    temporarily_blocked.dedup();
    for entity in temporarily_blocked {
        if let Some(progress) = tracking.0.get_mut(&entity) {
            progress.stalled_for = 0.0;
        }
    }
}

fn resolve_obstacle_collisions_system(
    mut q_units: Query<&mut Transform, With<Unit>>,
    grid: Res<Grid>,
) {
    let radius = 0.45;
    for mut transform in q_units.iter_mut() {
        let ux = transform.translation.x;
        let uz = transform.translation.z;
        let gx = ux.round() as i32;
        let gz = uz.round() as i32;

        for dx in -1..=1 {
            for dz in -1..=1 {
                let cx = gx + dx;
                let cz = gz + dz;
                // Only push against STATIC obstacles (buildings, map edges).
                // Dynamic blocking (idle units) is handled by resolve_unit_collisions.
                // Using is_blocked here caused double-pushing: the unit collision system
                // pushed the unit, then this system pushed it again for the same idle unit.
                if dx == 0 && dz == 0 && !grid.is_statically_blocked(cx, cz) {
                    continue;
                }
                if grid.is_statically_blocked(cx, cz) {
                    let min_x = cx as f32 - 0.5;
                    let max_x = cx as f32 + 0.5;
                    let min_z = cz as f32 - 0.5;
                    let max_z = cz as f32 + 0.5;

                    let closest_x = ux.clamp(min_x, max_x);
                    let closest_z = uz.clamp(min_z, max_z);

                    let diff_x = ux - closest_x;
                    let diff_z = uz - closest_z;
                    let dist_sq = diff_x * diff_x + diff_z * diff_z;
                    let min_dist = radius;
                    if dist_sq < min_dist * min_dist {
                        let dist = dist_sq.sqrt();
                        let push_dir = if dist > 0.001 {
                            Vec3::new(diff_x, 0.0, diff_z) / dist
                        } else {
                            let left_dist = ux - min_x;
                            let right_dist = max_x - ux;
                            let top_dist = uz - min_z;
                            let bottom_dist = max_z - uz;
                            let min_edge_dist =
                                left_dist.min(right_dist).min(top_dist).min(bottom_dist);
                            if min_edge_dist == left_dist {
                                Vec3::new(-1.0, 0.0, 0.0)
                            } else if min_edge_dist == right_dist {
                                Vec3::new(1.0, 0.0, 0.0)
                            } else if min_edge_dist == top_dist {
                                Vec3::new(0.0, 0.0, -1.0)
                            } else {
                                Vec3::new(0.0, 0.0, 1.0)
                            }
                        };
                        let push_amount = min_dist - dist;
                        transform.translation += push_dir * push_amount;
                    }
                }
            }
        }
    }
}
