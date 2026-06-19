use crate::game::camera::RtsCamera;
use crate::game::commands::PlaceBuildingCommand;
use crate::game::data::Definitions;
use crate::game::economy::OreField;
use crate::game::game_state::{AppState, game_is_playing};
use crate::game::pathfinding::Grid;
use crate::game::player::{LocalPlayer, Players};
use crate::game::units::Unit;
use bevy::prelude::*;

pub struct PlacementPlugin;

impl Plugin for PlacementPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PlacementState {
            active: false,
            building_id: "".to_string(),
        })
        .add_systems(OnEnter(AppState::InGame), setup_ghost_assets)
        .add_systems(
            Update,
            (
                handle_placement_input,
                update_placement_ghost,
                handle_placement_click,
            )
                .run_if(game_is_playing)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

#[derive(Resource)]
pub struct PlacementState {
    pub active: bool,
    pub building_id: String,
}

#[derive(Component)]
pub struct PlacementGhost;

#[derive(Resource)]
pub struct GhostAssets {
    pub valid_mat: Handle<StandardMaterial>,
    pub invalid_mat: Handle<StandardMaterial>,
    pub cube_mesh: Handle<Mesh>,
}

fn setup_ghost_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(GhostAssets {
        valid_mat: materials.add(StandardMaterial {
            base_color: Color::srgba(0.0, 1.0, 0.0, 0.5),
            alpha_mode: AlphaMode::Blend,
            ..default()
        }),
        invalid_mat: materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.0, 0.0, 0.5),
            alpha_mode: AlphaMode::Blend,
            ..default()
        }),
        cube_mesh: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
    });
}

fn handle_placement_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    minimap_interaction: Option<Res<crate::game::ui::minimap::MinimapInteractionState>>,
    mut state: ResMut<PlacementState>,
    mut commands: Commands,
    q_ghost: Query<Entity, With<PlacementGhost>>,
    _definitions: Res<Definitions>,
    _players: Res<Players>,
    _local_player: Res<LocalPlayer>,
    console_data: Option<Res<crate::game::ui::console::ConsoleData>>,
) {
    if let Some(console) = console_data {
        if console.is_open {
            return;
        }
    }

    let mut changed = false;
    let cursor_over_minimap = minimap_interaction
        .as_deref()
        .is_some_and(|state| state.cursor_over || state.active);
    if keyboard.just_pressed(KeyCode::Escape)
        || (!cursor_over_minimap && mouse_buttons.just_pressed(MouseButton::Right))
    {
        state.active = false;
        changed = true;
    }

    if changed && !state.active {
        for entity in q_ghost.iter() {
            commands.entity(entity).despawn();
        }
    }
}

