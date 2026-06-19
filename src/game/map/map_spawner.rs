use crate::game::map::map_asset::Map;
use crate::game::map::map_chunk::{CHUNK_SIZE, Cell};
use crate::game::map::terrain::{DEFAULT_TERRAIN_ID, TerrainDefinition};
use crate::game::map::{MapBounds, MapSelection, MinimapData};
use crate::game::pathfinding::Grid;
use bevy::prelude::*;
use std::collections::HashMap;

#[derive(Resource)]
pub struct MapLoadingHandle(pub Handle<Map>);

pub fn start_loading_map(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    map_selection: Res<MapSelection>,
) {
    let map_path = if map_selection.0.is_empty() {
        "maps/desert_dunes.ron".to_string() // Fallback
    } else {
        format!("maps/{}", map_selection.0)
    };

    let handle: Handle<Map> = asset_server.load(&map_path);
    commands.insert_resource(MapLoadingHandle(handle));
}

pub fn spawn_map_when_loaded(
    mut commands: Commands,
    handle: Option<Res<MapLoadingHandle>>,
    map_assets: Res<Assets<Map>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut grid: ResMut<Grid>,
    definitions: Res<crate::game::data::Definitions>,
    players_res: Res<crate::game::player::Players>,
    asset_server: Res<AssetServer>,
    local_player: Res<crate::game::player::LocalPlayer>,
    mut q_camera: Query<&mut Transform, With<crate::game::camera::RtsCamera>>,
) {
    if let Some(handle_res) = handle {
        if let Some(map) = map_assets.get(&handle_res.0) {
            // Map is loaded! Time to spawn it.
            commands.remove_resource::<MapLoadingHandle>();

            info!("Spawning Map: {}", map.metadata.name);

            // A logical grid based on map size
            let logical_size = map.width + map.height;
            let cx = logical_size as f32 / 2.0;
            let cz = logical_size as f32 / 2.0;
            commands.insert_resource(MapBounds::new(
                Vec2::new(cx, cz),
                map.width as f32,
                map.height as f32,
            ));

            *grid = crate::game::pathfinding::Grid::new(
                logical_size as i32,
                logical_size as i32,
                map.width as i32,
                map.height as i32,
            );

            crate::game::fog_of_war::setup_fog_of_war(
                &mut commands,
                &mut meshes,
                &mut materials,
                &mut images,
                logical_size as i32,
                map.width as i32,
                map.height as i32,
            );

            spawn_terrain(
                &mut commands,
                map,
                &definitions.terrain,
                &mut meshes,
                &mut materials,
                &mut grid,
                cx,
                cz,
            );
            commands.insert_resource(build_minimap_data(map, &definitions.terrain));

            // Setup the pathfinding grid size if Grid supports dynamic sizing?
            // Currently Grid is a Resource. Let's just assume we update the existing grid, or it's statically sized at 128x128.
            // Removed unused variables

            for node in &map.resource_nodes {
                let (world_x, world_z) = map_to_world(map, node.x, node.y, cx, cz);
                let pos = Vec3::new(world_x, 0.1, world_z);

                if let Some(res_def) = definitions.resources.get(&node.resource_id) {
                    let amount = node.amount.unwrap_or(res_def.amount);

                    commands.spawn((
                        crate::game::economy::OreField {
                            resource_id: node.resource_id.clone(),
                            amount,
                        },
                        Transform::from_translation(pos),
                    ));
                } else {
                    println!(
                        "Warning: Resource definition not found: {}",
                        node.resource_id
                    );
                }
            }

            // Directional light for the scene
            commands.spawn((
                DirectionalLight {
                    illuminance: 5000.0,
                    shadows_enabled: true,
                    ..default()
                },
                Transform::from_xyz(cx - 0.5, 50.0, cz - 0.5)
                    .looking_at(Vec3::new(cx - 0.5, 0.0, cz - 0.5), Vec3::Y),
            ));

            // Spawn teams based on starting positions
            let player_inf_offsets: Vec<Vec3> = (0..5)
                .map(|i| Vec3::new((i as f32 * 2.0) - 4.0, 0.0, -5.0))
                .collect();
            let player_tank_offsets: Vec<Vec3> = (0..4)
                .map(|i| Vec3::new((i as f32 * 3.0) - 4.5, 0.0, -8.0))
                .collect();
            let ai_inf_offsets: Vec<Vec3> = (0..5)
                .map(|i| Vec3::new(-(i as f32 * 2.0) + 4.0, 0.0, 5.0))
                .collect();
            let ai_tank_offsets: Vec<Vec3> = (0..4)
                .map(|i| Vec3::new(-(i as f32 * 3.0) + 4.5, 0.0, 8.0))
                .collect();

            for (i, start_pos) in map.starting_positions.iter().enumerate() {
                let player_id = i;

                // Skip if player is None
                if let Some(player) = players_res.players.get(&player_id) {
                    if matches!(
                        player.controller,
                        crate::game::player::PlayerController::None
                    ) {
                        continue;
                    }
                } else {
                    continue; // Skip if no player configured
                }

                let (world_x, world_z) = map_to_world(map, start_pos.x, start_pos.y, cx, cz);
                let pos = Vec3::new(world_x, 0.5, world_z);
                let inf_offsets = if player_id == local_player.0 {
                    player_inf_offsets.clone()
                } else {
                    ai_inf_offsets.clone()
                };
                let tank_offsets = if player_id == local_player.0 {
                    player_tank_offsets.clone()
                } else {
                    ai_tank_offsets.clone()
                };

                if player_id == local_player.0 {
                    if let Some(mut cam_transform) = q_camera.iter_mut().next() {
                        let yaw = 45.0_f32.to_radians();
                        let pitch = -55.0_f32.to_radians();
                        let rotation = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
                        let forward = rotation * Vec3::NEG_Z;
                        let distance = 50.0 / forward.y.abs();
                        cam_transform.translation = pos - forward * distance;
                        cam_transform.rotation = rotation;
                    }
                }

                crate::game::units::spawn_faction_base(
                    &mut commands,
                    &mut materials,
                    &definitions,
                    &mut grid,
                    &players_res,
                    &asset_server,
                    player_id,
                    pos,
                    inf_offsets,
                    tank_offsets,
                );
            }
        }
    }
}

