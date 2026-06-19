use crate::game::buildings::{
    Building, BuildingQueueEntry, Constructing, PowerSystem, ProductionQueue, TeamBuildingQueues,
};
use crate::game::combat::AttackTarget;
use crate::game::data::{ArmorType, Definitions};
use crate::game::economy::{Harvester, HarvesterState, OreField};
use crate::game::fog_of_war::Vision;
use crate::game::game_state::AppState;
use crate::game::pathfinding::{Grid, Path, find_path};
use crate::game::player::Players;
use crate::game::units::{Owner, Unit};
use bevy::prelude::*;
use std::collections::HashSet;

pub struct CommandsPlugin;

impl Plugin for CommandsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<MoveCommand>()
            .add_message::<AttackCommand>()
            .add_message::<HarvestCommand>()
            .add_message::<BuildCommand>()
            .add_message::<CancelBuildCommand>()
            .add_message::<PlaceBuildingCommand>()
            .add_message::<TrainUnitCommand>()
            .add_message::<CancelUnitCommand>()
            .add_message::<ConsumeReadyCommand>()
            .add_message::<SellBuildingCommand>()
            .add_message::<SetRallyPointCommand>()
            .add_message::<ReturnToRefineryCommand>()
            .add_systems(
                Update,
                (
                    handle_move_commands,
                    handle_attack_commands,
                    handle_harvest_commands,
                    handle_build_commands,
                    handle_cancel_build_commands,
                    handle_place_building_commands,
                    handle_train_unit_commands,
                    handle_cancel_unit_commands,
                    handle_consume_ready_commands,
                    handle_sell_building_commands,
                    handle_set_rally_point_commands,
                    handle_return_to_refinery_commands,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

#[derive(Message, Debug, Clone)]
pub struct MoveCommand {
    pub player_id: usize,
    pub unit_entities: Vec<Entity>,
    pub target_pos: Vec2,
}

#[derive(Message, Debug, Clone)]
pub struct AttackCommand {
    pub player_id: usize,
    pub unit_entities: Vec<Entity>,
    pub target_entity: Entity,
}

#[derive(Message, Debug, Clone)]
pub struct HarvestCommand {
    pub player_id: usize,
    pub unit_entities: Vec<Entity>,
    pub ore_entity: Entity,
}

#[derive(Message, Debug, Clone)]
pub struct ReturnToRefineryCommand {
    pub player_id: usize,
    pub unit_entities: Vec<Entity>,
    pub refinery_entity: Entity,
}

#[derive(Message, Debug, Clone)]
pub struct BuildCommand {
    pub player_id: usize,
    pub building_id: String,
}

#[derive(Message, Debug, Clone)]
pub struct CancelBuildCommand {
    pub player_id: usize,
    pub building_id: String,
}

#[derive(Message, Debug, Clone)]
pub struct PlaceBuildingCommand {
    pub player_id: usize,
    pub building_id: String,
    pub position: Vec3,
}

#[derive(Message, Debug, Clone)]
pub struct TrainUnitCommand {
    pub player_id: usize,
    pub building_entity: Entity,
    pub unit_id: String,
}

#[derive(Message, Debug, Clone)]
pub struct CancelUnitCommand {
    pub player_id: usize,
    pub building_entity: Entity,
    pub unit_id: String,
}

#[derive(Message, Debug, Clone)]
pub struct ConsumeReadyCommand {
    pub player_id: usize,
    pub building_id: String,
}

#[derive(Message, Debug, Clone)]
pub struct SellBuildingCommand {
    pub player_id: usize,
    pub building_entity: Entity,
}

#[derive(Message, Debug, Clone)]
pub struct SetRallyPointCommand {
    pub player_id: usize,
    pub building_entity: Entity,
    pub target_pos: Vec2,
}

// Helper functions for team credits management
fn get_player_credits(player_id: usize, players: &Players) -> u32 {
    players
        .players
        .get(&player_id)
        .map(|p| p.credits)
        .unwrap_or(0)
}

fn change_player_credits(player_id: usize, delta: i32, players: &mut Players) {
    if let Some(player) = players.players.get_mut(&player_id) {
        if delta < 0 {
            player.credits = player.credits.saturating_sub(delta.abs() as u32);
        } else {
            player.credits = player.credits.saturating_add(delta as u32);
        }
    }
}

fn handle_move_commands(
    mut commands: Commands,
    mut events: MessageReader<MoveCommand>,
    mut q_units: Query<(Entity, &Transform, &Owner, Option<&mut Harvester>), With<Unit>>,
    mut grid: ResMut<Grid>,
) {
    for cmd in events.read() {
        // Collect positions of all units in this group so we can temporarily
        // unblock their cells so they do not block each other's paths.
        let mut group_units: Vec<(Entity, Vec2)> = Vec::new();
        let mut group_cells: Vec<(i32, i32)> = Vec::new();
        for &entity in &cmd.unit_entities {
            if let Ok((_ent, transform, owner, _)) = q_units.get(entity) {
                if owner.0 == cmd.player_id {
                    let pos = Vec2::new(transform.translation.x, transform.translation.z);
                    group_units.push((entity, pos));
                    let gx = transform.translation.x.round() as i32;
                    let gz = transform.translation.z.round() as i32;
                    group_cells.push((gx, gz));
                }
            }
        }

        if group_units.is_empty() {
            continue;
        }

        // Temporarily unblock all group member cells
        for &(gx, gz) in &group_cells {
            if !grid.is_statically_blocked(gx, gz) {
                grid.set_dynamic_blocked(gx, gz, false);
            }
        }

        let assignments = assign_group_move_slots(cmd.target_pos, &group_units, &grid);

        for (entity, end_pos) in assignments {
            if let Ok((ent, transform, owner, opt_harvester)) = q_units.get_mut(entity) {
                if owner.0 != cmd.player_id {
                    continue;
                }

                if let Some(mut harvester) = opt_harvester {
                    harvester.state = HarvesterState::Idle;
                }

                let start_pos = Vec2::new(transform.translation.x, transform.translation.z);

                if let Some(path_waypoints) = find_path(start_pos, end_pos, &grid) {
                    commands.entity(ent).try_insert(Path {
                        waypoints: path_waypoints,
                    });
                    commands.entity(ent).try_remove::<AttackTarget>();
                }
            }
        }

        // Restore dynamic blocking for group cells (will be re-evaluated next frame
        // by update_dynamic_grid_blocking for any units that are still idle)
        for &(gx, gz) in &group_cells {
            if !grid.is_statically_blocked(gx, gz) {
                grid.set_dynamic_blocked(gx, gz, true);
            }
        }
    }
}

fn assign_group_move_slots(
    target_pos: Vec2,
    group_units: &[(Entity, Vec2)],
    grid: &Grid,
) -> Vec<(Entity, Vec2)> {
    if group_units.len() <= 1 {
        return group_units
            .iter()
            .map(|(entity, _)| (*entity, target_pos))
            .collect();
    }

    let mut available_slots = formation_slots_near_target(target_pos, group_units.len(), grid);
    if available_slots.is_empty() {
        return group_units
            .iter()
            .map(|(entity, _)| (*entity, target_pos))
            .collect();
    }

    let mut units: Vec<(Entity, Vec2)> = group_units.to_vec();
    units.sort_by(|(_, a), (_, b)| {
        a.distance_squared(target_pos)
            .partial_cmp(&b.distance_squared(target_pos))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut assignments = Vec::with_capacity(units.len());
    for (entity, unit_pos) in units {
        if available_slots.is_empty() {
            assignments.push((entity, target_pos));
            continue;
        }

        let mut best_slot = 0;
        let mut best_dist = f32::MAX;
        for (idx, slot) in available_slots.iter().enumerate() {
            let dist = unit_pos.distance_squared(*slot);
            if dist < best_dist {
                best_dist = dist;
                best_slot = idx;
            }
        }

        assignments.push((entity, available_slots.remove(best_slot)));
    }

    assignments
}

fn formation_slots_near_target(target_pos: Vec2, needed: usize, grid: &Grid) -> Vec<Vec2> {
    const MIN_SLOT_SPACING: f32 = 1.5;
    let center = (target_pos.x.round() as i32, target_pos.y.round() as i32);
    let mut slots: Vec<Vec2> = Vec::with_capacity(needed);
    let mut reserved = HashSet::new();
    let max_radius: i32 = 18;

    for radius in 0..=max_radius {
        let mut ring = Vec::new();
        for dz in -radius..=radius {
            for dx in -radius..=radius {
                if radius > 0 && dx.abs().max(dz.abs()) != radius {
                    continue;
                }

                let cell = (center.0 + dx, center.1 + dz);
                if reserved.contains(&cell) || grid.is_blocked(cell.0, cell.1) {
                    continue;
                }

                ring.push(cell);
            }
        }

        ring.sort_by(|a, b| {
            let a_dist = Vec2::new(a.0 as f32, a.1 as f32).distance_squared(target_pos);
            let b_dist = Vec2::new(b.0 as f32, b.1 as f32).distance_squared(target_pos);
            a_dist
                .partial_cmp(&b_dist)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.1.cmp(&b.1))
                .then_with(|| a.0.cmp(&b.0))
        });

        for cell in ring {
            let candidate = Vec2::new(cell.0 as f32, cell.1 as f32);
            if slots
                .iter()
                .any(|slot| slot.distance_squared(candidate) < MIN_SLOT_SPACING * MIN_SLOT_SPACING)
            {
                continue;
            }

            reserved.insert(cell);
            slots.push(candidate);
            if slots.len() == needed {
                return slots;
            }
        }
    }

    slots
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formation_destinations_have_breathing_room() {
        let grid = Grid::new(64, 64, 64, 64);
        let slots = formation_slots_near_target(Vec2::new(32.0, 32.0), 12, &grid);

        assert_eq!(slots.len(), 12);
        for (index, slot) in slots.iter().enumerate() {
            for other in slots.iter().skip(index + 1) {
                assert!(slot.distance(*other) >= 1.5);
            }
        }
    }
}

fn handle_attack_commands(
    mut commands: Commands,
    mut events: MessageReader<AttackCommand>,
    mut q_units: Query<(&Owner, Option<&mut Harvester>), With<Unit>>,
) {
    for cmd in events.read() {
        for &entity in &cmd.unit_entities {
            if let Ok((owner, opt_harvester)) = q_units.get_mut(entity) {
                if owner.0 != cmd.player_id {
                    continue;
                }
                if let Some(mut harvester) = opt_harvester {
                    harvester.state = HarvesterState::Idle;
                }
                commands
                    .entity(entity)
                    .try_insert(AttackTarget(cmd.target_entity));
                commands.entity(entity).try_remove::<Path>();
            }
        }
    }
}

fn handle_harvest_commands(
    mut commands: Commands,
    mut events: MessageReader<HarvestCommand>,
    mut q_harvesters: Query<(&Owner, &mut Harvester), With<Unit>>,
) {
    for cmd in events.read() {
        for &entity in &cmd.unit_entities {
            if let Ok((owner, mut harvester)) = q_harvesters.get_mut(entity) {
                if owner.0 != cmd.player_id {
                    continue;
                }
                harvester.state = HarvesterState::MovingToOre(cmd.ore_entity);
                commands.entity(entity).try_remove::<Path>();
                commands.entity(entity).try_remove::<AttackTarget>();
            }
        }
    }
}

fn handle_return_to_refinery_commands(
    mut commands: Commands,
    mut events: MessageReader<ReturnToRefineryCommand>,
    mut q_harvesters: Query<(&Owner, &mut Harvester), With<Unit>>,
) {
    for cmd in events.read() {
        for &entity in &cmd.unit_entities {
            if let Ok((owner, mut harvester)) = q_harvesters.get_mut(entity) {
                if owner.0 != cmd.player_id {
                    continue;
                }
                harvester.state = HarvesterState::ReturningToRefinery(None);
                commands.entity(entity).try_remove::<Path>();
                commands.entity(entity).try_remove::<AttackTarget>();
            }
        }
    }
}

fn handle_build_commands(
    mut events: MessageReader<BuildCommand>,
    mut team_queues: ResMut<TeamBuildingQueues>,
    mut players: ResMut<Players>,
    definitions: Res<Definitions>,
    q_buildings: Query<(&Building, &Owner)>,
) {
    for cmd in events.read() {
        let Some(def) = definitions.buildings.get(&cmd.building_id) else {
            continue;
        };
        let queue = team_queues.0.entry(cmd.player_id).or_default();
        if queue.current.is_none() && queue.ready.is_none() {
            // Check requirements
            if let Some(reqs) = &def.requires {
                let mut has_all = true;
                for req in reqs {
                    let mut found = false;
                    for (building, owner) in q_buildings.iter() {
                        if owner.0 == cmd.player_id && &building.building_id == req {
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        has_all = false;
                        break;
                    }
                }
                if !has_all {
                    println!(
                        "BuildCommand: Requirements not met for {} (Team {})",
                        def.name, cmd.player_id
                    );
                    continue;
                }
            }

            let credits = get_player_credits(cmd.player_id, &players);
            if credits < def.cost {
                println!(
                    "BuildCommand: Not enough credits! Need {} (Team {})",
                    def.cost, cmd.player_id
                );
                continue;
            }

            change_player_credits(cmd.player_id, -(def.cost as i32), &mut players);
            queue.current = Some(BuildingQueueEntry {
                building_id: cmd.building_id.clone(),
                progress: 0.0,
                build_time: def.build_time,
            });
            println!("Team {} started building {}", cmd.player_id, def.name);
        } else {
            println!("Team {} already building something", cmd.player_id);
        }
    }
}

fn handle_cancel_build_commands(
    mut events: MessageReader<CancelBuildCommand>,
    mut team_queues: ResMut<TeamBuildingQueues>,
    mut players: ResMut<Players>,
    definitions: Res<Definitions>,
) {
    for cmd in events.read() {
        let Some(def) = definitions.buildings.get(&cmd.building_id) else {
            continue;
        };
        let queue = team_queues.0.entry(cmd.player_id).or_default();

        // Cancel if currently building
        if let Some(ref entry) = queue.current {
            if entry.building_id == cmd.building_id {
                change_player_credits(cmd.player_id, def.cost as i32, &mut players);
                queue.current = None;
                println!(
                    "Cancelled {}, refunded {} credits to Team {}",
                    def.name, def.cost, cmd.player_id
                );
            }
        }
        // Cancel if ready but not placed
        else if queue.ready.as_ref() == Some(&cmd.building_id) {
            change_player_credits(cmd.player_id, def.cost as i32, &mut players);
            queue.ready = None;
            println!(
                "Cancelled ready {}, refunded {} credits to Team {}",
                def.name, def.cost, cmd.player_id
            );
        }
    }
}

fn handle_place_building_commands(
    mut commands: Commands,
    mut events: MessageReader<PlaceBuildingCommand>,
    definitions: Res<Definitions>,
    mut power: ResMut<PowerSystem>,
    mut grid: ResMut<Grid>,
    q_units: Query<&Transform, With<Unit>>,
    q_ore: Query<&Transform, With<OreField>>,
    mut team_queues: ResMut<TeamBuildingQueues>,
    local_player: Res<crate::game::player::LocalPlayer>,
) {
    for cmd in events.read() {
        let Some(def) = definitions.buildings.get(&cmd.building_id) else {
            continue;
        };

        let size = def.size;
        let min_x = (cmd.position.x - size.0 as f32 / 2.0 + 0.5).round() as i32;
        let min_z = (cmd.position.z - size.1 as f32 / 2.0 + 0.5).round() as i32;
        let y_pos = if def.model_path.is_some() { 0.0 } else { 0.5 };
        let snapped_pos = Vec3::new(cmd.position.x, y_pos, cmd.position.z);

        // Verify if grid is blocked
        let mut is_blocked = false;
        for dz in 0..size.1 {
            for dx in 0..size.0 {
                let check_x = min_x + dx;
                let check_z = min_z + dz;
                if grid.is_blocked(check_x, check_z) {
                    is_blocked = true;
                }
            }
        }

        // Check if any unit is in the placement footprint
        for unit_transform in q_units.iter() {
            let ux = unit_transform.translation.x.round() as i32;
            let uz = unit_transform.translation.z.round() as i32;
            let max_x = min_x + size.0;
            let max_z = min_z + size.1;
            if ux >= min_x && ux < max_x && uz >= min_z && uz < max_z {
                is_blocked = true;
            }
        }

        // Ore is walkable for units, but construction cannot cover resource nodes.
        for ore_transform in q_ore.iter() {
            let ox = ore_transform.translation.x.round() as i32;
            let oz = ore_transform.translation.z.round() as i32;
            let max_x = min_x + size.0;
            let max_z = min_z + size.1;
            if ox >= min_x && ox < max_x && oz >= min_z && oz < max_z {
                is_blocked = true;
            }
        }

        if is_blocked {
            println!("PlaceBuildingCommand: Cell is blocked!");
            continue;
        }

        // Consume the ready building from the queue
        if let Some(queue) = team_queues.0.get_mut(&cmd.player_id) {
            if queue.ready.as_ref() == Some(&cmd.building_id) {
                queue.ready = None;
            }
        }

        // Apply power modification (only for Local Player)
        if cmd.player_id == local_player.0 {
            power.produced += def.power_produced;
            power.consumed += def.power_consumed;
        }

        // Block cells in the grid
        for dz in 0..size.1 {
            for dx in 0..size.0 {
                let check_x = min_x + dx;
                let check_z = min_z + dz;
                grid.set_blocked(check_x, check_z, true);
            }
        }

        let target_scale = if def.model_path.is_some() {
            let scale_x = def.model_scale.unwrap_or(1.0);
            let scale_y = def.model_scale_y.unwrap_or(scale_x);
            let scale_z = def.model_scale.unwrap_or(1.0);
            Vec3::new(scale_x, scale_y, scale_z)
        } else {
            Vec3::new(size.0 as f32, 2.0, size.1 as f32)
        };

        let initial_scale = if def.model_path.is_some() {
            Vec3::new(target_scale.x, 0.1 * target_scale.y, target_scale.z)
        } else {
            Vec3::new(size.0 as f32, 0.1, size.1 as f32)
        };

        // Spawn building under Construction
        commands.spawn((
            Transform::from_translation(snapped_pos).with_scale(initial_scale),
            Building {
                building_id: cmd.building_id.clone(),
                health: def.health,
                max_health: def.health,
                armor: def.armor.unwrap_or(ArmorType::Wood),
            },
            Owner(cmd.player_id),
            Constructing {
                timer: 0.0,
                duration: 1.0,
                target_scale,
            },
            Vision {
                range: def.sight_radius,
            },
        ));

        println!("Placed {} for Team {}.", def.name, cmd.player_id);
    }
}

fn handle_train_unit_commands(
    mut events: MessageReader<TrainUnitCommand>,
    mut q_buildings: Query<(&Owner, &mut ProductionQueue, &Building)>,
    mut players: ResMut<Players>,
    definitions: Res<Definitions>,
    q_all_buildings: Query<(&Building, &Owner)>,
) {
    for cmd in events.read() {
        let Some(def) = definitions.units.get(&cmd.unit_id) else {
            continue;
        };

        let Ok((owner, mut queue, building)) = q_buildings.get_mut(cmd.building_entity) else {
            println!("TrainUnitCommand: Building entity not found or lacks production queue");
            continue;
        };

        if owner.0 != cmd.player_id {
            println!("TrainUnitCommand: Building team does not match command team");
            continue;
        }

        if !def.produced_by.contains(&building.building_id) {
            println!("TrainUnitCommand: Building cannot produce {}", def.name);
            continue;
        }

        // Requirements checks
        if let Some(reqs) = &def.requires {
            let mut has_all = true;
            for req in reqs {
                let mut found = false;
                for (b, t) in q_all_buildings.iter() {
                    if t.0 == cmd.player_id && &b.building_id == req {
                        found = true;
                        break;
                    }
                }
                if !found {
                    has_all = false;
                    break;
                }
            }
            if !has_all {
                println!(
                    "TrainUnitCommand: Requirements not met for {} (Team {})",
                    def.name, cmd.player_id
                );
                continue;
            }
        }

        let credits = get_player_credits(cmd.player_id, &players);
        if credits < def.cost {
            println!(
                "TrainUnitCommand: Not enough credits! Need {} (Team {})",
                def.cost, cmd.player_id
            );
            continue;
        }

        change_player_credits(cmd.player_id, -(def.cost as i32), &mut players);
        queue.queue.push(cmd.unit_id.clone());
        println!("Team {} queued {} in building", cmd.player_id, def.name);
    }
}

fn handle_cancel_unit_commands(
    mut events: MessageReader<CancelUnitCommand>,
    mut q_buildings: Query<(&Owner, &mut ProductionQueue, &Building)>,
    mut players: ResMut<Players>,
    definitions: Res<Definitions>,
) {
    for cmd in events.read() {
        let Some(def) = definitions.units.get(&cmd.unit_id) else {
            continue;
        };

        let Ok((owner, mut queue, _building)) = q_buildings.get_mut(cmd.building_entity) else {
            continue;
        };

        if owner.0 != cmd.player_id {
            continue;
        }

        if let Some(pos) = queue.queue.iter().rposition(|u| *u == cmd.unit_id) {
            queue.queue.remove(pos);
            change_player_credits(cmd.player_id, def.cost as i32, &mut players);
            println!(
                "Team {} canceled {} training, refunded {} credits",
                cmd.player_id, def.name, def.cost
            );
        }
    }
}

fn handle_consume_ready_commands(
    mut events: MessageReader<ConsumeReadyCommand>,
    mut team_queues: ResMut<TeamBuildingQueues>,
) {
    for cmd in events.read() {
        if let Some(queue) = team_queues.0.get_mut(&cmd.player_id) {
            if queue.ready.as_ref() == Some(&cmd.building_id) {
                queue.ready = None;
            }
        }
    }
}

fn handle_sell_building_commands(
    mut commands: Commands,
    mut events: MessageReader<SellBuildingCommand>,
    q_buildings: Query<(&Building, &Owner, &Transform)>,
    mut players: ResMut<Players>,
    definitions: Res<Definitions>,
    mut grid: ResMut<Grid>,
    mut power: ResMut<PowerSystem>,
    local_player: Res<crate::game::player::LocalPlayer>,
) {
    for cmd in events.read() {
        if let Ok((building, owner, transform)) = q_buildings.get(cmd.building_entity) {
            if owner.0 != cmd.player_id {
                continue;
            }

            if let Some(def) = definitions.buildings.get(&building.building_id) {
                // Refund 50% cost
                let refund = def.cost / 2;
                change_player_credits(cmd.player_id, refund as i32, &mut players);
                println!("Sold {} for {} credits", def.name, refund);

                // Unblock grid
                let size = def.size;
                let min_x = (transform.translation.x - size.0 as f32 / 2.0 + 0.5).round() as i32;
                let min_z = (transform.translation.z - size.1 as f32 / 2.0 + 0.5).round() as i32;
                for dz in 0..size.1 {
                    for dx in 0..size.0 {
                        let check_x = min_x + dx;
                        let check_z = min_z + dz;
                        grid.set_blocked(check_x, check_z, false);
                    }
                }

                // Adjust power if local player
                if owner.0 == local_player.0 {
                    power.produced -= def.power_produced;
                    power.consumed -= def.power_consumed;
                }

                // Despawn building
                if let Ok(mut cmds) = commands.get_entity(cmd.building_entity) {
                    cmds.despawn();
                }
            }
        }
    }
}

fn handle_set_rally_point_commands(
    mut events: MessageReader<SetRallyPointCommand>,
    mut q_buildings: Query<(&Owner, &mut crate::game::buildings::RallyPoint)>,
) {
    for cmd in events.read() {
        if let Ok((owner, mut rally_point)) = q_buildings.get_mut(cmd.building_entity) {
            if owner.0 == cmd.player_id {
                rally_point.0 = cmd.target_pos;
            }
        }
    }
}
