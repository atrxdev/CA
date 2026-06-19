use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::RelativeCursorPosition;
use bevy::window::PrimaryWindow;
use std::collections::HashSet;

use crate::game::buildings::Building;
use crate::game::camera::{RTS_CAMERA_VIEW_HEIGHT, RtsCamera, clamp_camera_to_map_bounds};
use crate::game::commands::{MoveCommand, SetRallyPointCommand};
use crate::game::data::Definitions;
use crate::game::economy::OreField;
use crate::game::fog_of_war::{FogOfWar, VisibilityState};
use crate::game::game_state::AppState;
use crate::game::map::{MapBounds, MinimapData};
use crate::game::player::{LocalPlayer, Players};
use crate::game::selection::Selected;
use crate::game::units::{Owner, Unit};

const MINIMAP_SIZE: f32 = 184.0;
const UNIT_PIP_SIZE: f32 = 3.0;
const BUILDING_PIP_SIZE: f32 = 5.0;
const ORE_PIP_SIZE: f32 = 2.0;

pub struct MinimapPlugin;

impl Plugin for MinimapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MinimapInteractionState>().add_systems(
            Update,
            (
                update_minimap_interaction_state,
                update_minimap_terrain,
                update_minimap_pips,
                update_minimap_viewport,
                handle_minimap_clicks,
                handle_minimap_secondary_click,
            )
                .run_if(in_state(AppState::InGame)),
        );
    }
}

#[derive(Component)]
pub struct MinimapPanel;

#[derive(Resource, Default)]
pub struct MinimapInteractionState {
    pub cursor_over: bool,
    pub active: bool,
    pub normalized_cursor: Option<Vec2>,
}

#[derive(Component)]
struct MinimapTerrainImage;

#[derive(Component)]
struct MinimapPipLayer;

#[derive(Component)]
struct MinimapViewport;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum MinimapPipKind {
    Unit,
    Building,
    Ore,
}

#[derive(Component)]
struct MinimapPip {
    target: Entity,
    kind: MinimapPipKind,
}

pub fn spawn_minimap(parent: &mut ChildSpawnerCommands) {
    parent
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(200.0),
                padding: UiRect::all(Val::Px(8.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgb(0.04, 0.045, 0.035)),
            BorderColor::all(Color::srgb(0.42, 0.46, 0.36)),
        ))
        .with_children(|frame| {
            frame
                .spawn((
                    Button,
                    Node {
                        width: Val::Px(MINIMAP_SIZE),
                        height: Val::Px(MINIMAP_SIZE),
                        position_type: PositionType::Relative,
                        overflow: Overflow::clip(),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::BLACK),
                    BorderColor::all(Color::srgb(0.08, 0.12, 0.08)),
                    MinimapPanel,
                    RelativeCursorPosition::default(),
                ))
                .with_children(|map| {
                    map.spawn((
                        ImageNode::default(),
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(0.0),
                            top: Val::Px(0.0),
                            width: Val::Percent(100.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        MinimapTerrainImage,
                    ));
                    map.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(0.0),
                            top: Val::Px(0.0),
                            width: Val::Percent(100.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        MinimapPipLayer,
                    ));
                    map.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(0.0),
                            top: Val::Px(0.0),
                            width: Val::Percent(20.0),
                            height: Val::Percent(20.0),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.04)),
                        BorderColor::all(Color::WHITE),
                        ZIndex(10),
                        MinimapViewport,
                    ));
                });
        });
}