fn spawn_terrain(
    commands: &mut Commands,
    map: &Map,
    terrain_defs: &HashMap<String, TerrainDefinition>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    grid: &mut Grid,
    cx: f32,
    cz: f32,
) {
    let mut terrain_materials: HashMap<String, Handle<StandardMaterial>> = HashMap::new();

    let base_terrain_id = single_cell_terrain(map)
        .map(|cell| cell.terrain.as_str())
        .unwrap_or(map.default_terrain.as_str());
    let base_terrain = terrain_def(terrain_defs, base_terrain_id);
    let base_material = terrain_material(&mut terrain_materials, materials, base_terrain);

    commands.spawn((
        Mesh3d(meshes.add(Plane3d::new(
            Vec3::Y,
            Vec2::new(
                map.width as f32 / std::f32::consts::SQRT_2,
                map.height as f32 / std::f32::consts::SQRT_2,
            ),
        ))),
        MeshMaterial3d(base_material),
        Transform::from_xyz(cx - 0.5, 0.0, cz - 0.5)
            .with_rotation(Quat::from_rotation_y(45.0_f32.to_radians())),
    ));

    apply_terrain_to_map_area(grid, map, base_terrain, cx, cz);

    if single_cell_terrain(map).is_some() {
        return;
    }

    let tile_mesh = meshes.add(Plane3d::new(
        Vec3::Y,
        Vec2::splat(1.0 / std::f32::consts::SQRT_2),
    ));

    for chunk in &map.chunks {
        for (idx, cell) in chunk.cells.iter().enumerate() {
            let map_x = chunk.x * CHUNK_SIZE as i32
                + cell.x.map_or((idx % CHUNK_SIZE) as i32, |x| x as i32);
            let map_y = chunk.y * CHUNK_SIZE as i32
                + cell.y.map_or((idx / CHUNK_SIZE) as i32, |y| y as i32);

            if map_x < 0 || map_y < 0 || map_x >= map.width as i32 || map_y >= map.height as i32 {
                continue;
            }

            let terrain = terrain_def(terrain_defs, &cell.terrain);
            let material = terrain_material(&mut terrain_materials, materials, terrain);
            let (world_x, world_z) = map_to_world(map, map_x as u32, map_y as u32, cx, cz);

            commands.spawn((
                Mesh3d(tile_mesh.clone()),
                MeshMaterial3d(material),
                Transform::from_xyz(world_x, 0.02, world_z)
                    .with_rotation(Quat::from_rotation_y(45.0_f32.to_radians())),
            ));

            if !terrain.passable {
                grid.set_blocked(world_x.round() as i32, world_z.round() as i32, true);
            }
            grid.set_movement_cost(
                world_x.round() as i32,
                world_z.round() as i32,
                terrain.movement_cost,
            );
        }
    }
}

