use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

use crate::game::buildings::{Building, ProductionQueue, TeamBuildingQueues};
use crate::game::combat::AttackTarget;
use crate::game::commands::{
    AttackCommand, BuildCommand, MoveCommand, PlaceBuildingCommand, SellBuildingCommand,
    TrainUnitCommand,
};
use crate::game::data::Definitions;
use crate::game::game_state::{AppState, game_is_playing};
use crate::game::pathfinding::{Grid, Path};
use crate::game::player::{PlayerController, Players};
use crate::game::units::{Owner, Unit};

pub struct AiPlugin;

/// Emitted every time the AI timer ticks. Carries the Player ID.
#[derive(Message)]
pub struct AiTickEvent(pub usize);

impl Plugin for AiPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(AiTimer(Timer::from_seconds(1.0, TimerMode::Repeating)))
            .add_message::<AiTickEvent>()
            .insert_resource(EnemyIntelMap::default())
            .insert_resource(AiArmyControl::default())
            .insert_resource(ScoutAssignments::default())
            .insert_resource(DesiredArmyComposition {
                ratios: {
                    let mut m = HashMap::new();
                    m.insert("tank".to_string(), 0.6);
                    m.insert("infantry".to_string(), 0.4);
                    m
                },
            })
            .add_systems(
                Update,
                (
                    ai_ticker,
                    // Combat runs right after the tick fires so the rest of the
                    // brain (build priorities, production) acts on fresh intel.
                    ai_combat_commander,
                    ai_economy_and_placement_system,
                    ai_production_system,
                    ai_scouting_system,
                )
                    .chain()
                    .run_if(game_is_playing)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

#[derive(Resource)]
pub struct AiTimer(pub Timer);

// -------------------------------------------------------------------------
// Resources for smarter decision making
// -------------------------------------------------------------------------

/// What one AI has observed about its enemies. Sightings decay rather than
/// being wiped every tick, so a momentary loss of visibility doesn't cause
/// the tech-tree priorities to flicker.
#[derive(Default, Clone)]
pub struct EnemyIntel {
    pub has_infantry: bool,
    pub has_vehicle: bool,
    pub has_air: bool,
    infantry_ttl: u8,
    vehicle_ttl: u8,
    air_ttl: u8,
}

impl EnemyIntel {
    /// How many AI ticks (~seconds) a sighting is remembered for.
    const TTL_TICKS: u8 = 20;

    fn decay(&mut self) {
        if self.infantry_ttl > 0 {
            self.infantry_ttl -= 1;
        } else {
            self.has_infantry = false;
        }
        if self.vehicle_ttl > 0 {
            self.vehicle_ttl -= 1;
        } else {
            self.has_vehicle = false;
        }
        if self.air_ttl > 0 {
            self.air_ttl -= 1;
        } else {
            self.has_air = false;
        }
    }

    fn see_infantry(&mut self) {
        self.has_infantry = true;
        self.infantry_ttl = Self::TTL_TICKS;
    }
    fn see_vehicle(&mut self) {
        self.has_vehicle = true;
        self.vehicle_ttl = Self::TTL_TICKS;
    }
    fn see_air(&mut self) {
        self.has_air = true;
        self.air_ttl = Self::TTL_TICKS;
    }
}

/// Per-player enemy intel (keyed by AI player id, so multiple AIs don't
/// pollute each other's view of the world).
#[derive(Resource, Default)]
pub struct EnemyIntelMap(pub HashMap<usize, EnemyIntel>);

/// Desired fraction of each unit role in the army.
#[derive(Resource)]
pub struct DesiredArmyComposition {
    pub ratios: HashMap<String, f32>,
}

/// Persistent state machine driving each AI's field army, so attacks are a
/// repeating cycle (mass up -> push -> retreat & regroup if it goes badly)
/// instead of a single one-shot decision.
///
/// `Massing` carries a patience counter: how many ticks it's been waiting
/// to reach its ideal force size. Without this, an AI whose economy stalls
/// (e.g. it mines out all the ore on the map) can get stuck waiting forever
/// for a force size it will never reach again, and just sits there doing
/// nothing for the rest of the match.
#[derive(Clone)]
pub enum ArmyState {
    Massing { patience: u32 },
    Attacking { target: Entity, peak_size: usize },
    Retreating { rally_point: Vec3 },
}

impl ArmyState {
    fn fresh_massing() -> Self {
        ArmyState::Massing { patience: 0 }
    }
}

#[derive(Resource, Default)]
pub struct AiArmyControl {
    pub states: HashMap<usize, ArmyState>,
}

/// Tracks each scout's current destination so we don't reissue a brand new
/// random move order every single tick (which previously made scouts twitch
/// in place instead of actually exploring).
#[derive(Resource, Default)]
pub struct ScoutAssignments(pub HashMap<Entity, Vec3>);

// Tuning constants for the combat commander.
const GARRISON_SIZE: usize = 2;
const MIN_ATTACK_FORCE: usize = 4;
// Cap how large the "ideal" force can grow to, so a handful of neutral or
// far-away hostile units can't inflate the requirement into something the
// AI can never realistically reach.
const MAX_ATTACK_FORCE_REQUIREMENT: usize = MIN_ATTACK_FORCE * 3;
const HOME_DEFENSE_RANGE: f32 = 16.0;
const ENGAGE_RANGE: f32 = 16.0;
const RETREAT_LOSS_FRACTION: f32 = 0.45;
const REGROUP_DISTANCE: f32 = 10.0;
const CORNERED_RANGE: f32 = 10.0;
// How many ticks (~seconds) the AI will wait to mass an ideal force before
// attacking with whatever it currently has, so it's never stuck forever.
const NORMAL_MASS_PATIENCE_TICKS: u32 = 40;
// Once there's no ore left anywhere on the map, further waiting is pointless
// - the army will only ever get smaller from here - so give up much sooner.
const DESPERATION_PATIENCE_TICKS: u32 = 8;
// Never attack with nothing, but don't require much either once patience runs out.
const TIMEOUT_MIN_FORCE: usize = 2;

// -------------------------------------------------------------------------
// Helper Functions
// -------------------------------------------------------------------------

fn is_valid_placement(
    pos: Vec3,
    size: (i32, i32),
    grid: &Grid,
    unit_positions: &HashSet<(i32, i32)>,
    resource_positions: &HashSet<(i32, i32)>,
    building_extents: &[(Vec3, (i32, i32), u32)],
) -> bool {
    let min_x = (pos.x - size.0 as f32 / 2.0 + 0.5).round() as i32;
    let min_z = (pos.z - size.1 as f32 / 2.0 + 0.5).round() as i32;

    for dz in 0..size.1 {
        for dx in 0..size.0 {
            let check_x = min_x + dx;
            let check_z = min_z + dz;
            if grid.is_blocked(check_x, check_z) {
                return false;
            }
        }
    }

    let max_x = min_x + size.0;
    let max_z = min_z + size.1;
    for ux in min_x..max_x {
        for uz in min_z..max_z {
            if unit_positions.contains(&(ux, uz)) {
                return false;
            }
            if resource_positions.contains(&(ux, uz)) {
                return false;
            }
        }
    }

    if building_extents.is_empty() {
        return true;
    }

    let mut in_influence = false;
    for &(b_pos, b_size, b_radius) in building_extents {
        let b_min_x = (b_pos.x - b_size.0 as f32 / 2.0 + 0.5).round() as i32;
        let b_min_z = (b_pos.z - b_size.1 as f32 / 2.0 + 0.5).round() as i32;

        let b_max_x = b_min_x + b_size.0;
        let b_max_z = b_min_z + b_size.1;

        let dx = if max_x <= b_min_x {
            b_min_x - max_x
        } else if min_x >= b_max_x {
            min_x - b_max_x
        } else {
            0
        };
        let dz = if max_z <= b_min_z {
            b_min_z - max_z
        } else if min_z >= b_max_z {
            min_z - b_max_z
        } else {
            0
        };
        let dist = std::cmp::max(dx, dz);

        if dist <= b_radius as i32 {
            in_influence = true;
            break;
        }
    }

    in_influence
}

/// Find a good spot for a refinery near resource tiles.
fn find_best_refinery_spot(
    base_pos: Vec3,
    size: (i32, i32),
    grid: &Grid,
    unit_positions: &HashSet<(i32, i32)>,
    resource_positions: &HashSet<(i32, i32)>,
    building_extents: &[(Vec3, (i32, i32), u32)],
) -> Option<Vec3> {
    let bx = base_pos.x.round() as i32;
    let bz = base_pos.z.round() as i32;
    let mut best: Option<(Vec3, f32)> = None;

    // Search outward from the base
    for r in 6_i32..30_i32 {
        for dx in -r..=r {
            for dz in -r..=r {
                if dx.abs() != r && dz.abs() != r {
                    continue;
                }
                let tx = bx + dx;
                let tz = bz + dz;
                // Check if the tile itself is a resource (or adjacent to one)
                if !resource_positions.contains(&(tx, tz)) {
                    continue;
                }
                // Try placing the refinery adjacent to the resource tile
                for adj_dx in -1..=1 {
                    for adj_dz in -1..=1 {
                        let build_x = tx + adj_dx;
                        let build_z = tz + adj_dz;
                        let test_pos = Vec3::new(build_x as f32, 0.0, build_z as f32);
                        if is_valid_placement(
                            test_pos,
                            size,
                            grid,
                            unit_positions,
                            resource_positions,
                            building_extents,
                        ) {
                            let dist =
                                (test_pos.x - base_pos.x).abs() + (test_pos.z - base_pos.z).abs();
                            if best.is_none() || dist < best.unwrap().1 {
                                best = Some((test_pos, dist));
                            }
                        }
                    }
                }
            }
        }
        if best.is_some() {
            break;
        }
    }
    best.map(|(pos, _)| pos)
}

/// Determine which building to build next based on priority.
fn next_building_priority(
    _ai_id: usize,
    ai_buildings: &[String],
    player: &crate::game::player::Player,
    definitions: &Definitions,
    enemy_intel: &EnemyIntel,
    resources_available: bool,
) -> Option<String> {
    let faction = definitions.factions.get(&player.faction)?;
    let mut candidates: Vec<(String, f32)> = Vec::new();

    for b_id in &faction.buildings {
        let Some(def) = definitions.buildings.get(b_id) else {
            continue;
        };
        // Prerequisite check
        if let Some(reqs) = &def.requires {
            if !reqs.iter().all(|r| ai_buildings.contains(r)) {
                continue;
            }
        }

        // How many of this building do we already have?
        let count = ai_buildings
            .iter()
            .filter(|id| *id == b_id.as_str())
            .count();

        // Maximum allowed count for this building type
        let max_count = match def.role.as_deref() {
            Some("power") => 3,
            Some("refinery") => 4,
            Some("barracks") => 1,
            Some("war_factory") => 2,
            _ => 1,
        };
        if count >= max_count {
            continue;
        }

        // No point building more refineries once the map's ore is gone -
        // without this the AI can burn its remaining credits on an
        // income building that will never pay for itself.
        if def.role.as_deref() == Some("refinery") && !resources_available {
            continue;
        }

        // Base priority
        let mut priority = 0.0;
        match def.role.as_deref() {
            Some("power") => priority = 50.0 - (count as f32 * 10.0), // less urgent if we have many
            Some("refinery") => {
                // Higher priority early game, lower when we have enough
                priority = 80.0 - (count as f32 * 20.0);
            }
            Some("barracks") => {
                if count == 0 {
                    priority = 85.0; // Essential for tech tree unlocking
                } else if enemy_intel.has_infantry {
                    priority = 60.0;
                } else {
                    priority = 20.0;
                }
            }
            Some("war_factory") => {
                if count == 0 {
                    priority = 90.0;
                } else {
                    priority = 20.0; // second factory only if we swim in cash
                }
            }
            _ => {}
        }

        // Factor in credit cost – don’t queue if we can’t afford it soon
        if player.credits < def.cost {
            priority *= 0.5;
        }

        candidates.push((b_id.clone(), priority));
    }

    candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    candidates.first().map(|(id, _)| id.clone())
}

/// Choose the next unit to train based on desired composition.
fn next_unit_to_train(
    building_id: &str,
    faction: &crate::game::data::FactionDefinition,
    definitions: &Definitions,
    current_role_counts: &HashMap<String, usize>,
    target_ratios: &HashMap<String, f32>,
    enemy_intel: &EnemyIntel,
) -> Option<String> {
    let mut best: Option<(String, f32)> = None;
    let combat_total = current_role_counts
        .iter()
        .filter(|(role, _)| is_combat_role(Some(role.as_str())))
        .map(|(_, count)| *count)
        .sum::<usize>()
        .max(1);

    for unit_id in &faction.units {
        let Some(def) = definitions.units.get(unit_id) else {
            continue;
        };

        // Ensure this building can produce this unit
        if !def.produced_by.contains(&building_id.to_string()) {
            continue;
        }

        let role = def.role.clone().unwrap_or_default();
        let Some(mut desired_ratio) = target_ratios
            .get(&role)
            .copied()
            .or_else(|| is_combat_role(Some(&role)).then_some(0.2))
        else {
            continue;
        };

        if role == "infantry" && enemy_intel.has_infantry {
            desired_ratio += 0.15;
        }
        if role == "tank" && enemy_intel.has_vehicle {
            desired_ratio += 0.2;
        }
        if role == "anti_infantry" && enemy_intel.has_infantry {
            desired_ratio += 0.25;
        }

        let current = *current_role_counts.get(&role).unwrap_or(&0) as f32;
        let desired = desired_ratio * combat_total as f32;
        let deficit = (desired - current).max(0.0);
        let weapon_value = def
            .weapon
            .as_ref()
            .map(|weapon| weapon.damage * weapon.range / weapon.cooldown.max(0.1))
            .unwrap_or(0.0);
        let durability_value = def.health / 100.0;
        let value_per_credit = (weapon_value + durability_value) / def.cost.max(1) as f32;
        let score = deficit * 10.0 + value_per_credit;

        if best
            .as_ref()
            .map_or(true, |(_, best_score)| score > *best_score)
        {
            best = Some((unit_id.clone(), score));
        }
    }
    best.map(|(id, _)| id)
}

fn target_score(target_pos: Vec3, priority: i32, field_center: Vec3) -> f32 {
    priority as f32 * 100.0 - field_center.distance(target_pos)
}

fn best_attack_target(
    enemy_targets: &[(Entity, Vec3, i32)],
    field: &[(Entity, Vec3, Option<Entity>)],
) -> Option<Entity> {
    if enemy_targets.is_empty() || field.is_empty() {
        return None;
    }

    let field_center = field
        .iter()
        .map(|(_, pos, _)| *pos)
        .reduce(|a, b| a + b)
        .unwrap_or(Vec3::ZERO)
        / field.len() as f32;

    enemy_targets
        .iter()
        .max_by(|a, b| {
            target_score(a.1, a.2, field_center)
                .partial_cmp(&target_score(b.1, b.2, field_center))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(entity, _, _)| *entity)
}

/// Whether a unit role counts as a combat unit eligible for garrison/strike duty.
fn is_combat_role(role: Option<&str>) -> bool {
    matches!(
        role,
        Some("tank") | Some("infantry") | Some("anti_infantry") | Some("artillery")
    )
}

// -------------------------------------------------------------------------
// 1. AI Ticker System
// -------------------------------------------------------------------------
fn ai_ticker(
    time: Res<Time>,
    mut ai_timer: ResMut<AiTimer>,
    players: Res<Players>,
    mut ev_ai_tick: MessageWriter<AiTickEvent>,
) {
    if !ai_timer.0.tick(time.delta()).just_finished() {
        return;
    }

    for player in players.players.values() {
        if matches!(player.controller, PlayerController::AI) {
            ev_ai_tick.write(AiTickEvent(player.id));
        }
    }
}

// -------------------------------------------------------------------------
// 2. Economy & Placement System
// -------------------------------------------------------------------------
fn ai_economy_and_placement_system(
    mut ev_ai_tick: MessageReader<AiTickEvent>,
    q_buildings: Query<(Entity, &Building, &Owner, &Transform)>,
    q_units: Query<(&Unit, &Transform)>,
    q_ore: Query<&Transform, With<crate::game::economy::OreField>>,
    definitions: Res<Definitions>,
    players: Res<Players>,
    team_queues: Res<TeamBuildingQueues>,
    grid: Res<Grid>,
    enemy_intel_map: Res<EnemyIntelMap>,
    mut place_events: MessageWriter<PlaceBuildingCommand>,
    mut build_events: MessageWriter<BuildCommand>,
    mut _sell_events: MessageWriter<SellBuildingCommand>,
) {
    let default_intel = EnemyIntel::default();

    for tick in ev_ai_tick.read() {
        let ai_id = tick.0;
        let Some(player) = players.players.get(&ai_id) else {
            continue;
        };
        let enemy_intel = enemy_intel_map.0.get(&ai_id).unwrap_or(&default_intel);

        let mut unit_positions = HashSet::new();
        for (_, transform) in q_units.iter() {
            let ux = transform.translation.x.round() as i32;
            let uz = transform.translation.z.round() as i32;
            unit_positions.insert((ux, uz));
        }

        let mut resource_positions = HashSet::new();
        for transform in q_ore.iter() {
            let ox = transform.translation.x.round() as i32;
            let oz = transform.translation.z.round() as i32;
            resource_positions.insert((ox, oz));
        }
        let resources_available = !resource_positions.is_empty();

        let mut ai_buildings = Vec::new();
        let mut ai_building_extents = Vec::new();
        let mut construction_yard_pos = None;

        for (_entity, building, owner, transform) in q_buildings.iter() {
            if owner.0 == ai_id {
                ai_buildings.push(building.building_id.clone());
                if let Some(def) = definitions.buildings.get(&building.building_id) {
                    ai_building_extents.push((
                        transform.translation,
                        def.size,
                        def.influence_radius,
                    ));
                    if def.role.as_deref() == Some("construction_yard") {
                        construction_yard_pos = Some(transform.translation);
                    }
                }
            }
        }

        let base_pos = construction_yard_pos.unwrap_or(Vec3::new(108.0, 1.0, 108.0));
        let ai_queue = team_queues.0.get(&ai_id);

        if let Some(queue) = ai_queue {
            // PLACEMENT LOGIC
            if let Some(ready_id) = queue.ready.clone() {
                let Some(def) = definitions.buildings.get(&ready_id) else {
                    continue;
                };

                let spawn_pos = if def.role.as_deref() == Some("refinery") {
                    find_best_refinery_spot(
                        base_pos,
                        def.size,
                        &grid,
                        &unit_positions,
                        &resource_positions,
                        &ai_building_extents,
                    )
                    .unwrap_or(base_pos + Vec3::new(-5.0, 0.0, 0.0))
                } else {
                    // Other buildings: offset from construction yard, searching outward
                    let offset = if def.role.as_deref() == Some("power") {
                        Vec3::new(5.0, 0.0, 0.0)
                    } else {
                        Vec3::new(-5.0, 0.0, 0.0)
                    };
                    base_pos + offset
                };

                let mut final_spawn_pos = spawn_pos;
                if !is_valid_placement(
                    spawn_pos,
                    def.size,
                    &grid,
                    &unit_positions,
                    &resource_positions,
                    &ai_building_extents,
                ) {
                    let mut found_pos = None;
                    let bx = base_pos.x.round() as i32;
                    let bz = base_pos.z.round() as i32;
                    'search: for r in 4_i32..30_i32 {
                        for dx in -r..=r {
                            for dz in -r..=r {
                                if dx.abs() != r && dz.abs() != r {
                                    continue;
                                }
                                let test_pos = Vec3::new((bx + dx) as f32, 0.0, (bz + dz) as f32);
                                if is_valid_placement(
                                    test_pos,
                                    def.size,
                                    &grid,
                                    &unit_positions,
                                    &resource_positions,
                                    &ai_building_extents,
                                ) {
                                    found_pos = Some(test_pos);
                                    break 'search;
                                }
                            }
                        }
                    }
                    if let Some(pos) = found_pos {
                        final_spawn_pos = pos;
                    } else {
                        println!("AI {} cannot find valid placement for {}", ai_id, def.name);
                        continue;
                    }
                }

                place_events.write(PlaceBuildingCommand {
                    player_id: ai_id,
                    building_id: ready_id.clone(),
                    position: final_spawn_pos,
                });
            }
            // QUEUEING LOGIC
            else if queue.current.is_none() {
                if let Some(b_id) = next_building_priority(
                    ai_id,
                    &ai_buildings,
                    player,
                    &definitions,
                    enemy_intel,
                    resources_available,
                ) {
                    if let Some(def) = definitions.buildings.get(&b_id) {
                        if player.credits >= def.cost {
                            build_events.write(BuildCommand {
                                player_id: ai_id,
                                building_id: b_id,
                            });
                        }
                    }
                }
            }
        } else {
            // Initial queue creation (unchanged)
            if let Some(faction) = definitions.factions.get(&player.faction) {
                if let Some(power_id) = faction.buildings.iter().find(|b| {
                    definitions
                        .buildings
                        .get(*b)
                        .map_or(false, |def| def.role.as_deref() == Some("power"))
                }) {
                    build_events.write(BuildCommand {
                        player_id: ai_id,
                        building_id: power_id.to_string(),
                    });
                }
            }
        }
    }
}

// -------------------------------------------------------------------------
// 3. Production System
// -------------------------------------------------------------------------
fn ai_production_system(
    mut ev_ai_tick: MessageReader<AiTickEvent>,
    q_buildings: Query<(Entity, &Building, &Owner, Option<&ProductionQueue>)>,
    definitions: Res<Definitions>,
    players: Res<Players>,
    desired_comp: Res<DesiredArmyComposition>,
    enemy_intel_map: Res<EnemyIntelMap>,
    q_units: Query<(&Unit, &Owner)>,
    mut train_events: MessageWriter<TrainUnitCommand>,
) {
    let default_intel = EnemyIntel::default();

    for tick in ev_ai_tick.read() {
        let ai_id = tick.0;
        let Some(player) = players.players.get(&ai_id) else {
            continue;
        };
        let enemy_intel = enemy_intel_map.0.get(&ai_id).unwrap_or(&default_intel);

        let mut current_role_counts: HashMap<String, usize> = HashMap::new();
        for (unit, owner) in q_units.iter() {
            if owner.0 == ai_id {
                if let Some(def) = definitions.units.get(&unit.unit_id) {
                    if let Some(role) = &def.role {
                        *current_role_counts.entry(role.clone()).or_default() += 1;
                    }
                }
            }
        }

        for (ent, building, owner, queue_opt) in q_buildings.iter() {
            if owner.0 != ai_id {
                continue;
            }

            if let Some(def) = definitions.buildings.get(&building.building_id) {
                // Only produce from factories (war factory, barracks, etc.)
                if def.role.as_deref() == Some("war_factory")
                    || def.role.as_deref() == Some("barracks")
                {
                    if let Some(queue) = queue_opt {
                        // Keep a small queue buffer (2 items per factory)
                        if queue.queue.len() < 2 {
                            if let Some(faction) = definitions.factions.get(&player.faction) {
                                if let Some(unit_id) = next_unit_to_train(
                                    &building.building_id,
                                    faction,
                                    &definitions,
                                    &current_role_counts,
                                    &desired_comp.ratios,
                                    enemy_intel,
                                ) {
                                    if let Some(unit_def) = definitions.units.get(&unit_id) {
                                        if player.credits >= unit_def.cost {
                                            train_events.write(TrainUnitCommand {
                                                player_id: ai_id,
                                                building_entity: ent,
                                                unit_id: unit_id.clone(),
                                            });
                                            // Update our local count to avoid over‑queuing in the same tick
                                            if let Some(role) = &unit_def.role {
                                                *current_role_counts
                                                    .entry(role.clone())
                                                    .or_default() += 1;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// -------------------------------------------------------------------------
// 4. Combat Commander System
// -------------------------------------------------------------------------
//
// Each AI's combat units are split every tick into:
//   - a small home garrison (closest few units to the construction yard),
//     which always defends itself independently, and
//   - a field force, which is governed by a persistent per-player state
//     machine: Massing -> Attacking -> Retreating -> Massing -> ...
//
// This means defending the base never blocks the offensive brain (the old
// bug), reinforcements automatically flow into an ongoing attack (since the
// field force is recomputed from the live unit list every tick), and the AI
// pulls back and regroups instead of fighting to the last unit.
fn ai_combat_commander(
    mut ev_ai_tick: MessageReader<AiTickEvent>,
    q_units: Query<(Entity, &Unit, &Owner, &Transform, Option<&AttackTarget>)>,
    q_moving_units: Query<(), With<Path>>,
    q_buildings: Query<(Entity, &Building, &Owner, &Transform)>,
    q_ore: Query<&Transform, With<crate::game::economy::OreField>>,
    definitions: Res<Definitions>,
    mut enemy_intel_map: ResMut<EnemyIntelMap>,
    mut army_control: ResMut<AiArmyControl>,
    mut attack_events: MessageWriter<AttackCommand>,
    mut move_events: MessageWriter<MoveCommand>,
) {
    for tick in ev_ai_tick.read() {
        let ai_id = tick.0;
        let resources_available = !q_ore.is_empty();

        // Decay this AI's memory of the enemy before refreshing it below.
        let intel = enemy_intel_map.0.entry(ai_id).or_default();
        intel.decay();

        // --- Gather friendlies & enemies ------------------------------------
        let mut ai_combat_units: Vec<(Entity, Vec3, Option<Entity>)> = Vec::new();
        let mut enemy_targets: Vec<(Entity, Vec3, i32)> = Vec::new();
        let mut enemy_combat_strength: usize = 0;

        for (ent, unit, owner, transform, attack_target) in q_units.iter() {
            let Some(def) = definitions.units.get(&unit.unit_id) else {
                continue;
            };
            if owner.0 == ai_id {
                if is_combat_role(def.role.as_deref()) {
                    ai_combat_units.push((
                        ent,
                        transform.translation,
                        attack_target.map(|target| target.0),
                    ));
                }
            } else {
                match def.role.as_deref() {
                    Some("infantry") | Some("anti_infantry") => intel.see_infantry(),
                    Some("tank") | Some("vehicle") => intel.see_vehicle(),
                    Some("air") => intel.see_air(),
                    _ => {}
                }
                if is_combat_role(def.role.as_deref()) {
                    enemy_combat_strength += 1;
                }
                enemy_targets.push((ent, transform.translation, 10));
            }
        }

        let mut cy_pos = None;
        for (ent, building, owner, transform) in q_buildings.iter() {
            if owner.0 == ai_id {
                if let Some(def) = definitions.buildings.get(&building.building_id) {
                    if def.role.as_deref() == Some("construction_yard") && cy_pos.is_none() {
                        cy_pos = Some(transform.translation);
                    }
                }
            } else {
                let mut priority = 50;
                if let Some(def) = definitions.buildings.get(&building.building_id) {
                    if def.role.as_deref() == Some("construction_yard") {
                        priority = 100;
                    } else if matches!(def.role.as_deref(), Some("turret") | Some("defense")) {
                        priority = 75;
                    }
                }
                enemy_targets.push((ent, transform.translation, priority));
            }
        }
        let cy_pos = cy_pos.unwrap_or(Vec3::new(108.0, 1.0, 108.0));

        if ai_combat_units.is_empty() {
            continue;
        }

        // --- Split into home garrison vs field force ------------------------
        let mut by_distance = ai_combat_units.clone();
        by_distance.sort_by(|a, b| {
            a.1.distance(cy_pos)
                .partial_cmp(&b.1.distance(cy_pos))
                .unwrap()
        });
        let garrison_count = GARRISON_SIZE.min(by_distance.len());
        let garrison: Vec<(Entity, Vec3, Option<Entity>)> = by_distance[..garrison_count].to_vec();
        let field: Vec<(Entity, Vec3, Option<Entity>)> = by_distance[garrison_count..].to_vec();

        // --- Garrison: defend the base individually --------------------------
        for (garrison_ent, garrison_pos, current_target) in &garrison {
            if current_target.is_some() {
                continue;
            }
            if let Some((target_ent, _, _)) = enemy_targets
                .iter()
                .filter(|(_, epos, _)| epos.distance(*garrison_pos) < HOME_DEFENSE_RANGE)
                .min_by(|a, b| {
                    a.1.distance(*garrison_pos)
                        .partial_cmp(&b.1.distance(*garrison_pos))
                        .unwrap()
                })
            {
                attack_events.write(AttackCommand {
                    player_id: ai_id,
                    unit_entities: vec![*garrison_ent],
                    target_entity: *target_ent,
                });
            }
        }

        // --- Field force: governed by the attack state machine ---------------
        let state = army_control
            .states
            .entry(ai_id)
            .or_insert_with(ArmyState::fresh_massing)
            .clone();

        match state {
            ArmyState::Massing { mut patience } => {
                patience += 1;
                army_control
                    .states
                    .insert(ai_id, ArmyState::Massing { patience });

                // Let isolated field units fight back if jumped while massing.
                for (field_ent, field_pos, current_target) in &field {
                    if current_target.is_some() {
                        continue;
                    }
                    if let Some((target_ent, _, _)) = enemy_targets
                        .iter()
                        .filter(|(_, epos, _)| epos.distance(*field_pos) < ENGAGE_RANGE)
                        .min_by(|a, b| {
                            a.1.distance(*field_pos)
                                .partial_cmp(&b.1.distance(*field_pos))
                                .unwrap()
                        })
                    {
                        attack_events.write(AttackCommand {
                            player_id: ai_id,
                            unit_entities: vec![*field_ent],
                            target_entity: *target_ent,
                        });
                    }
                }

                let mut required = MIN_ATTACK_FORCE
                    .max((enemy_combat_strength as f32 * 0.8) as usize)
                    .min(MAX_ATTACK_FORCE_REQUIREMENT);

                let max_patience = if resources_available {
                    NORMAL_MASS_PATIENCE_TICKS
                } else {
                    DESPERATION_PATIENCE_TICKS
                };

                if patience >= max_patience {
                    required = TIMEOUT_MIN_FORCE;
                }

                if field.len() >= required && !enemy_targets.is_empty() {
                    let Some(target) = best_attack_target(&enemy_targets, &field) else {
                        continue;
                    };

                    let unit_entities: Vec<Entity> = field.iter().map(|(e, _, _)| *e).collect();
                    attack_events.write(AttackCommand {
                        player_id: ai_id,
                        unit_entities,
                        target_entity: target,
                    });

                    army_control.states.insert(
                        ai_id,
                        ArmyState::Attacking {
                            target,
                            peak_size: field.len(),
                        },
                    );
                } else if !field.is_empty() {
                    // Rally scattered field units to a staging point near home so
                    // the strike force masses together instead of trickling in.
                    let staging_point = cy_pos + Vec3::new(-10.0, 0.0, -10.0);
                    let scattered: Vec<Entity> = field
                        .iter()
                        .filter(|(entity, pos, target)| {
                            target.is_none()
                                && !q_moving_units.contains(*entity)
                                && pos.distance(staging_point) > 6.0
                        })
                        .map(|(e, _, _)| *e)
                        .collect();
                    if !scattered.is_empty() {
                        move_events.write(MoveCommand {
                            player_id: ai_id,
                            unit_entities: scattered,
                            target_pos: Vec2::new(staging_point.x, staging_point.z),
                        });
                    }
                }
            }
            ArmyState::Attacking { target, peak_size } => {
                let current_size = field.len();
                let new_peak = peak_size.max(current_size);

                let target_alive = enemy_targets.iter().any(|(e, _, _)| *e == target);
                let effective_target = if target_alive {
                    Some(target)
                } else if !enemy_targets.is_empty() {
                    best_attack_target(&enemy_targets, &field)
                } else {
                    None
                };

                if field.is_empty() || effective_target.is_none() {
                    // Wiped out, or nothing left standing - stand down and remass.
                    army_control
                        .states
                        .insert(ai_id, ArmyState::fresh_massing());
                } else if new_peak >= MIN_ATTACK_FORCE
                    && (current_size as f32) < (new_peak as f32 * RETREAT_LOSS_FRACTION)
                {
                    // Taking heavy losses - pull back and regroup rather than die.
                    let unit_entities: Vec<Entity> = field.iter().map(|(e, _, _)| *e).collect();
                    move_events.write(MoveCommand {
                        player_id: ai_id,
                        unit_entities,
                        target_pos: Vec2::new(cy_pos.x, cy_pos.z),
                    });
                    army_control.states.insert(
                        ai_id,
                        ArmyState::Retreating {
                            rally_point: cy_pos,
                        },
                    );
                } else {
                    // Keep pushing - reinforcements that have joined the field
                    // force since the attack started are automatically included.
                    // Existing engagements are intentionally left alone. Only
                    // reinforcements without a target need the strategic order.
                    let unit_entities: Vec<Entity> = field
                        .iter()
                        .filter(|(_, _, current_target)| current_target.is_none())
                        .map(|(e, _, _)| *e)
                        .collect();
                    let target = effective_target.unwrap();
                    if !unit_entities.is_empty() {
                        attack_events.write(AttackCommand {
                            player_id: ai_id,
                            unit_entities,
                            target_entity: target,
                        });
                    }
                    army_control.states.insert(
                        ai_id,
                        ArmyState::Attacking {
                            target,
                            peak_size: new_peak,
                        },
                    );
                }
            }
            ArmyState::Retreating { rally_point } => {
                if field.is_empty() {
                    army_control
                        .states
                        .insert(ai_id, ArmyState::fresh_massing());
                } else {
                    // Cornered units fight back instead of dying mid-retreat;
                    // everyone else keeps heading for the rally point.
                    let mut still_fleeing: Vec<Entity> = Vec::new();
                    for (ent, pos, current_target) in &field {
                        if current_target.is_some() {
                            continue;
                        }
                        if let Some((target_ent, _, _)) = enemy_targets
                            .iter()
                            .filter(|(_, epos, _)| epos.distance(*pos) < CORNERED_RANGE)
                            .min_by(|a, b| {
                                a.1.distance(*pos).partial_cmp(&b.1.distance(*pos)).unwrap()
                            })
                        {
                            attack_events.write(AttackCommand {
                                player_id: ai_id,
                                unit_entities: vec![*ent],
                                target_entity: *target_ent,
                            });
                        } else if !q_moving_units.contains(*ent) {
                            still_fleeing.push(*ent);
                        }
                    }

                    let avg_dist: f32 = field
                        .iter()
                        .map(|(_, p, _)| p.distance(rally_point))
                        .sum::<f32>()
                        / field.len() as f32;

                    if avg_dist <= REGROUP_DISTANCE {
                        army_control
                            .states
                            .insert(ai_id, ArmyState::fresh_massing());
                    } else if !still_fleeing.is_empty() {
                        move_events.write(MoveCommand {
                            player_id: ai_id,
                            unit_entities: still_fleeing,
                            target_pos: Vec2::new(rally_point.x, rally_point.z),
                        });
                    }
                }
            }
        }
    }
}

// -------------------------------------------------------------------------
// 5. Scouting System
// -------------------------------------------------------------------------
//
// Gives each scout a destination and only reassigns a new one once it
// arrives (or never had one), instead of overwriting its move order with a
// fresh random point every tick - which previously made scouts jitter on
// the spot rather than actually explore the map.
fn ai_scouting_system(
    mut ev_ai_tick: MessageReader<AiTickEvent>,
    q_units: Query<(Entity, &Unit, &Owner, &Transform)>,
    definitions: Res<Definitions>,
    mut scout_assignments: ResMut<ScoutAssignments>,
    mut move_events: MessageWriter<MoveCommand>,
) {
    const ARRIVAL_DIST: f32 = 6.0;

    for tick in ev_ai_tick.read() {
        let ai_id = tick.0;

        for (ent, unit, owner, transform) in q_units.iter() {
            if owner.0 != ai_id {
                continue;
            }
            let Some(def) = definitions.units.get(&unit.unit_id) else {
                continue;
            };
            if def.role.as_deref() != Some("scout") {
                continue;
            }

            let pos = transform.translation;
            let needs_new_target = match scout_assignments.0.get(&ent) {
                Some(target) => pos.distance(*target) < ARRIVAL_DIST,
                None => true,
            };

            if needs_new_target {
                // Pick a random point on the map (adjust bounds to your map size).
                let rx = 20.0 + (rand::random::<f32>() * 200.0); // assumes map ~ 240x240
                let rz = 20.0 + (rand::random::<f32>() * 200.0);
                let target = Vec3::new(rx, 0.0, rz);

                scout_assignments.0.insert(ent, target);
                move_events.write(MoveCommand {
                    player_id: ai_id,
                    unit_entities: vec![ent],
                    target_pos: Vec2::new(target.x, target.z),
                });
            }
        }
    }

    // Prune assignments for scouts that died/were sold so the map doesn't grow forever.
    scout_assignments
        .0
        .retain(|ent, _| q_units.get(*ent).is_ok());
}
