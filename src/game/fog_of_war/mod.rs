use crate::game::buildings::Building;
use crate::game::economy::OreField;
use crate::game::game_state::AppState;
use crate::game::units::{Owner, Unit};
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use serde::{Deserialize, Serialize};

pub struct FogOfWarPlugin;

impl Plugin for FogOfWarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                update_fog_of_war,
                update_entity_visibility_in_fog.after(update_fog_of_war),
            )
                .run_if(in_state(AppState::InGame))
                .run_if(resource_exists::<FogOfWar>)
                .run_if(resource_exists::<FogUpdateTimer>)
                .run_if(resource_exists::<FogTexture>)
                .run_if(resource_exists::<FogMaterial>),
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum VisibilityState {
    #[default]
    Unexplored,
    Explored,
    Visible,
}

#[derive(Resource)]
pub struct FogOfWar {
    pub width: i32,
    pub height: i32,
    pub screen_width: i32,
    pub screen_height: i32,
    pub states: Vec<VisibilityState>,
    pub is_disabled: bool,
}

impl FogOfWar {
    pub fn new(width: i32, height: i32, screen_width: i32, screen_height: i32) -> Self {
        Self {
            width,
            height,
            screen_width,
            screen_height,
            states: vec![VisibilityState::Unexplored; (width * height) as usize],
            is_disabled: false,
        }
    }

    pub fn get_state(&self, x: i32, y: i32) -> VisibilityState {
        if x >= 0 && x < self.width && y >= 0 && y < self.height {
            self.states[(y * self.width + x) as usize]
        } else {
            VisibilityState::Unexplored
        }
    }

    pub fn set_state(&mut self, x: i32, y: i32, state: VisibilityState) {
        if x >= 0 && x < self.width && y >= 0 && y < self.height {
            self.states[(y * self.width + x) as usize] = state;
        }
    }
}

#[derive(Resource)]
pub struct FogTexture {
    pub handle: Handle<Image>,
}

#[derive(Resource)]
pub struct FogMaterial {
    pub handle: Handle<StandardMaterial>,
}

#[derive(Resource)]
pub struct FogUpdateTimer(pub Timer);

#[derive(Component, Clone, Copy, Debug)]
pub struct Vision {
    pub range: f32,
}

pub fn setup_fog_of_war(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
    logical_size: i32,
    screen_width: i32,
    screen_height: i32,
) {
    let width = logical_size;
    let height = logical_size;

    let mut initial_data = vec![0u8; (width * height * 4) as usize];
    let cx = width as i32 / 2;
    let cz = height as i32 / 2;

    for z in 0..height {
        for x in 0..width {
            let i = (z * width + x) as usize;
            let dx = x as i32 - cx;
            let dz = z as i32 - cz;
            if (dx - dz).abs() <= screen_width && (dx + dz).abs() <= screen_height {
                initial_data[i * 4 + 3] = 255; // Fully opaque black initially for unexplored areas
            } else {
                initial_data[i * 4 + 3] = 0; // Transparent outside map bounds
            }
        }
    }

    let image = Image::new(
        Extent3d {
            width: width as u32,
            height: height as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        initial_data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );

    let image_handle = images.add(image);

    let material_handle = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(image_handle.clone()),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });

    // Spawn plane slightly above ground at Y = 0.05
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::new(
            Vec3::Y,
            Vec2::splat(logical_size as f32 / 2.0),
        ))),
        MeshMaterial3d(material_handle.clone()),
        Transform::from_xyz(
            logical_size as f32 / 2.0 - 0.5,
            0.05,
            logical_size as f32 / 2.0 - 0.5,
        ),
        bevy::light::NotShadowCaster,
        bevy::light::NotShadowReceiver,
    ));

    commands.insert_resource(FogOfWar::new(width, height, screen_width, screen_height));
    commands.insert_resource(FogTexture {
        handle: image_handle,
    });
    commands.insert_resource(FogMaterial {
        handle: material_handle,
    });
    commands.insert_resource(FogUpdateTimer(Timer::from_seconds(
        0.1,
        TimerMode::Repeating,
    )));
}

