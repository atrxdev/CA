use crate::game::buildings::{Building, Constructing};
use crate::game::combat::{Explosion, Projectile};
use crate::game::data::{Definitions, UnitDefinition};
use crate::game::economy::{Harvester, OreField};
use crate::game::game_state::AppState;
use crate::game::selection::{BaseMaterial, Selectable};
use crate::game::units::{Owner, Unit};
use bevy::prelude::*;

pub struct VisualsPlugin;

impl Plugin for VisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                attach_unit_visuals,
                attach_building_visuals,
                attach_projectile_visuals,
                attach_explosion_visuals,
                attach_ore_field_visuals,
            )
                .run_if(in_state(AppState::InGame)),
        );
    }
}

/// Returns a mesh handle appropriate for the unit's role.
pub fn create_unit_mesh(def: &UnitDefinition, meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
    match def.role.as_deref() {
        Some("infantry") => meshes.add(Capsule3d::new(0.25, 0.5)),
        Some("tank") => meshes.add(Cylinder::new(0.5, 0.4)),
        Some("harvester") => meshes.add(Sphere::new(0.5)),
        _ => meshes.add(Cuboid::new(0.8, 0.8, 0.8)),
    }
}

/// Returns a Transform scale appropriate for the unit's role.
pub fn unit_scale_for_role(role: Option<&str>) -> Vec3 {
    match role {
        Some("infantry") => Vec3::new(1.0, 1.0, 1.0),
        Some("tank") => Vec3::new(1.4, 1.0, 1.8),
        Some("harvester") => Vec3::new(1.2, 1.0, 1.2),
        _ => Vec3::ONE,
    }
}

/// Returns a team-tinted material using the unit definition's color, blended with team hue.
pub fn create_unit_material(
    def: &UnitDefinition,
    team: &Owner,
    materials: &mut Assets<StandardMaterial>,
) -> Handle<StandardMaterial> {
    // Use the definition's base color, tinted toward the team hue
    let base = def.color;
    let (team_r, team_g, team_b) = match team.0 {
        0 => (0.8_f32, 0.2, 0.2), // red team
        1 => (0.2, 0.2, 0.8),     // blue team
        _ => (0.5, 0.5, 0.5),
    };
    // Blend: 60% unit color, 40% team color
    let r = base[0] * 0.6 + team_r * 0.4;
    let g = base[1] * 0.6 + team_g * 0.4;
    let b = base[2] * 0.6 + team_b * 0.4;
    materials.add(StandardMaterial {
        base_color: Color::srgb(r, g, b),
        ..default()
    })
}

fn attach_unit_visuals(
    mut commands: Commands,
    mut q_units: Query<(Entity, &Unit, &Owner, Option<&Harvester>, &mut Transform), Added<Unit>>,
    definitions: Res<Definitions>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    for (entity, unit, team, _opt_harvester, mut transform) in q_units.iter_mut() {
        if let Some(def) = definitions.units.get(&unit.unit_id) {
            if let Ok(mut cmds) = commands.get_entity(entity) {
                if let Some(ref model_path) = def.model_path {
                    let scale_x = def.model_scale.unwrap_or(1.0);
                    let scale_y = def.model_scale_y.unwrap_or(scale_x);
                    let scale_z = def.model_scale.unwrap_or(1.0);
                    transform.scale = Vec3::new(scale_x, scale_y, scale_z);

                    cmds.try_insert((
                        SceneRoot(asset_server.load(format!("{}#Scene0", model_path))),
                        Selectable,
                    ));
                } else {
                    let mesh = create_unit_mesh(def, &mut meshes);
                    let material = create_unit_material(def, team, &mut materials);
                    let scale = unit_scale_for_role(def.role.as_deref());
                    transform.scale = scale;

                    cmds.try_insert((
                        Mesh3d(mesh),
                        MeshMaterial3d(material.clone()),
                        BaseMaterial(material),
                        Selectable,
                    ));
                }
            }
        }
    }
}

