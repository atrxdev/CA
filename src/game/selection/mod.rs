use crate::game::camera::RtsCamera;
use crate::game::combat::AttackTarget;
use crate::game::commands::SellBuildingCommand;
use crate::game::game_state::AppState;
use crate::game::ui::CursorMode;
use bevy::prelude::*;

pub struct SelectionPlugin;

impl Plugin for SelectionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectionBox>()
            .add_systems(OnEnter(AppState::InGame), setup_selection_materials)
            .add_systems(
                Update,
                (
                    handle_selection,
                    update_selection_visuals,
                    draw_selection_health_bars,
                    draw_rally_points,
                    draw_unit_destinations,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

#[derive(Component)]
pub struct Selectable;

#[derive(Component)]
pub struct BaseMaterial(pub Handle<StandardMaterial>);

#[derive(Component)]
pub struct Selected;

#[derive(Resource, Default)]
pub struct SelectionBox {
    pub start_pos: Option<Vec2>,
}

#[derive(Resource)]
pub struct SelectionMaterials {
    pub default: Handle<StandardMaterial>,
    pub selected: Handle<StandardMaterial>,
}

fn setup_selection_materials(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(SelectionMaterials {
        default: materials.add(StandardMaterial {
            base_color: Color::srgb(0.8, 0.2, 0.2), // Red for unselected
            ..default()
        }),
        selected: materials.add(StandardMaterial {
            base_color: Color::srgb(0.2, 0.8, 0.2), // Green for selected
            ..default()
        }),
    });

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.2, 0.8, 0.2, 0.2)),
        BorderColor::all(Color::srgba(0.2, 0.8, 0.2, 0.8)),
        Visibility::Hidden,
        SelectionMarquee,
    ));
}

#[derive(Component)]
pub struct SelectionMarquee;

