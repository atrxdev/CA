use crate::game::camera::RtsCamera;
use crate::game::game_state::AppState;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

pub mod map_asset;
pub mod map_chunk;
pub mod map_loader;
pub mod map_registry;
pub mod map_spawner;
pub mod terrain;

use map_asset::Map;
use map_loader::MapAssetLoader;
use map_registry::MapRegistry;
use map_spawner::{spawn_map_when_loaded, start_loading_map};

#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct MapSelection(pub String);

#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct ShowMapGrid(pub bool);

#[derive(Resource, Debug, Clone, Copy)]
pub struct MapBounds {
    pub center: Vec2,
    pub half_width: f32,
    pub half_height: f32,
}

impl MapBounds {
    pub fn new(center: Vec2, width: f32, height: f32) -> Self {
        Self {
            center,
            half_width: width / 2.0,
            half_height: height / 2.0,
        }
    }

    pub fn clamp_point(&self, point: Vec2) -> Vec2 {
        let (map_x, map_y) = self.world_to_map_offset(point);
        let map_x = map_x.clamp(-self.half_width, self.half_width);
        let map_y = map_y.clamp(-self.half_height, self.half_height);

        self.map_offset_to_world(map_x, map_y)
    }

    pub fn world_to_map_offset(&self, point: Vec2) -> (f32, f32) {
        let offset = point - self.center;
        ((offset.x - offset.y) / 2.0, (offset.x + offset.y) / 2.0)
    }

    pub fn map_offset_to_world(&self, map_x: f32, map_y: f32) -> Vec2 {
        self.center + Vec2::new(map_x + map_y, map_y - map_x)
    }
}

#[derive(Resource, Debug, Clone)]
pub struct MinimapData {
    pub width: u32,
    pub height: u32,
    pub terrain_colors: Vec<Color>,
}

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MapSelection>();
        app.init_resource::<ShowMapGrid>();

        app.init_asset::<Map>();
        app.init_asset_loader::<MapAssetLoader>();

        let mut registry = MapRegistry::default();
        registry.scan();
        app.insert_resource(registry);

        app.add_systems(OnEnter(AppState::InGame), start_loading_map);
        app.add_systems(
            Update,
            (spawn_map_when_loaded, draw_map_grid).run_if(in_state(AppState::InGame)),
        );
    }
}

fn draw_map_grid(
    show_grid: Res<ShowMapGrid>,
    grid: Res<crate::game::pathfinding::Grid>,
    q_window: Query<&Window, With<PrimaryWindow>>,
    q_camera: Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
    mut gizmos: Gizmos,
) {
    if !show_grid.0 {
        return;
    }

    let color = Color::srgba(1.0, 1.0, 1.0, 0.2);

    // Draw vertical lines (along z axis)
    for x in 0..=grid.width {
        let mut min_z = grid.height;
        let mut max_z = -1;
        let mut found = false;

        for z in 0..grid.height {
            if grid.in_bounds(x, z) || grid.in_bounds(x - 1, z) {
                min_z = min_z.min(z);
                max_z = max_z.max(z);
                found = true;
            }
        }

        if found {
            gizmos.line(
                Vec3::new(x as f32 - 0.5, 0.05, min_z as f32 - 0.5),
                Vec3::new(x as f32 - 0.5, 0.05, max_z as f32 + 0.5),
                color,
            );
        }
    }

    // Draw horizontal lines (along x axis)
    for z in 0..=grid.height {
        let mut min_x = grid.width;
        let mut max_x = -1;
        let mut found = false;

        for x in 0..grid.width {
            if grid.in_bounds(x, z) || grid.in_bounds(x, z - 1) {
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                found = true;
            }
        }

        if found {
            gizmos.line(
                Vec3::new(min_x as f32 - 0.5, 0.05, z as f32 - 0.5),
                Vec3::new(max_x as f32 + 0.5, 0.05, z as f32 - 0.5),
                color,
            );
        }
    }

    if let Some((grid_x, grid_z)) = mouse_grid_coords(&q_window, &q_camera) {
        if grid.in_bounds(grid_x, grid_z) {
            draw_highlighted_grid_cell(&mut gizmos, grid_x, grid_z);
        }
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

fn draw_highlighted_grid_cell(gizmos: &mut Gizmos, x: i32, z: i32) {
    let min_x = x as f32 - 0.5;
    let max_x = x as f32 + 0.5;
    let min_z = z as f32 - 0.5;
    let max_z = z as f32 + 0.5;
    let y = 0.08;
    let fill = Color::srgba(1.0, 0.0, 0.0, 0.35);
    let border = Color::srgb(1.0, 0.0, 0.0);

    for i in 0..=8 {
        let line_z = min_z + (max_z - min_z) * i as f32 / 8.0;
        gizmos.line(
            Vec3::new(min_x, y, line_z),
            Vec3::new(max_x, y, line_z),
            fill,
        );
    }

    gizmos.line(
        Vec3::new(min_x, y, min_z),
        Vec3::new(max_x, y, min_z),
        border,
    );
    gizmos.line(
        Vec3::new(max_x, y, min_z),
        Vec3::new(max_x, y, max_z),
        border,
    );
    gizmos.line(
        Vec3::new(max_x, y, max_z),
        Vec3::new(min_x, y, max_z),
        border,
    );
    gizmos.line(
        Vec3::new(min_x, y, max_z),
        Vec3::new(min_x, y, min_z),
        border,
    );
}