fn attach_building_visuals(
    mut commands: Commands,
    q_buildings: Query<(Entity, &Building, &Owner, Option<&Constructing>), Added<Building>>,
    definitions: Res<Definitions>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    for (entity, building, _team, _constructing) in q_buildings.iter() {
        if let Some(def) = definitions.buildings.get(&building.building_id) {
            if let Ok(mut cmds) = commands.get_entity(entity) {
                if let Some(ref model_path) = def.model_path {
                    cmds.try_insert((
                        SceneRoot(asset_server.load(format!("{}#Scene0", model_path))),
                        Selectable,
                    ));
                } else {
                    let mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
                    let mat = materials.add(StandardMaterial {
                        base_color: Color::srgb(def.color[0], def.color[1], def.color[2]),
                        ..default()
                    });
                    cmds.try_insert((
                        Mesh3d(mesh),
                        MeshMaterial3d(mat.clone()),
                        BaseMaterial(mat),
                        Selectable,
                    ));
                }
            }
        }
    }
}

fn attach_projectile_visuals(
    mut commands: Commands,
    q_projectiles: Query<Entity, Added<Projectile>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for entity in q_projectiles.iter() {
        let proj_mesh = meshes.add(Sphere::new(0.2));
        let proj_mat = materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 1.0, 0.0),
            emissive: LinearRgba::new(1.0, 1.0, 0.0, 1.0),
            ..default()
        });

        if let Ok(mut cmds) = commands.get_entity(entity) {
            cmds.try_insert((Mesh3d(proj_mesh), MeshMaterial3d(proj_mat)));
        }
    }
}

fn attach_explosion_visuals(
    mut commands: Commands,
    q_explosions: Query<(Entity, &Explosion), Added<Explosion>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, explosion) in q_explosions.iter() {
        let radius = if explosion.max_time > 0.6 { 2.0 } else { 1.0 }; // Hacky way to distinguish building vs unit
        let color = if explosion.max_time > 0.6 {
            Color::srgb(1.0, 0.3, 0.0)
        } else {
            Color::srgb(1.0, 0.5, 0.0)
        };
        let emissive = if explosion.max_time > 0.6 {
            LinearRgba::new(1.0, 0.3, 0.0, 1.0)
        } else {
            LinearRgba::new(1.0, 0.5, 0.0, 1.0)
        };

        let exp_mesh = meshes.add(Sphere::new(radius));
        let exp_mat = materials.add(StandardMaterial {
            base_color: color,
            emissive,
            alpha_mode: AlphaMode::Blend,
            ..default()
        });

        if let Ok(mut cmds) = commands.get_entity(entity) {
            cmds.try_insert((Mesh3d(exp_mesh), MeshMaterial3d(exp_mat)));
        }
    }
}

fn attach_ore_field_visuals(
    mut commands: Commands,
    mut q_ore: Query<(Entity, &OreField, &mut Transform), Added<OreField>>,
    definitions: Res<Definitions>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    for (entity, ore_field, mut transform) in q_ore.iter_mut() {
        if let Some(def) = definitions.resources.get(&ore_field.resource_id) {
            if let Ok(mut cmds) = commands.get_entity(entity) {
                if let Some(ref model_path) = def.model_path {
                    let scale = def.model_scale.unwrap_or(1.0);
                    transform.scale = Vec3::splat(scale);
                    cmds.try_insert((
                        SceneRoot(asset_server.load(format!("{}#Scene0", model_path))),
                        Selectable,
                    ));
                } else {
                    let mesh = meshes.add(Cuboid::new(0.8, 0.1, 0.8));
                    let mat = materials.add(StandardMaterial {
                        base_color: Color::srgb(def.color[0], def.color[1], def.color[2]),
                        ..default()
                    });
                    cmds.try_insert((
                        Mesh3d(mesh),
                        MeshMaterial3d(mat.clone()),
                        BaseMaterial(mat),
                        Selectable,
                    ));
                }
            }
        }
    }
}