fn handle_selection(
    mut commands: Commands,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    minimap_interaction: Option<Res<crate::game::ui::minimap::MinimapInteractionState>>,
    q_window: Query<&Window>,
    q_camera: Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
    q_selectable: Query<
        (
            Entity,
            &GlobalTransform,
            &Visibility,
            Option<&crate::game::units::Owner>,
        ),
        With<Selectable>,
    >,
    mut selection_box: ResMut<SelectionBox>,
    keys: Res<ButtonInput<KeyCode>>,
    mut q_marquee: Query<
        (&mut Node, &mut Visibility),
        (With<SelectionMarquee>, Without<Selectable>),
    >,
    local_player: Res<crate::game::player::LocalPlayer>,
    mut cursor_mode: ResMut<CursorMode>,
    mut sell_events: MessageWriter<SellBuildingCommand>,
    q_buildings: Query<&crate::game::buildings::Building>,
    definitions: Res<crate::game::data::Definitions>,
) {
    let Ok(window) = q_window.single() else {
        return;
    };
    let Ok((camera, camera_transform)) = q_camera.single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };

    let is_shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let is_ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);

    let Ok((mut marquee_node, mut marquee_vis)) = q_marquee.single_mut() else {
        return;
    };

    if minimap_interaction
        .as_deref()
        .is_some_and(|state| state.cursor_over || state.active)
    {
        selection_box.start_pos = None;
        *marquee_vis = Visibility::Hidden;
        return;
    }

    if mouse_buttons.just_pressed(MouseButton::Right) {
        if *cursor_mode != CursorMode::Normal {
            *cursor_mode = CursorMode::Normal;
        }
        return;
    }

    if mouse_buttons.just_pressed(MouseButton::Left) {
        selection_box.start_pos = Some(cursor_pos);
        if *cursor_mode == CursorMode::Normal {
            *marquee_vis = Visibility::Visible;
        }
    } else if mouse_buttons.just_released(MouseButton::Left) {
        let start_pos = selection_box.start_pos.unwrap_or(cursor_pos);
        selection_box.start_pos = None;
        *marquee_vis = Visibility::Hidden;

        let end_pos = cursor_pos;
        let min_x = start_pos.x.min(end_pos.x);
        let max_x = start_pos.x.max(end_pos.x);
        let min_y = start_pos.y.min(end_pos.y);
        let max_y = start_pos.y.max(end_pos.y);

        let is_click = (max_x - min_x) < 5.0 && (max_y - min_y) < 5.0;

        if !is_shift && !is_ctrl {
            for (entity, _, _, _) in q_selectable.iter() {
                commands.entity(entity).try_remove::<Selected>();
            }
        }

        let mut clicked_entity = None;
        if is_click {
            let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) else {
                return;
            };
            let mut closest_dist = f32::MAX;
            for (entity, transform, visibility, _) in q_selectable.iter() {
                if *visibility == Visibility::Hidden {
                    continue;
                }
                let unit_pos = transform.translation();
                let v = unit_pos - ray.origin;
                let t = v.dot(*ray.direction);
                if t > 0.0 {
                    let proj = ray.origin + *ray.direction * t;

                    let mut is_hit = false;
                    if let Ok(building) = q_buildings.get(entity) {
                        if let Some(def) = definitions.buildings.get(&building.building_id) {
                            let half_w = def.size.0 as f32 / 2.0;
                            let half_h = def.size.1 as f32 / 2.0;
                            if proj.x >= unit_pos.x - half_w
                                && proj.x <= unit_pos.x + half_w
                                && proj.z >= unit_pos.z - half_h
                                && proj.z <= unit_pos.z + half_h
                            {
                                is_hit = true;
                            }
                        }
                    } else {
                        let dist = proj.distance(unit_pos);
                        if dist < 1.0 {
                            is_hit = true;
                        }
                    }

                    if is_hit && t < closest_dist {
                        closest_dist = t;
                        clicked_entity = Some(entity);
                    }
                }
            }
        }

        if is_click && clicked_entity.is_some() {
            let entity = clicked_entity.unwrap();

            if *cursor_mode == CursorMode::Sell {
                if q_buildings.get(entity).is_ok() {
                    if let Ok((_, _, _, Some(owner))) = q_selectable.get(entity) {
                        if owner.0 == local_player.0 {
                            sell_events.write(SellBuildingCommand {
                                player_id: local_player.0,
                                building_entity: entity,
                            });
                        }
                    }
                }
                return;
            }

            if is_ctrl {
                commands.entity(entity).try_remove::<Selected>();
            } else {
                commands.entity(entity).insert(Selected);
            }
        } else if !is_click && *cursor_mode == CursorMode::Normal {
            for (entity, transform, visibility, owner_opt) in q_selectable.iter() {
                if *visibility == Visibility::Hidden {
                    continue;
                }
                // Only select local player's units when dragging
                if let Some(owner) = owner_opt {
                    if owner.0 != local_player.0 {
                        continue;
                    }
                }

                let unit_pos = transform.translation();
                if let Ok(screen_pos) = camera.world_to_viewport(camera_transform, unit_pos) {
                    if screen_pos.x >= min_x
                        && screen_pos.x <= max_x
                        && screen_pos.y >= min_y
                        && screen_pos.y <= max_y
                    {
                        if is_ctrl {
                            commands.entity(entity).try_remove::<Selected>();
                        } else {
                            commands.entity(entity).insert(Selected);
                        }
                    }
                }
            }
        }
    }

    if let Some(start_pos) = selection_box.start_pos {
        if mouse_buttons.pressed(MouseButton::Left) {
            let min_x = start_pos.x.min(cursor_pos.x);
            let max_x = start_pos.x.max(cursor_pos.x);
            let min_y = start_pos.y.min(cursor_pos.y);
            let max_y = start_pos.y.max(cursor_pos.y);

            marquee_node.left = Val::Px(min_x);
            marquee_node.top = Val::Px(min_y);
            marquee_node.width = Val::Px(max_x - min_x);
            marquee_node.height = Val::Px(max_y - min_y);
        }
    }
}

fn update_selection_visuals(
    mut q_materials: Query<
        (
            &mut MeshMaterial3d<StandardMaterial>,
            &BaseMaterial,
            Has<Selected>,
        ),
        With<Selectable>,
    >,
    selection_materials: Res<SelectionMaterials>,
) {
    for (mut material, base, is_selected) in q_materials.iter_mut() {
        let expected_mat = if is_selected {
            selection_materials.selected.clone()
        } else {
            base.0.clone()
        };

        if material.0 != expected_mat {
            material.0 = expected_mat;
        }
    }
}

