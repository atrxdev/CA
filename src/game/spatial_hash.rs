use crate::game::units::Unit;
use bevy::prelude::*;
use std::collections::HashMap;

/// Cell size for the spatial hash. Units are bucketed into cells of this width.
/// Larger than the unit collision separation (1.0) so a single neighbor-cell
/// query covers all possible collisions.
const CELL_SIZE: f32 = 2.0;

/// Spatial hash grid that maps (cell_x, cell_z) -> list of entities in that cell.
/// Rebuilt every frame in `rebuild_spatial_hash`.
#[derive(Resource, Default)]
pub struct SpatialHash {
    pub cells: HashMap<(i32, i32), Vec<Entity>>,
    pub cell_size: f32,
}

impl SpatialHash {
    pub fn new() -> Self {
        Self {
            cells: HashMap::new(),
            cell_size: CELL_SIZE,
        }
    }

    /// Convert a world position to a cell coordinate.
    #[inline]
    pub fn cell_coords(&self, x: f32, z: f32) -> (i32, i32) {
        (
            (x / self.cell_size).floor() as i32,
            (z / self.cell_size).floor() as i32,
        )
    }

    /// Insert an entity at the given world position.
    #[inline]
    pub fn insert(&mut self, entity: Entity, x: f32, z: f32) {
        let cell = self.cell_coords(x, z);
        self.cells.entry(cell).or_default().push(entity);
    }

    /// Clear all entries for the next frame rebuild.
    #[inline]
    pub fn clear(&mut self) {
        for v in self.cells.values_mut() {
            v.clear();
        }
    }

    /// Query all entities within `radius` of the given world position.
    /// Returns entities from all cells that could overlap the query circle.
    /// The caller must still do a precise distance check.
    pub fn query_radius(&self, x: f32, z: f32, radius: f32) -> impl Iterator<Item = Entity> + '_ {
        let min_cell = self.cell_coords(x - radius, z - radius);
        let max_cell = self.cell_coords(x + radius, z + radius);

        (min_cell.0..=max_cell.0).flat_map(move |cx| {
            (min_cell.1..=max_cell.1).flat_map(move |cz| {
                self.cells
                    .get(&(cx, cz))
                    .into_iter()
                    .flat_map(|v| v.iter().copied())
            })
        })
    }
}

/// System that rebuilds the spatial hash each frame from all unit positions.
pub fn rebuild_spatial_hash(
    mut spatial_hash: ResMut<SpatialHash>,
    q_units: Query<(Entity, &Transform), With<Unit>>,
) {
    spatial_hash.clear();
    for (entity, transform) in q_units.iter() {
        spatial_hash.insert(entity, transform.translation.x, transform.translation.z);
    }
}
