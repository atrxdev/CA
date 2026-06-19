use crate::game::camera::RtsCamera;
use crate::game::commands::{
    AttackCommand, HarvestCommand, MoveCommand, ReturnToRefineryCommand, SetRallyPointCommand,
};
use crate::game::economy::{Harvester, OreField};
use crate::game::game_state::{AppState, game_is_playing};
use crate::game::selection::Selected;
use crate::game::units::{Owner, Unit};
use bevy::prelude::*;

pub struct OrdersPlugin;

impl Plugin for OrdersPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            handle_right_click
                .run_if(game_is_playing)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

fn handle_right_click(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    minimap_interaction: Option<Res<crate::game::ui::minimap::MinimapInteractionState>>,
    q_window: Query<&Window>,
    q_camera: Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
    q_selected: Query<(Entity, &Owner, Has<Harvester>), (With<Unit>, With<Selected>)>,
    q_selected_buildings: Query<
        (Entity, &Owner),
        (
            With<crate::game::buildings::Building>,
            With<Selected>,
            With<crate::game::buildings::RallyPoint>,
        ),
    >,
    q_all_units: Query<(Entity, &Transform, &Owner, &Visibility), With<Unit>>,
    q_ore: Query<(Entity, &Transform), With<OreField>>,
    q_refinery: Query<(Entity, &Transform, &Owner), With<crate::game::economy::Refinery>>,
    mut move_events: MessageWriter<MoveCommand>,
    mut attack_events: MessageWriter<AttackCommand>,
    mut harvest_events: MessageWriter<HarvestCommand>,
    mut set_rally_point_events: MessageWriter<SetRallyPointCommand>,
    mut return_events: MessageWriter<ReturnToRefineryCommand>,
    local_player: Res<crate::game::player::LocalPlayer>,
) {
    if !mouse_buttons.just_pressed(MouseButton::Right) {
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

    // Filter clicked enemy
    let mut clicked_enemy = None;
    for (entity, transform, team, visibility) in q_all_units.iter() {
        if team.0 != local_player.0 {
            if *visibility == Visibility::Hidden {
                continue;
            }
            let dist = transform.translation.distance(intersection);
            if dist < 1.5 {
                clicked_enemy = Some(entity);
                break;
            }
        }
    }

    // Filter clicked ore
    let mut clicked_ore = None;
    for (entity, transform) in q_ore.iter() {
        let dist = transform.translation.distance(intersection);
        if dist < 1.5 {
            clicked_ore = Some(entity);
            break;
        }
    }

    // Filter clicked refinery
    let mut clicked_refinery = None;
    for (entity, transform, owner) in q_refinery.iter() {
        if owner.0 == local_player.0 {
            let dist = transform.translation.distance(intersection);
            if dist < 3.0 {
                // Refineries are larger
                clicked_refinery = Some(entity);
                break;
            }
        }
    }

    // Gather selected player units
    let mut selected_player_units = Vec::new();
    let mut harvester_units = Vec::new();
    let mut non_harvester_units = Vec::new();

    let mut selected_player_buildings = Vec::new();

    // Query selected units
    for (entity, team, is_harvester) in q_selected.iter() {
        if team.0 == local_player.0 {
            selected_player_units.push(entity);
            if is_harvester {
                harvester_units.push(entity);
            } else {
                non_harvester_units.push(entity);
            }
        }
    }

    // Query selected buildings with rally point
    // We didn't add it to query params, we need to add a new query

    for (entity, team) in q_selected_buildings.iter() {
        if team.0 == local_player.0 {
            selected_player_buildings.push(entity);
        }
    }

    if selected_player_units.is_empty() && selected_player_buildings.is_empty() {
        return;
    }

    // Handle buildings rally point
    if !selected_player_buildings.is_empty() {
        for building_entity in selected_player_buildings {
            set_rally_point_events.write(SetRallyPointCommand {
                player_id: local_player.0,
                building_entity,
                target_pos: Vec2::new(intersection.x, intersection.z),
            });
        }
    }

    if selected_player_units.is_empty() {
        return;
    }

    if let Some(ore_entity) = clicked_ore {
        // Harvesters harvest, others move
        if !harvester_units.is_empty() {
            harvest_events.write(HarvestCommand {
                player_id: local_player.0,
                unit_entities: harvester_units,
                ore_entity,
            });
        }
        if !non_harvester_units.is_empty() {
            move_events.write(MoveCommand {
                player_id: local_player.0,
                unit_entities: non_harvester_units,
                target_pos: Vec2::new(intersection.x, intersection.z),
            });
        }
    } else if let Some(refinery_entity) = clicked_refinery {
        if !harvester_units.is_empty() {
            return_events.write(ReturnToRefineryCommand {
                player_id: local_player.0,
                unit_entities: harvester_units,
                refinery_entity,
            });
        }
        if !non_harvester_units.is_empty() {
            move_events.write(MoveCommand {
                player_id: local_player.0,
                unit_entities: non_harvester_units,
                target_pos: Vec2::new(intersection.x, intersection.z),
            });
        }
    } else if let Some(enemy_entity) = clicked_enemy {
        // Attack
        attack_events.write(AttackCommand {
            player_id: local_player.0,
            unit_entities: selected_player_units,
            target_entity: enemy_entity,
        });
    } else {
        // Move
        move_events.write(MoveCommand {
            player_id: local_player.0,
            unit_entities: selected_player_units,
            target_pos: Vec2::new(intersection.x, intersection.z),
        });
    }
}