fn draw_selection_health_bars(
    mut gizmos: Gizmos,
    q_selected: Query<
        (
            &GlobalTransform,
            Option<&crate::game::units::Unit>,
            Option<&crate::game::buildings::Building>,
        ),
        With<Selected>,
    >,
    q_camera: Query<&GlobalTransform, With<crate::game::camera::RtsCamera>>,
) {
    let Ok(camera_transform) = q_camera.single() else {
        return;
    };
    let cam_right = camera_transform.right().normalize_or_zero();
    let cam_up = camera_transform.up().normalize_or_zero();

    for (transform, opt_unit, opt_building) in q_selected.iter() {
        let (health, max_health, y_offset, width) = if let Some(unit) = opt_unit {
            (unit.health, unit.max_health, 1.2, 1.0)
        } else if let Some(building) = opt_building {
            (building.health, building.max_health, 2.5, 2.0)
        } else {
            continue;
        };

        if max_health <= 0.0 {
            continue;
        }

        let health_pct = (health / max_health).clamp(0.0, 1.0);
        let pos = transform.translation() + Vec3::new(0.0, y_offset, 0.0);

        let half_width = width / 2.0;
        let half_width_vec = cam_right * half_width;

        // Draw many tightly packed lines to simulate a solid bar
        for i in 0..20 {
            let offset = cam_up * (i as f32 * 0.005);
            let left = pos - half_width_vec + offset;
            let right_point = pos + half_width_vec + offset;

            let middle = left + (cam_right * (width * health_pct));

            // Foreground (Green)
            if health_pct > 0.0 {
                gizmos.line(left, middle, Color::srgb(0.0, 0.8, 0.0));
            }
            // Background (Red)
            if health_pct < 1.0 {
                gizmos.line(middle, right_point, Color::srgb(0.8, 0.0, 0.0));
            }
        }
    }
}

fn draw_rally_points(
    mut gizmos: Gizmos,
    q_selected: Query<(&GlobalTransform, &crate::game::buildings::RallyPoint), With<Selected>>,
) {
    for (transform, rally_point) in q_selected.iter() {
        let start = transform.translation() + Vec3::new(0.0, 0.5, 0.0);
        let end = Vec3::new(rally_point.0.x, 0.5, rally_point.0.y);

        draw_destination_gizmo(&mut gizmos, start, end, Color::srgb(1.0, 1.0, 1.0));
    }
}

fn draw_unit_destinations(
    mut gizmos: Gizmos,
    q_selected: Query<
        (
            &GlobalTransform,
            Option<&crate::game::pathfinding::Path>,
            Option<&AttackTarget>,
        ),
        (With<Selected>, With<crate::game::units::Unit>),
    >,
    q_targets: Query<&GlobalTransform>,
) {
    for (transform, path, attack_target) in q_selected.iter() {
        let start = transform.translation() + Vec3::new(0.0, 0.5, 0.0);
        let end = if let Some(attack_target) = attack_target {
            let Ok(target_transform) = q_targets.get(attack_target.0) else {
                continue;
            };
            let target_translation = target_transform.translation();
            Vec3::new(target_translation.x, 0.5, target_translation.z)
        } else if let Some(destination) = path.and_then(|path| path.waypoints.last()) {
            Vec3::new(destination.x, 0.5, destination.y)
        } else {
            continue;
        };

        draw_destination_gizmo(&mut gizmos, start, end, Color::srgb(1.0, 1.0, 1.0));
    }
}

fn draw_destination_gizmo(gizmos: &mut Gizmos, start: Vec3, end: Vec3, color: Color) {
    let dir = end - start;
    let dist = dir.length();

    if dist > 0.1 {
        let dir_norm = dir / dist;
        let segment_len = 0.4;
        let gap_len = 0.3;
        let mut current_d = 0.0;

        while current_d < dist {
            let next_d = (current_d + segment_len).min(dist);
            gizmos.line(
                start + dir_norm * current_d,
                start + dir_norm * next_d,
                color,
            );
            current_d = next_d + gap_len;
        }
    }

    let dot_radius: f32 = 0.15;
    let mut z = -dot_radius;
    while z <= dot_radius {
        let x = (dot_radius * dot_radius - z * z).sqrt();
        gizmos.line(
            end + Vec3::new(-x, 0.0, z),
            end + Vec3::new(x, 0.0, z),
            color,
        );
        z += 0.02;
    }
}