fn update_minimap_terrain(
    minimap: Option<Res<MinimapData>>,
    fog: Option<Res<FogOfWar>>,
    mut images: ResMut<Assets<Image>>,
    mut q_image: Query<&mut ImageNode, With<MinimapTerrainImage>>,
    mut last_size: Local<Option<(u32, u32)>>,
) {
    let Some(minimap) = minimap else {
        return;
    };
    let fog_changed = fog.as_ref().is_some_and(|fog| fog.is_changed());
    if !minimap.is_changed() && !fog_changed && *last_size == Some((minimap.width, minimap.height))
    {
        return;
    }

    let Ok(mut image_node) = q_image.single_mut() else {
        return;
    };

    let mut pixels = Vec::with_capacity((minimap.width * minimap.height * 4) as usize);
    for map_y in 0..minimap.height {
        for map_x in 0..minimap.width {
            let index = (map_y * minimap.width + map_x) as usize;
            let state = fog
                .as_deref()
                .map(|fog| minimap_fog_state(fog, &minimap, map_x, map_y))
                .unwrap_or(VisibilityState::Visible);
            let color = terrain_color_for_visibility(minimap.terrain_colors[index], state);
            let srgba = color.to_srgba();
            pixels.push(float_to_u8(srgba.red));
            pixels.push(float_to_u8(srgba.green));
            pixels.push(float_to_u8(srgba.blue));
            pixels.push(255);
        }
    }

    let image = Image::new(
        Extent3d {
            width: minimap.width,
            height: minimap.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );

    image_node.image = images.add(image);
    *last_size = Some((minimap.width, minimap.height));
}

fn update_minimap_pips(
    mut commands: Commands,
    bounds: Option<Res<MapBounds>>,
    definitions: Res<Definitions>,
    fog: Option<Res<FogOfWar>>,
    local_player: Res<LocalPlayer>,
    players: Res<Players>,
    q_layer: Query<Entity, With<MinimapPipLayer>>,
    q_units: Query<
        (Entity, &Transform, &Owner),
        (With<Unit>, Without<Building>, Without<OreField>),
    >,
    q_buildings: Query<
        (Entity, &Transform, &Owner),
        (With<Building>, Without<Unit>, Without<OreField>),
    >,
    q_ore: Query<
        (Entity, &Transform, &OreField),
        (With<OreField>, Without<Unit>, Without<Building>),
    >,
    mut q_pips: Query<(Entity, &MinimapPip, &mut Node, &mut BackgroundColor)>,
) {
    let Some(bounds) = bounds.as_deref() else {
        return;
    };
    let Ok(layer) = q_layer.single() else {
        return;
    };

    let mut existing = HashSet::new();
    for (pip_entity, pip, mut node, mut color) in &mut q_pips {
        let maybe_data = match pip.kind {
            MinimapPipKind::Unit => {
                q_units
                    .get(pip.target)
                    .ok()
                    .and_then(|(_, transform, owner)| {
                        pip_data_for_unit(
                            transform.translation,
                            owner.0,
                            &players,
                            local_player.0,
                            fog.as_deref(),
                        )
                    })
            }
            MinimapPipKind::Building => {
                q_buildings
                    .get(pip.target)
                    .ok()
                    .and_then(|(_, transform, owner)| {
                        pip_data_for_building(
                            transform.translation,
                            owner.0,
                            &players,
                            local_player.0,
                            fog.as_deref(),
                        )
                    })
            }
            MinimapPipKind::Ore => q_ore.get(pip.target).ok().and_then(|(_, transform, ore)| {
                pip_data_for_ore(transform.translation, ore, &definitions, fog.as_deref())
            }),
        };

        let Some((position, pip_color, size)) = maybe_data else {
            if q_units.get(pip.target).is_err()
                && q_buildings.get(pip.target).is_err()
                && q_ore.get(pip.target).is_err()
            {
                commands.entity(pip_entity).despawn();
            } else {
                node.display = Display::None;
                existing.insert(pip.target);
            }
            continue;
        };

        existing.insert(pip.target);
        node.display = Display::Flex;
        place_pip(&mut node, bounds, position, size);
        color.0 = pip_color;
    }

    for (entity, transform, owner) in &q_units {
        if !existing.contains(&entity) {
            let Some((position, color, size)) = pip_data_for_unit(
                transform.translation,
                owner.0,
                &players,
                local_player.0,
                fog.as_deref(),
            ) else {
                continue;
            };
            spawn_pip(
                &mut commands,
                layer,
                entity,
                MinimapPipKind::Unit,
                position,
                color,
                size,
                bounds,
            );
        }
    }

    for (entity, transform, owner) in &q_buildings {
        if !existing.contains(&entity) {
            let Some((position, color, size)) = pip_data_for_building(
                transform.translation,
                owner.0,
                &players,
                local_player.0,
                fog.as_deref(),
            ) else {
                continue;
            };
            spawn_pip(
                &mut commands,
                layer,
                entity,
                MinimapPipKind::Building,
                position,
                color,
                size,
                bounds,
            );
        }
    }

    for (entity, transform, ore) in &q_ore {
        if !existing.contains(&entity) {
            let Some((position, color, size)) =
                pip_data_for_ore(transform.translation, ore, &definitions, fog.as_deref())
            else {
                continue;
            };
            spawn_pip(
                &mut commands,
                layer,
                entity,
                MinimapPipKind::Ore,
                position,
                color,
                size,
                bounds,
            );
        }
    }
}

fn update_minimap_viewport(
    bounds: Option<Res<MapBounds>>,
    q_window: Query<&Window, With<PrimaryWindow>>,
    q_camera: Query<(&Transform, &Projection), With<RtsCamera>>,
    mut q_viewport: Query<&mut Node, With<MinimapViewport>>,
) {
    let Some(bounds) = bounds.as_deref() else {
        return;
    };
    let Ok(window) = q_window.single() else {
        return;
    };
    let Ok((camera_transform, projection)) = q_camera.single() else {
        return;
    };
    let Ok(mut node) = q_viewport.single_mut() else {
        return;
    };
    let Projection::Orthographic(orthographic) = projection else {
        return;
    };

    let (half_width, half_height) = orthographic_half_extents(orthographic, window);
    let footprint = ground_footprint(camera_transform, half_width, half_height);
    let (min_x, max_x, min_y, max_y) = footprint.iter().fold(
        (
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ),
        |(min_x, max_x, min_y, max_y), point| {
            let (map_x, map_y) = bounds.world_to_map_offset(*point);
            (
                min_x.min(map_x),
                max_x.max(map_x),
                min_y.min(map_y),
                max_y.max(map_y),
            )
        },
    );

    let left = ((min_x + bounds.half_width) / (bounds.half_width * 2.0)).clamp(0.0, 1.0);
    let right = ((max_x + bounds.half_width) / (bounds.half_width * 2.0)).clamp(0.0, 1.0);
    let top = ((min_y + bounds.half_height) / (bounds.half_height * 2.0)).clamp(0.0, 1.0);
    let bottom = ((max_y + bounds.half_height) / (bounds.half_height * 2.0)).clamp(0.0, 1.0);

    node.left = Val::Percent(left * 100.0);
    node.top = Val::Percent(top * 100.0);
    node.width = Val::Percent(((right - left) * 100.0).max(2.0));
    node.height = Val::Percent(((bottom - top) * 100.0).max(2.0));
}

fn handle_minimap_clicks(
    bounds: Option<Res<MapBounds>>,
    q_window: Query<&Window, With<PrimaryWindow>>,
    mut q_camera: Query<(&mut Transform, &Projection), With<RtsCamera>>,
    minimap_interaction: Res<MinimapInteractionState>,
) {
    let Some(bounds) = bounds.as_deref() else {
        return;
    };
    if !minimap_interaction.active {
        return;
    }
    let Some(target) = minimap_target_position(bounds, &minimap_interaction) else {
        return;
    };

    let Ok((mut camera_transform, projection)) = q_camera.single_mut() else {
        return;
    };

    let current_target = camera_ground_target(&camera_transform);
    let delta = target - current_target;
    camera_transform.translation.x += delta.x;
    camera_transform.translation.z += delta.y;

    if let Ok(window) = q_window.single() {
        clamp_camera_to_map_bounds(&mut camera_transform, projection, window, bounds);
    }
}

fn handle_minimap_secondary_click(
    bounds: Option<Res<MapBounds>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    minimap_interaction: Res<MinimapInteractionState>,
    q_selected_units: Query<(Entity, &Owner), (With<Unit>, With<Selected>)>,
    q_selected_buildings: Query<
        (Entity, &Owner),
        (
            With<Building>,
            With<Selected>,
            With<crate::game::buildings::RallyPoint>,
        ),
    >,
    local_player: Res<LocalPlayer>,
    mut move_events: MessageWriter<MoveCommand>,
    mut set_rally_point_events: MessageWriter<SetRallyPointCommand>,
) {
    if !mouse_buttons.just_pressed(MouseButton::Right) || !minimap_interaction.cursor_over {
        return;
    }

    let Some(bounds) = bounds.as_deref() else {
        return;
    };
    let Some(target_pos) = minimap_target_position(bounds, &minimap_interaction) else {
        return;
    };

    let mut selected_buildings = Vec::new();
    for (entity, owner) in &q_selected_buildings {
        if owner.0 == local_player.0 {
            selected_buildings.push(entity);
        }
    }

    for building_entity in selected_buildings {
        set_rally_point_events.write(SetRallyPointCommand {
            player_id: local_player.0,
            building_entity,
            target_pos,
        });
    }

    let mut selected_units = Vec::new();
    for (entity, owner) in &q_selected_units {
        if owner.0 == local_player.0 {
            selected_units.push(entity);
        }
    }

    if !selected_units.is_empty() {
        move_events.write(MoveCommand {
            player_id: local_player.0,
            unit_entities: selected_units,
            target_pos,
        });
    }
}

fn update_minimap_interaction_state(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    q_panel: Query<&RelativeCursorPosition, With<MinimapPanel>>,
    mut state: ResMut<MinimapInteractionState>,
) {
    let Ok(cursor) = q_panel.single() else {
        state.cursor_over = false;
        state.active = false;
        state.normalized_cursor = None;
        return;
    };

    state.cursor_over = cursor.cursor_over();
    state.normalized_cursor = if state.cursor_over {
        cursor.normalized.map(|position| {
            Vec2::new(
                (position.x + 0.5).clamp(0.0, 1.0),
                (position.y + 0.5).clamp(0.0, 1.0),
            )
        })
    } else {
        None
    };
    state.active = state.cursor_over && mouse_buttons.pressed(MouseButton::Left);
}

fn minimap_target_position(
    bounds: &MapBounds,
    minimap_interaction: &MinimapInteractionState,
) -> Option<Vec2> {
    let normalized = minimap_interaction.normalized_cursor?;
    let map_x = normalized.x.clamp(0.0, 1.0) * bounds.half_width * 2.0 - bounds.half_width;
    let map_y = normalized.y.clamp(0.0, 1.0) * bounds.half_height * 2.0 - bounds.half_height;

    Some(bounds.map_offset_to_world(map_x, map_y))
}

fn spawn_pip(
    commands: &mut Commands,
    layer: Entity,
    target: Entity,
    kind: MinimapPipKind,
    position: Vec3,
    color: Color,
    size: f32,
    bounds: &MapBounds,
) {
    let mut node = Node {
        position_type: PositionType::Absolute,
        width: Val::Px(size),
        height: Val::Px(size),
        ..default()
    };
    place_pip(&mut node, bounds, position, size);

    commands.entity(layer).with_children(|parent| {
        parent.spawn((
            node,
            BackgroundColor(color),
            ZIndex(5),
            MinimapPip { target, kind },
        ));
    });
}

fn place_pip(node: &mut Node, bounds: &MapBounds, position: Vec3, size: f32) {
    let (map_x, map_y) = bounds.world_to_map_offset(Vec2::new(position.x, position.z));
    let normalized_x = ((map_x + bounds.half_width) / (bounds.half_width * 2.0)).clamp(0.0, 1.0);
    let normalized_y = ((map_y + bounds.half_height) / (bounds.half_height * 2.0)).clamp(0.0, 1.0);

    node.left = Val::Px(normalized_x * MINIMAP_SIZE - size * 0.5);
    node.top = Val::Px(normalized_y * MINIMAP_SIZE - size * 0.5);
}

fn player_color(players: &Players, owner: usize) -> Color {
    players
        .players
        .get(&owner)
        .map(|player| player.color)
        .unwrap_or(Color::WHITE)
}

fn terrain_color_for_visibility(color: Color, state: VisibilityState) -> Color {
    let srgba = color.to_srgba();
    match state {
        VisibilityState::Unexplored => Color::srgb(0.0, 0.0, 0.0),
        VisibilityState::Explored => {
            Color::srgb(srgba.red * 0.18, srgba.green * 0.18, srgba.blue * 0.18)
        }
        VisibilityState::Visible => {
            Color::srgb(srgba.red * 0.75, srgba.green * 0.75, srgba.blue * 0.75)
        }
    }
}

fn pip_data_for_unit(
    position: Vec3,
    owner: usize,
    players: &Players,
    local_player: usize,
    fog: Option<&FogOfWar>,
) -> Option<(Vec3, Color, f32)> {
    if owner != local_player && fog_state_at_world(fog, position) != VisibilityState::Visible {
        return None;
    }

    Some((position, player_color(players, owner), UNIT_PIP_SIZE))
}

fn pip_data_for_building(
    position: Vec3,
    owner: usize,
    players: &Players,
    local_player: usize,
    fog: Option<&FogOfWar>,
) -> Option<(Vec3, Color, f32)> {
    if owner != local_player && fog_state_at_world(fog, position) == VisibilityState::Unexplored {
        return None;
    }

    Some((position, player_color(players, owner), BUILDING_PIP_SIZE))
}

fn pip_data_for_ore(
    position: Vec3,
    ore: &OreField,
    definitions: &Definitions,
    fog: Option<&FogOfWar>,
) -> Option<(Vec3, Color, f32)> {
    if fog_state_at_world(fog, position) == VisibilityState::Unexplored {
        return None;
    }

    let color = definitions
        .resources
        .get(&ore.resource_id)
        .map(|resource| Color::srgb(resource.color[0], resource.color[1], resource.color[2]))
        .unwrap_or(Color::srgb(0.95, 0.8, 0.2));

    Some((position, color, ORE_PIP_SIZE))
}

fn fog_state_at_world(fog: Option<&FogOfWar>, position: Vec3) -> VisibilityState {
    fog.map(|fog| fog.get_state(position.x.round() as i32, position.z.round() as i32))
        .unwrap_or(VisibilityState::Visible)
}

fn minimap_fog_state(
    fog: &FogOfWar,
    minimap: &MinimapData,
    map_x: u32,
    map_y: u32,
) -> VisibilityState {
    let cx = (minimap.width + minimap.height) as f32 / 2.0;
    let sx_rel = map_x as f32 - minimap.width as f32 / 2.0;
    let sy_rel = map_y as f32 - minimap.height as f32 / 2.0;
    let world_x = cx + sx_rel + sy_rel;
    let world_z = cx + sy_rel - sx_rel;

    fog.get_state(world_x.round() as i32, world_z.round() as i32)
}

fn camera_ground_target(transform: &Transform) -> Vec2 {
    let forward = transform.forward().as_vec3();
    if forward.y.abs() <= f32::EPSILON {
        return Vec2::new(transform.translation.x, transform.translation.z);
    }

    let distance_to_ground = -transform.translation.y / forward.y;
    let target = transform.translation + forward * distance_to_ground;
    Vec2::new(target.x, target.z)
}

fn orthographic_half_extents(orthographic: &OrthographicProjection, window: &Window) -> (f32, f32) {
    let aspect = if window.height() > 0.0 {
        window.width() / window.height()
    } else {
        1.0
    };
    let height = match orthographic.scaling_mode {
        bevy::camera::ScalingMode::FixedVertical { viewport_height } => viewport_height,
        _ => RTS_CAMERA_VIEW_HEIGHT,
    };

    (
        height * aspect * orthographic.scale * 0.5,
        height * orthographic.scale * 0.5,
    )
}

fn ground_footprint(transform: &Transform, half_width: f32, half_height: f32) -> [Vec2; 4] {
    let right = transform.right().as_vec3();
    let up = transform.up().as_vec3();
    let forward = transform.forward().as_vec3();
    let target = camera_ground_target(transform);

    [
        target + ground_footprint_offset(right, up, forward, -half_width, -half_height),
        target + ground_footprint_offset(right, up, forward, half_width, -half_height),
        target + ground_footprint_offset(right, up, forward, -half_width, half_height),
        target + ground_footprint_offset(right, up, forward, half_width, half_height),
    ]
}

fn ground_footprint_offset(right: Vec3, up: Vec3, forward: Vec3, view_x: f32, view_y: f32) -> Vec2 {
    let view_offset = right * view_x + up * view_y;
    if forward.y.abs() <= f32::EPSILON {
        return Vec2::new(view_offset.x, view_offset.z);
    }

    let ground_offset = view_offset - forward * (view_offset.y / forward.y);
    Vec2::new(ground_offset.x, ground_offset.z)
}

fn float_to_u8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}