fn single_cell_terrain(map: &Map) -> Option<&Cell> {
    if map.chunks.len() == 1 && map.chunks[0].cells.len() == 1 {
        map.chunks[0].cells.first()
    } else {
        None
    }
}

fn terrain_def<'a>(
    terrain_defs: &'a HashMap<String, TerrainDefinition>,
    terrain_id: &str,
) -> &'a TerrainDefinition {
    terrain_defs
        .get(terrain_id)
        .or_else(|| terrain_defs.get(DEFAULT_TERRAIN_ID))
        .expect("default terrain definition should be loaded")
}

fn terrain_material(
    terrain_materials: &mut HashMap<String, Handle<StandardMaterial>>,
    materials: &mut Assets<StandardMaterial>,
    terrain: &TerrainDefinition,
) -> Handle<StandardMaterial> {
    terrain_materials
        .entry(terrain.id.clone())
        .or_insert_with(|| {
            materials.add(StandardMaterial {
                base_color: Color::srgb(terrain.color[0], terrain.color[1], terrain.color[2]),
                ..default()
            })
        })
        .clone()
}

fn apply_terrain_to_map_area(
    grid: &mut Grid,
    map: &Map,
    terrain: &TerrainDefinition,
    cx: f32,
    cz: f32,
) {
    for map_y in 0..map.height {
        for map_x in 0..map.width {
            let (world_x, world_z) = map_to_world(map, map_x, map_y, cx, cz);
            let grid_x = world_x.round() as i32;
            let grid_z = world_z.round() as i32;
            if !terrain.passable {
                grid.set_blocked(grid_x, grid_z, true);
            }
            grid.set_movement_cost(grid_x, grid_z, terrain.movement_cost);
        }
    }
}

fn build_minimap_data(map: &Map, terrain_defs: &HashMap<String, TerrainDefinition>) -> MinimapData {
    let base_terrain = terrain_def(terrain_defs, &map.default_terrain);
    let mut terrain_colors = vec![
        Color::srgb(
            base_terrain.color[0],
            base_terrain.color[1],
            base_terrain.color[2],
        );
        (map.width * map.height) as usize
    ];

    if let Some(cell) = single_cell_terrain(map) {
        let terrain = terrain_def(terrain_defs, &cell.terrain);
        terrain_colors.fill(Color::srgb(
            terrain.color[0],
            terrain.color[1],
            terrain.color[2],
        ));
        return MinimapData {
            width: map.width,
            height: map.height,
            terrain_colors,
        };
    }

    for chunk in &map.chunks {
        for (idx, cell) in chunk.cells.iter().enumerate() {
            let map_x = chunk.x * CHUNK_SIZE as i32
                + cell.x.map_or((idx % CHUNK_SIZE) as i32, |x| x as i32);
            let map_y = chunk.y * CHUNK_SIZE as i32
                + cell.y.map_or((idx / CHUNK_SIZE) as i32, |y| y as i32);

            if map_x < 0 || map_y < 0 || map_x >= map.width as i32 || map_y >= map.height as i32 {
                continue;
            }

            let terrain = terrain_def(terrain_defs, &cell.terrain);
            let index = (map_y as u32 * map.width + map_x as u32) as usize;
            terrain_colors[index] =
                Color::srgb(terrain.color[0], terrain.color[1], terrain.color[2]);
        }
    }

    MinimapData {
        width: map.width,
        height: map.height,
        terrain_colors,
    }
}

fn map_to_world(map: &Map, x: u32, y: u32, cx: f32, cz: f32) -> (f32, f32) {
    let sx_rel = x as f32 - map.width as f32 / 2.0;
    let sy_rel = y as f32 - map.height as f32 / 2.0;
    let dx = sx_rel + sy_rel;
    let dz = sy_rel - sx_rel;
    (cx + dx, cz + dz)
}
