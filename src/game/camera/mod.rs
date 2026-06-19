use crate::game::game_state::AppState;
use crate::game::map::MapBounds;
use bevy::camera::ScalingMode;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

pub const RTS_CAMERA_VIEW_HEIGHT: f32 = 50.0;
pub const RTS_CAMERA_MIN_SCALE: f32 = 0.35;
pub const RTS_CAMERA_MAX_SCALE: f32 = 3.0;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), setup_camera)
            .add_systems(Update, camera_controls.run_if(in_state(AppState::InGame)));
    }
}

#[derive(Component)]
pub struct RtsCamera {
    pub pan_speed: f32,
    pub zoom_speed: f32,
    pub edge_pan_width: f32,
}

impl Default for RtsCamera {
    fn default() -> Self {
        Self {
            pan_speed: 30.0,
            zoom_speed: 2.0,
            edge_pan_width: 20.0,
        }
    }
}

fn setup_camera(mut commands: Commands) {
    // 45 degrees yaw (isometric), 55 degrees pitch (looking down)
    let yaw = 45.0_f32.to_radians();
    let pitch = -55.0_f32.to_radians();

    let rotation = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
    let target = Vec3::new(20.0, 0.0, 20.0);

    // We want the camera at y = 50.0.
    let forward = rotation * Vec3::NEG_Z;
    let distance = 50.0 / forward.y.abs();
    let position = target - forward * distance;

    commands.spawn((
        Camera3d::default(),
        Projection::from(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: RTS_CAMERA_VIEW_HEIGHT,
            },
            ..OrthographicProjection::default_3d()
        }),
        Transform::from_translation(position).with_rotation(rotation),
        RtsCamera::default(),
    ));
}

fn camera_controls(
    mut q_camera: Query<(&mut Transform, &mut Projection, &RtsCamera), With<Camera3d>>,
    q_window: Query<&Window, With<PrimaryWindow>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut mouse_motion: MessageReader<MouseMotion>,
    mut mouse_wheel: MessageReader<MouseWheel>,
    time: Res<Time>,
    console_data: Option<Res<crate::game::ui::console::ConsoleData>>,
    minimap_interaction: Option<Res<crate::game::ui::minimap::MinimapInteractionState>>,
    map_bounds: Option<Res<MapBounds>>,
) {
    let Ok((mut transform, mut projection, rts_camera)) = q_camera.single_mut() else {
        return;
    };
    let Ok(window) = q_window.single() else {
        return;
    };

    let mut pan_vector = Vec2::ZERO;

    let console_open = console_data.map(|c| c.is_open).unwrap_or(false);

    // Arrow key panning
    if !console_open {
        if keyboard.pressed(KeyCode::ArrowLeft) {
            pan_vector.x -= 1.0;
        }
        if keyboard.pressed(KeyCode::ArrowRight) {
            pan_vector.x += 1.0;
        }
        if keyboard.pressed(KeyCode::ArrowUp) {
            pan_vector.y += 1.0;
        }
        if keyboard.pressed(KeyCode::ArrowDown) {
            pan_vector.y -= 1.0;
        }
    }

    // Edge panning
    let cursor_over_minimap = minimap_interaction
        .as_deref()
        .is_some_and(|state| state.cursor_over || state.active);
    if !cursor_over_minimap && let Some(cursor_pos) = window.cursor_position() {
        if cursor_pos.x < rts_camera.edge_pan_width {
            pan_vector.x -= 1.0;
        } else if cursor_pos.x > window.width() - rts_camera.edge_pan_width {
            pan_vector.x += 1.0;
        }

        if cursor_pos.y < rts_camera.edge_pan_width {
            pan_vector.y += 1.0; // Panning "up" (forward in world space)
        } else if cursor_pos.y > window.height() - rts_camera.edge_pan_width {
            pan_vector.y -= 1.0;
        }
    }

    // Middle mouse drag
    if mouse_buttons.pressed(MouseButton::Middle) {
        for motion in mouse_motion.read() {
            pan_vector.x -= motion.delta.x * 0.1;
            pan_vector.y += motion.delta.y * 0.1;
        }
    } else {
        mouse_motion.clear(); // Consume events
    }

    // Apply pan
    if pan_vector != Vec2::ZERO {
        let pan_dir = pan_vector.normalize_or_zero();

        let forward = transform.forward().as_vec3();
        let forward_flat = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
        let right = transform.right().as_vec3();
        let right_flat = Vec3::new(right.x, 0.0, right.z).normalize_or_zero();

        let move_vec = (right_flat * pan_dir.x + forward_flat * pan_dir.y)
            * rts_camera.pan_speed
            * time.delta_secs();
        transform.translation += move_vec;
    }

    // Zooming
    let mut zoom = 0.0;
    for wheel in mouse_wheel.read() {
        zoom += wheel.y;
    }

    if let Projection::Orthographic(orthographic) = projection.as_mut() {
        let zoom_factor = if zoom != 0.0 {
            1.0 - zoom * rts_camera.zoom_speed * 0.05
        } else {
            1.0
        };
        let max_scale = map_bounds
            .as_deref()
            .map(|bounds| {
                max_orthographic_scale_for_bounds(&transform, orthographic, window, bounds)
            })
            .unwrap_or(RTS_CAMERA_MAX_SCALE);

        orthographic.scale =
            (orthographic.scale * zoom_factor).clamp(RTS_CAMERA_MIN_SCALE, max_scale);
    }

    if let Some(bounds) = map_bounds.as_deref() {
        clamp_camera_to_map_bounds(&mut transform, &projection, window, bounds);
    }
}