fn update_fog_of_war(
    time: Res<Time>,
    mut timer: ResMut<FogUpdateTimer>,
    mut fog: ResMut<FogOfWar>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    fog_texture: Res<FogTexture>,
    fog_material: Res<FogMaterial>,
    q_vision: Query<(&Transform, &Vision, &Owner)>,
    local_player: Res<crate::game::player::LocalPlayer>,
) {
    if !timer.0.tick(time.delta()).just_finished() {
        return;
    }

    if fog.is_disabled {
        for state in fog.states.iter_mut() {
            *state = VisibilityState::Visible;
        }
    } else {
        // 1. Reset currently Visible tiles to Explored
        for state in fog.states.iter_mut() {
            if *state == VisibilityState::Visible {
                *state = VisibilityState::Explored;
            }
        }

        // 2. Mark tiles visible from local player vision sources
        for (transform, vision, team) in q_vision.iter() {
            if team.0 != local_player.0 {
                continue;
            }

            let pos = transform.translation;
            let gx = pos.x.round() as i32;
            let gz = pos.z.round() as i32;
            let r = vision.range;
            let r_sq = r * r;

            let min_x = (gx - r.ceil() as i32).max(0);
            let max_x = (gx + r.ceil() as i32).min(fog.width - 1);
            let min_z = (gz - r.ceil() as i32).max(0);
            let max_z = (gz + r.ceil() as i32).min(fog.height - 1);

            for z in min_z..=max_z {
                for x in min_x..=max_x {
                    let dx = x - gx;
                    let dz = z - gz;
                    if (dx * dx + dz * dz) as f32 <= r_sq {
                        fog.set_state(x, z, VisibilityState::Visible);
                    }
                }
            }
        }
    }

    // 3. Update texture image data
    if let Some(image) = images.get_mut(&fog_texture.handle) {
        if let Some(ref mut data) = image.data {
            let cx = fog.width / 2;
            let cz = fog.height / 2;

            for z in 0..fog.height {
                for x in 0..fog.width {
                    let idx = (z * fog.width + x) as usize;
                    let pixel_offset = idx * 4;

                    let dx = x - cx;
                    let dz = z - cz;
                    if (dx - dz).abs() > fog.screen_width || (dx + dz).abs() > fog.screen_height {
                        data[pixel_offset + 3] = 0; // Transparent outside bounds
                        continue;
                    }

                    let state = fog.states[idx];
                    let alpha = match state {
                        VisibilityState::Unexplored => 255,
                        VisibilityState::Explored => 150,
                        VisibilityState::Visible => 0,
                    };
                    data[pixel_offset + 3] = alpha;
                }
            }
        }
    }

    // Force material refresh to signal renderer to upload dynamic texture to GPU
    let _ = materials.get_mut(&fog_material.handle);
}

fn update_entity_visibility_in_fog(
    fog: Res<FogOfWar>,
    mut q_units: Query<(&Transform, &Owner, &mut Visibility), (With<Unit>, Without<Building>)>,
    mut q_buildings: Query<(&Transform, &Owner, &mut Visibility), With<Building>>,
    mut q_ore: Query<
        (&Transform, &mut Visibility),
        (With<OreField>, Without<Unit>, Without<Building>),
    >,
    local_player: Res<crate::game::player::LocalPlayer>,
) {
    // 1. Units
    for (transform, team, mut visibility) in q_units.iter_mut() {
        if team.0 == local_player.0 {
            // Player units are always visible
            if *visibility != Visibility::Inherited {
                *visibility = Visibility::Inherited;
            }
            continue;
        }

        let gx = transform.translation.x.round() as i32;
        let gz = transform.translation.z.round() as i32;
        let state = fog.get_state(gx, gz);

        let expected_vis = if state == VisibilityState::Visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };

        if *visibility != expected_vis {
            *visibility = expected_vis;
        }
    }

    // 2. Buildings
    for (transform, team, mut visibility) in q_buildings.iter_mut() {
        if team.0 == local_player.0 {
            // Player buildings are always visible
            if *visibility != Visibility::Inherited {
                *visibility = Visibility::Inherited;
            }
            continue;
        }

        let gx = transform.translation.x.round() as i32;
        let gz = transform.translation.z.round() as i32;
        let state = fog.get_state(gx, gz);

        // Enemy buildings are visible in Explored or Visible regions
        let expected_vis = if state != VisibilityState::Unexplored {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };

        if *visibility != expected_vis {
            *visibility = expected_vis;
        }
    }

    // 3. Ore fields
    for (transform, mut visibility) in q_ore.iter_mut() {
        let gx = transform.translation.x.round() as i32;
        let gz = transform.translation.z.round() as i32;
        let state = fog.get_state(gx, gz);

        // Ore is visible if explored or visible
        let expected_vis = if state != VisibilityState::Unexplored {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };

        if *visibility != expected_vis {
            *visibility = expected_vis;
        }
    }
}