fn update_placement_ghost(
    mut commands: Commands,
    state: Res<PlacementState>,
    minimap_interaction: Option<Res<crate::game::ui::minimap::MinimapInteractionState>>,
    definitions: Res<Definitions>,
    q_window: Query<&Window>,
    q_camera: Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
    mut q_ghost: Query<
        (
            Entity,
            &mut Transform,
            &mut MeshMaterial3d<StandardMaterial>,
        ),
        (With<PlacementGhost>, Without<Unit>),
    >,
    grid: Res<Grid>,
    ghost_assets: Res<GhostAssets>,
    q_units: Query<&Transform, With<Unit>>,
    q_ore: Query<&Transform, (With<OreField>, Without<PlacementGhost>)>,
    q_buildings: Query<
        (
            &Transform,
            &crate::game::buildings::Building,
            &crate::game::units::Owner,
        ),
        Without<PlacementGhost>,
    >,
    local_player: Res<crate::game::player::LocalPlayer>,
) {
    if !state.active {
        for (entity, _, _) in q_ghost.iter() {
            commands.entity(entity).despawn();
        }
        return;
    }
    if minimap_interaction
        .as_deref()
        .is_some_and(|state| state.cursor_over || state.active)
    {
        for (entity, _, _) in q_ghost.iter() {
            commands.entity(entity).despawn();
        }
        return;
    }

    let Some(def) = definitions.buildings.get(&state.building_id) else {
        return;
    };

    let Ok(window) = q_window.single() else {
        return;
    };
    let Ok((camera, camera_transform)) = q_camera.single() else {
        return;
    };
    let Some(cursor_position) = window.cursor_position() else {
        return;
    };
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_position) else {
        return;
    };

    if ray.direction.y.abs() < 0.001 {
        return;
    }
    let t = -ray.origin.y / ray.direction.y;
    if t < 0.0 {
        return;
    }
    let intersection = ray.origin + *ray.direction * t;

    let min_x = (intersection.x - def.size.0 as f32 / 2.0 + 0.5).round() as i32;
    let min_z = (intersection.z - def.size.1 as f32 / 2.0 + 0.5).round() as i32;
    let snapped_x = min_x as f32 + def.size.0 as f32 / 2.0 - 0.5;
    let snapped_z = min_z as f32 + def.size.1 as f32 / 2.0 - 0.5;
    let snapped_pos = Vec3::new(snapped_x, 0.5, snapped_z);

    let size = def.size;

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

    // Ore is walkable for units, but it reserves its tile against construction.
    for ore_transform in q_ore.iter() {
        let ox = ore_transform.translation.x.round() as i32;
        let oz = ore_transform.translation.z.round() as i32;
        let max_x = min_x + size.0;
        let max_z = min_z + size.1;
        if ox >= min_x && ox < max_x && oz >= min_z && oz < max_z {
            is_blocked = true;
        }
    }

    // Check if within influence_radius of any existing building
    let mut in_influence = false;
    let mut has_buildings = false;
    for (b_transform, building, owner) in q_buildings.iter() {
        if owner.0 == local_player.0 {
            has_buildings = true;
            if let Some(b_def) = definitions.buildings.get(&building.building_id) {
                let b_min_x =
                    (b_transform.translation.x - b_def.size.0 as f32 / 2.0 + 0.5).round() as i32;
                let b_min_z =
                    (b_transform.translation.z - b_def.size.1 as f32 / 2.0 + 0.5).round() as i32;

                let b_max_x = b_min_x + b_def.size.0;
                let b_max_z = b_min_z + b_def.size.1;

                let g_max_x = min_x + size.0;
                let g_max_z = min_z + size.1;

                let dx = if g_max_x <= b_min_x {
                    b_min_x - g_max_x
                } else if min_x >= b_max_x {
                    min_x - b_max_x
                } else {
                    0
                };
                let dz = if g_max_z <= b_min_z {
                    b_min_z - g_max_z
                } else if min_z >= b_max_z {
                    min_z - b_max_z
                } else {
                    0
                };
                let dist = std::cmp::max(dx, dz);

                if dist <= b_def.influence_radius as i32 {
                    in_influence = true;
                    break;
                }
            }
        }
    }

    // Allow placement if it's the first building (e.g. CY) or if it's within influence radius
    if !in_influence && has_buildings {
        is_blocked = true;
    }

    let expected_mat = if is_blocked {
        ghost_assets.invalid_mat.clone()
    } else {
        ghost_assets.valid_mat.clone()
    };

    if q_ghost.is_empty() {
        commands.spawn((
            Mesh3d(ghost_assets.cube_mesh.clone()),
            MeshMaterial3d(expected_mat),
            Transform::from_translation(snapped_pos).with_scale(Vec3::new(
                size.0 as f32,
                1.0,
                size.1 as f32,
            )),
            PlacementGhost,
        ));
    } else if let Ok((_, mut transform, mut material)) = q_ghost.single_mut() {
        transform.translation = snapped_pos;
        transform.scale = Vec3::new(size.0 as f32, 1.0, size.1 as f32);
        if material.0 != expected_mat {
            material.0 = expected_mat;
        }
    }
}

fn handle_placement_click(
    mut commands: Commands,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    minimap_interaction: Option<Res<crate::game::ui::minimap::MinimapInteractionState>>,
    mut state: ResMut<PlacementState>,
    q_ghost: Query<(&Transform, &MeshMaterial3d<StandardMaterial>), With<PlacementGhost>>,
    q_ghost_entities: Query<Entity, With<PlacementGhost>>,
    mut place_building_events: MessageWriter<PlaceBuildingCommand>,
    q_window: Query<&Window>,
    ghost_assets: Res<GhostAssets>,
    local_player: Res<LocalPlayer>,
) {
    if !state.active || !mouse_buttons.just_pressed(MouseButton::Left) {
        return;
    }
    if minimap_interaction
        .as_deref()
        .is_some_and(|state| state.cursor_over || state.active)
    {
        return;
    }

    let Ok(window) = q_window.single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };
    if cursor_pos.x >= window.width() - 200.0 {
        return;
    }

    let Ok((transform, material)) = q_ghost.single() else {
        return;
    };

    if material.0 == ghost_assets.invalid_mat {
        println!("Cannot place building here: cell is blocked or occupied.");
        return;
    }

    place_building_events.write(PlaceBuildingCommand {
        player_id: local_player.0,
        building_id: state.building_id.clone(),
        position: transform.translation,
    });

    state.active = false;
    for entity in q_ghost_entities.iter() {
        commands.entity(entity).despawn();
    }
}
