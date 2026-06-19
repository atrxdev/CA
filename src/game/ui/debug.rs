use crate::game::buildings::Building;
use crate::game::camera::RtsCamera;
use crate::game::combat::AttackTarget;
use crate::game::economy::{Harvester, HarvesterState, OreField};
use crate::game::game_state::AppState;
use crate::game::pathfinding::Grid;
use crate::game::pathfinding::Path;
use crate::game::selection::Selected;
use crate::game::units::Unit;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

pub struct DebugUiPlugin;

impl Plugin for DebugUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), setup_debug_ui)
            .add_systems(Update, update_debug_ui.run_if(in_state(AppState::InGame)));
    }
}

#[derive(Component)]
pub struct DebugText;

fn setup_debug_ui(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(10.0),
                left: Val::Px(10.0),
                padding: UiRect::all(Val::Px(10.0)),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            ZIndex(100),
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.8)),
        ))
        .with_children(|p| {
            p.spawn((
                Text::new(""),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                DebugText,
            ));
        });
}

fn update_debug_ui(
    time: Res<Time>,
    q_all_entities: Query<Entity>,
    q_units: Query<&Unit>,
    q_window: Query<&Window, With<PrimaryWindow>>,
    q_camera: Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
    grid: Res<Grid>,
    q_selected: Query<
        (
            Entity,
            Option<&Unit>,
            Option<&Building>,
            Option<&Path>,
            Option<&AttackTarget>,
            Option<&Harvester>,
            Option<&OreField>,
        ),
        With<Selected>,
    >,
    mut q_text: Query<&mut Text, With<DebugText>>,
    mut frame_times: Local<std::collections::VecDeque<f32>>,
) {
    let dt = time.delta_secs();
    frame_times.push_back(dt);
    if frame_times.len() > 30 {
        frame_times.pop_front();
    }
    let avg_dt: f32 = frame_times.iter().sum::<f32>() / frame_times.len() as f32;
    let fps = if avg_dt > 0.0 { 1.0 / avg_dt } else { 0.0 };

    let entity_count = q_all_entities.iter().count();
    let unit_count = q_units.iter().count();

    let mut debug_str = format!(
        "FPS: {:.1}\nEntities: {}\nUnits: {}\n\n",
        fps, entity_count, unit_count
    );
    if let Some((grid_x, grid_z)) = mouse_grid_coords(&q_window, &q_camera) {
        let status = if grid.in_bounds(grid_x, grid_z) {
            ""
        } else {
            " (outside)"
        };
        debug_str.push_str(&format!("Mouse Grid: {}, {}{}\n\n", grid_x, grid_z, status));
    } else {
        debug_str.push_str("Mouse Grid: --\n\n");
    }

    let selected_count = q_selected.iter().count();
    if selected_count > 0 {
        debug_str.push_str(&format!("Selected Count: {}\n", selected_count));

        // Show details for the first selected entity
        if let Some((
            entity,
            opt_unit,
            opt_building,
            opt_path,
            opt_attack,
            opt_harvester,
            opt_ore,
        )) = q_selected.iter().next()
        {
            debug_str.push_str(&format!("Selected [{}]:\n", entity.to_bits()));

            if let Some(unit) = opt_unit {
                debug_str.push_str(&format!("  Type: Unit ({})\n", unit.unit_id));
                debug_str.push_str(&format!(
                    "  Health: {:.0} / {:.0}\n",
                    unit.health, unit.max_health
                ));
            } else if let Some(building) = opt_building {
                debug_str.push_str(&format!("  Type: Building ({})\n", building.building_id));
                debug_str.push_str(&format!(
                    "  Health: {:.0} / {:.0}\n",
                    building.health, building.max_health
                ));
            } else if let Some(ore) = opt_ore {
                debug_str.push_str("  Type: Ore Field\n");
                debug_str.push_str(&format!("  Remaining Ore: {}\n", ore.amount));
            }

            let mut orders = Vec::new();
            if opt_path.is_some() {
                orders.push("Moving");
            }
            if opt_attack.is_some() {
                orders.push("Attacking");
            }
            if let Some(harvester) = opt_harvester {
                let state_str = match harvester.state {
                    HarvesterState::Idle => "Idle",
                    HarvesterState::SeekingNearestOre => "Seeking Ore",
                    HarvesterState::MovingToOre(_) => "Moving to Ore",
                    HarvesterState::Harvesting(_) => "Harvesting",
                    HarvesterState::ReturningToRefinery(_) => "Returning",
                };
                orders.push(state_str);
            }

            if orders.is_empty() {
                orders.push("Idle");
            }

            debug_str.push_str(&format!("  Orders: {}\n", orders.join(", ")));
        }
    } else {
        debug_str.push_str("No Selection\n");
    }

    for mut text in q_text.iter_mut() {
        text.0 = debug_str.clone();
    }
}

fn mouse_grid_coords(
    q_window: &Query<&Window, With<PrimaryWindow>>,
    q_camera: &Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
) -> Option<(i32, i32)> {
    let window = q_window.single().ok()?;
    let (camera, camera_transform) = q_camera.single().ok()?;
    let cursor_position = window.cursor_position()?;
    let ray = camera
        .viewport_to_world(camera_transform, cursor_position)
        .ok()?;

    if ray.direction.y.abs() < 0.001 {
        return None;
    }

    let t = -ray.origin.y / ray.direction.y;
    if t < 0.0 {
        return None;
    }

    let intersection = ray.origin + *ray.direction * t;
    Some((intersection.x.round() as i32, intersection.z.round() as i32))
}