pub fn clamp_camera_to_map_bounds(
    transform: &mut Transform,
    projection: &Projection,
    window: &Window,
    bounds: &MapBounds,
) {
    let target = camera_ground_target(transform);
    let mut target_map_offset = bounds.world_to_map_offset(target);

    if let Projection::Orthographic(orthographic) = projection {
        let (half_width, half_height) = orthographic_half_extents(orthographic, window);
        let footprint = ground_footprint_offsets(transform, half_width, half_height);
        let (min_x, max_x, min_y, max_y) = footprint.iter().fold(
            (
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
            ),
            |(min_x, max_x, min_y, max_y), offset| {
                let map_x = (offset.x - offset.y) / 2.0;
                let map_y = (offset.x + offset.y) / 2.0;
                (
                    min_x.min(map_x),
                    max_x.max(map_x),
                    min_y.min(map_y),
                    max_y.max(map_y),
                )
            },
        );

        target_map_offset.0 = clamp_with_extents(
            target_map_offset.0,
            -bounds.half_width - min_x,
            bounds.half_width - max_x,
        );
        target_map_offset.1 = clamp_with_extents(
            target_map_offset.1,
            -bounds.half_height - min_y,
            bounds.half_height - max_y,
        );
    } else {
        let clamped_target = bounds.clamp_point(target);
        target_map_offset = bounds.world_to_map_offset(clamped_target);
    }

    let clamped_target = bounds.map_offset_to_world(target_map_offset.0, target_map_offset.1);
    let target_delta = clamped_target - target;

    transform.translation.x += target_delta.x;
    transform.translation.z += target_delta.y;
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
    let (base_half_width, base_half_height) = orthographic_base_half_extents(orthographic, window);

    (
        base_half_width * orthographic.scale,
        base_half_height * orthographic.scale,
    )
}

fn orthographic_base_half_extents(
    orthographic: &OrthographicProjection,
    window: &Window,
) -> (f32, f32) {
    let aspect = if window.height() > 0.0 {
        window.width() / window.height()
    } else {
        1.0
    };
    let height = match orthographic.scaling_mode {
        ScalingMode::FixedVertical { viewport_height } => viewport_height,
        _ => RTS_CAMERA_VIEW_HEIGHT,
    };

    (height * aspect * 0.5, height * 0.5)
}

fn max_orthographic_scale_for_bounds(
    transform: &Transform,
    orthographic: &OrthographicProjection,
    window: &Window,
    bounds: &MapBounds,
) -> f32 {
    let (base_half_width, base_half_height) = orthographic_base_half_extents(orthographic, window);
    let footprint = ground_footprint_offsets(transform, base_half_width, base_half_height);
    let (max_x, max_y) = footprint
        .iter()
        .fold((0.0_f32, 0.0_f32), |(max_x, max_y), offset| {
            let map_x = (offset.x - offset.y) / 2.0;
            let map_y = (offset.x + offset.y) / 2.0;
            (max_x.max(map_x.abs()), max_y.max(map_y.abs()))
        });

    let x_scale = if max_x > f32::EPSILON {
        bounds.half_width / max_x
    } else {
        RTS_CAMERA_MAX_SCALE
    };
    let y_scale = if max_y > f32::EPSILON {
        bounds.half_height / max_y
    } else {
        RTS_CAMERA_MAX_SCALE
    };

    x_scale
        .min(y_scale)
        .min(RTS_CAMERA_MAX_SCALE)
        .max(RTS_CAMERA_MIN_SCALE)
}

fn ground_footprint_offsets(transform: &Transform, half_width: f32, half_height: f32) -> [Vec2; 4] {
    let right = transform.right().as_vec3();
    let up = transform.up().as_vec3();
    let forward = transform.forward().as_vec3();

    [
        ground_footprint_offset(right, up, forward, -half_width, -half_height),
        ground_footprint_offset(right, up, forward, half_width, -half_height),
        ground_footprint_offset(right, up, forward, -half_width, half_height),
        ground_footprint_offset(right, up, forward, half_width, half_height),
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

fn clamp_with_extents(value: f32, min: f32, max: f32) -> f32 {
    if min <= max {
        value.clamp(min, max)
    } else {
        (min + max) * 0.5
    }
}
