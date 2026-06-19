use crate::game::buildings::{Building, PowerSystem};
use crate::game::data::{Definitions, WarheadTable, WarheadType};
use crate::game::fog_of_war::Vision;
use crate::game::game_state::{AppState, game_is_playing};
use crate::game::pathfinding::{Grid, Path, find_path};
use crate::game::player::{LocalPlayer, Players};
use crate::game::spatial_hash::SpatialHash;
use crate::game::units::{Owner, Unit};
use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(JoinFightTimer(Timer::from_seconds(
            0.25,
            TimerMode::Repeating,
        )))
        .init_resource::<PursuitPathCache>()
        .add_systems(
            Update,
            (
                auto_attack_logic,
                join_fight_logic.after(auto_attack_logic),
                combat_logic,
                projectile_movement,
                explosion_logic,
                health_logic,
            )
                .run_if(game_is_playing)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

#[derive(Component)]
pub struct Weapon {
    pub damage: f32,
    pub range: f32,
    pub cooldown: f32,
    pub timer: f32,
    pub warhead: WarheadType,
}

#[derive(Component)]
pub struct AttackTarget(pub Entity);

#[derive(Component)]
pub struct Projectile {
    pub target: Entity,
    pub damage: f32,
    pub speed: f32,
    pub warhead: WarheadType,
}

#[derive(Component)]
pub struct Explosion {
    pub timer: f32,
    pub max_time: f32,
}

#[derive(Component)]
pub struct HealthBar;

#[derive(Resource)]
pub struct JoinFightTimer(pub Timer);

#[derive(Default, Resource)]
struct PursuitPathCache(HashMap<Entity, PursuitPathState>);

struct PursuitPathState {
    last_target_position: Vec2,
    retry_cooldown: f32,
}

fn auto_attack_logic(
    mut commands: Commands,
    q_attackers: Query<(
        Entity,
        &Transform,
        &Weapon,
        &Owner,
        Option<&AttackTarget>,
        Has<Path>,
    )>,
    q_targets: Query<(
        Entity,
        &GlobalTransform,
        &Owner,
        Option<&Unit>,
        Option<&crate::game::buildings::Building>,
        &Visibility,
    )>,
    players: Res<Players>,
    spatial_hash: Res<SpatialHash>,
) {
    for (attacker_ent, attacker_transform, weapon, attacker_owner, opt_target, has_path) in
        q_attackers.iter()
    {
        if opt_target.is_none() && has_path {
            continue; // Moving via move order, do not auto-acquire targets
        }
        if let Some(target) = opt_target {
            if let Ok((_, _, _, opt_unit, opt_building, visibility)) = q_targets.get(target.0) {
                if *visibility == Visibility::Hidden {
                    if let Ok(mut cmds) = commands.get_entity(attacker_ent) {
                        cmds.try_remove::<AttackTarget>();
                    }
                } else if opt_building.is_some() && opt_unit.is_none() {
                    // Valid building target
                } else {
                    continue; // Already attacking a valid unit
                }
            }
        }

        let mut best_target = None;
        let mut best_dist = f32::MAX;
        let mut found_unit = false;

        // Use spatial hash to only check nearby entities within weapon range
        let pos = attacker_transform.translation;
        for candidate in spatial_hash.query_radius(pos.x, pos.z, weapon.range) {
            if let Ok((
                target_ent,
                target_transform,
                target_owner,
                opt_unit,
                opt_building,
                visibility,
            )) = q_targets.get(candidate)
            {
                let same_team = if let (Some(a), Some(t)) = (
                    players.players.get(&attacker_owner.0),
                    players.players.get(&target_owner.0),
                ) {
                    a.team_id == t.team_id
                } else {
                    attacker_owner.0 == target_owner.0
                };
                if same_team {
                    continue;
                }
                if opt_unit.is_none() && opt_building.is_none() {
                    continue;
                }
                if *visibility == Visibility::Hidden {
                    continue;
                }

                let dist = pos.distance(target_transform.translation());
                if dist <= weapon.range {
                    let is_unit = opt_unit.is_some();
                    if is_unit && !found_unit {
                        found_unit = true;
                        best_target = Some(target_ent);
                        best_dist = dist;
                    } else if is_unit == found_unit {
                        if dist < best_dist {
                            best_target = Some(target_ent);
                            best_dist = dist;
                        }
                    }
                }
            }
        }

        // Also check buildings (not in spatial hash since it only tracks units)
        // Buildings are few so a targeted scan is fine
        if !found_unit {
            for (target_ent, target_transform, target_owner, opt_unit, opt_building, visibility) in
                q_targets.iter()
            {
                if opt_building.is_none() || opt_unit.is_some() {
                    continue;
                }
                let same_team = if let (Some(a), Some(t)) = (
                    players.players.get(&attacker_owner.0),
                    players.players.get(&target_owner.0),
                ) {
                    a.team_id == t.team_id
                } else {
                    attacker_owner.0 == target_owner.0
                };
                if same_team {
                    continue;
                }
                if *visibility == Visibility::Hidden {
                    continue;
                }

                let dist = pos.distance(target_transform.translation());
                if dist <= weapon.range && dist < best_dist {
                    best_target = Some(target_ent);
                    best_dist = dist;
                }
            }
        }

        if let Some(new_target) = best_target {
            let should_switch = if let Some(target) = opt_target {
                if found_unit {
                    target.0 != new_target
                } else {
                    false // Don't switch from one building to another automatically
                }
            } else {
                true // We had no target, so take anything we found
            };

            if should_switch {
                if let Ok(mut cmds) = commands.get_entity(attacker_ent) {
                    cmds.try_insert(AttackTarget(new_target));
                }
            }
        }
    }
}

fn combat_logic(
    mut commands: Commands,
    mut q_attackers: Query<(
        Entity,
        &mut Transform,
        &mut Weapon,
        &AttackTarget,
        Option<&mut Path>,
    )>,
    q_target_pos: Query<&GlobalTransform>,
    time: Res<Time>,
    grid: Res<Grid>,
    mut pursuit_cache: ResMut<PursuitPathCache>,
) {
    const REPATH_COOLDOWN: f32 = 0.35;
    let mut active_attackers = HashSet::new();

    for (entity, mut transform, mut weapon, target, opt_path) in q_attackers.iter_mut() {
        active_attackers.insert(entity);
        weapon.timer -= time.delta_secs();

        if let Ok(target_transform) = q_target_pos.get(target.0) {
            let target_translation = target_transform.translation();
            let dist = transform.translation.distance(target_translation);

            if dist <= weapon.range {
                pursuit_cache.0.remove(&entity);
                // In range, stop moving
                if let Some(mut path) = opt_path {
                    path.waypoints.clear();
                }

                // Look at target
                transform.look_at(target_translation, Vec3::Y);

                // Fire weapon
                if weapon.timer <= 0.0 {
                    weapon.timer = weapon.cooldown;

                    commands.spawn((
                        Transform::from_translation(
                            transform.translation + Vec3::new(0.0, 0.5, 0.0),
                        ),
                        Projectile {
                            target: target.0,
                            damage: weapon.damage,
                            speed: 30.0,
                            warhead: weapon.warhead,
                        },
                    ));
                }
            } else {
                // Crowded targets may temporarily be unreachable. Throttle both
                // failed retries and moving-target replans to keep large battles
                // from running A* once per attacker per frame.
                let needs_path = opt_path.as_ref().map_or(true, |p| p.waypoints.is_empty());
                let target_position = Vec2::new(target_translation.x, target_translation.z);
                let pursuit = pursuit_cache.0.entry(entity).or_insert(PursuitPathState {
                    last_target_position: target_position,
                    retry_cooldown: 0.0,
                });
                pursuit.retry_cooldown = (pursuit.retry_cooldown - time.delta_secs()).max(0.0);
                let target_moved = target_position.distance(pursuit.last_target_position) > 1.5;

                if (needs_path || target_moved) && pursuit.retry_cooldown <= 0.0 {
                    let start_pos = Vec2::new(transform.translation.x, transform.translation.z);
                    if let Some(path_waypoints) = find_path(start_pos, target_position, &grid) {
                        if let Ok(mut cmds) = commands.get_entity(entity) {
                            cmds.try_insert(Path {
                                waypoints: path_waypoints,
                            });
                        }
                    } else if let Ok(mut cmds) = commands.get_entity(entity) {
                        cmds.try_remove::<Path>();
                        cmds.try_remove::<AttackTarget>();
                    }
                    pursuit.last_target_position = target_position;
                    pursuit.retry_cooldown = REPATH_COOLDOWN;
                }
            }
        } else {
            pursuit_cache.0.remove(&entity);
            // Target is dead or invalid
            if let Ok(mut cmds) = commands.get_entity(entity) {
                cmds.try_remove::<AttackTarget>();
            }
        }
    }

    pursuit_cache
        .0
        .retain(|entity, _| active_attackers.contains(entity));
}

fn projectile_movement(
    mut commands: Commands,
    mut q_projectiles: Query<(Entity, &mut Transform, &Projectile)>,
    q_target_pos: Query<&GlobalTransform>,
    mut q_units: Query<&mut Unit>,
    mut q_buildings: Query<&mut Building>,
    time: Res<Time>,
    warhead_table: Res<WarheadTable>,
) {
    for (proj_entity, mut proj_transform, projectile) in q_projectiles.iter_mut() {
        if let Ok(target_transform) = q_target_pos.get(projectile.target) {
            let target_pos = target_transform.translation() + Vec3::new(0.0, 0.5, 0.0);
            let direction = target_pos - proj_transform.translation;
            let distance = direction.length();
            let move_dist = projectile.speed * time.delta_secs();

            if distance < move_dist {
                // Hit! Apply warhead vs armor modifier.
                if let Ok(mut target_unit) = q_units.get_mut(projectile.target) {
                    let modifier =
                        warhead_table.get_modifier(projectile.warhead, target_unit.armor);
                    target_unit.health -= projectile.damage * modifier;
                } else if let Ok(mut target_building) = q_buildings.get_mut(projectile.target) {
                    let modifier =
                        warhead_table.get_modifier(projectile.warhead, target_building.armor);
                    target_building.health -= projectile.damage * modifier;
                }
                if let Ok(mut cmds) = commands.get_entity(proj_entity) {
                    cmds.try_despawn();
                }
            } else {
                let move_vec = direction.normalize() * move_dist;
                proj_transform.translation += move_vec;
            }
        } else {
            // Target dead mid-flight
            if let Ok(mut cmds) = commands.get_entity(proj_entity) {
                cmds.try_despawn();
            }
        }
    }
}

fn health_logic(
    mut commands: Commands,
    q_units: Query<(Entity, &Unit, &Transform)>,
    q_buildings: Query<(Entity, &Building, &Transform, &Owner)>,
    definitions: Res<Definitions>,
    mut grid: ResMut<Grid>,
    mut power: ResMut<PowerSystem>,
    local_player: Res<LocalPlayer>,
) {
    for (entity, unit, transform) in q_units.iter() {
        if unit.health <= 0.0 {
            // Die
            if let Ok(mut cmds) = commands.get_entity(entity) {
                cmds.try_despawn();
            }

            commands.spawn((
                Transform::from_translation(transform.translation),
                Explosion {
                    timer: 0.0,
                    max_time: 0.5,
                },
            ));
        }
    }

    for (entity, building, transform, owner) in q_buildings.iter() {
        if building.health <= 0.0 {
            // Unblock grid cells
            if let Some(def) = definitions.buildings.get(&building.building_id) {
                let size = def.size;
                let min_x = (transform.translation.x - size.0 as f32 / 2.0 + 0.5).round() as i32;
                let min_z = (transform.translation.z - size.1 as f32 / 2.0 + 0.5).round() as i32;
                for dz in 0..size.1 {
                    for dx in 0..size.0 {
                        grid.set_blocked(min_x + dx, min_z + dz, false);
                    }
                }

                println!("Building {} (Owner {}) was destroyed!", def.name, owner.0);

                // Adjust power if local player
                if owner.0 == local_player.0 {
                    power.produced -= def.power_produced;
                    power.consumed -= def.power_consumed;
                }
            }

            // Die
            if let Ok(mut cmds) = commands.get_entity(entity) {
                cmds.try_despawn();
            }

            commands.spawn((
                Transform::from_translation(transform.translation),
                Explosion {
                    timer: 0.0,
                    max_time: 1.0,
                },
            ));
        }
    }
}

fn explosion_logic(
    mut commands: Commands,
    mut q_explosions: Query<(Entity, &mut Transform, &mut Explosion)>,
    time: Res<Time>,
) {
    for (entity, mut transform, mut explosion) in q_explosions.iter_mut() {
        explosion.timer += time.delta_secs();
        if explosion.timer >= explosion.max_time {
            if let Ok(mut cmds) = commands.get_entity(entity) {
                cmds.try_despawn();
            }
        } else {
            let scale = 1.0 + (explosion.timer / explosion.max_time) * 2.0;
            transform.scale = Vec3::splat(scale);
        }
    }
}

fn join_fight_logic(
    mut commands: Commands,
    time: Res<Time>,
    mut join_timer: ResMut<JoinFightTimer>,
    q_in_combat: Query<(&Transform, &Owner, &AttackTarget), With<Unit>>,
    q_idle: Query<
        (Entity, &Transform, &Owner, &Vision, &Weapon),
        (With<Unit>, Without<AttackTarget>, Without<Path>),
    >,
    players: Res<Players>,
    spatial_hash: Res<SpatialHash>,
) {
    // Throttle to every 0.25s — idle units don't need instant response
    if !join_timer.0.tick(time.delta()).just_finished() {
        return;
    }

    for (combat_transform, combat_owner, target) in q_in_combat.iter() {
        let combat_pos = combat_transform.translation;

        // Use spatial hash to only check nearby idle units
        for candidate in spatial_hash.query_radius(combat_pos.x, combat_pos.z, 15.0) {
            // Check if this candidate is an idle unit (has no AttackTarget, no Path)
            let Ok((idle_ent, idle_transform, idle_owner, vision, weapon)) = q_idle.get(candidate)
            else {
                continue;
            };

            // Must be on the same team
            let same_team = if let (Some(c), Some(i)) = (
                players.players.get(&combat_owner.0),
                players.players.get(&idle_owner.0),
            ) {
                c.team_id == i.team_id
            } else {
                combat_owner.0 == idle_owner.0
            };
            if !same_team {
                continue;
            }

            // Only combat units (damage > 0.0) join the fight
            if weapon.damage <= 0.0 {
                continue;
            }

            // "have this unit in range": check if B has A within B's sight radius
            let dist = idle_transform.translation.distance(combat_pos);
            if dist <= vision.range {
                // Join the fight! Attack A's target.
                if let Ok(mut cmds) = commands.get_entity(idle_ent) {
                    cmds.try_insert(AttackTarget(target.0));
                }
            }
        }
    }
}
