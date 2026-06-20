use crate::game::game_state::AppState;
use crate::game::units::Unit;
use bevy::prelude::*;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

pub struct PathfindingPlugin;

impl Plugin for PathfindingPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Grid::new(128, 128, 128, 128))
            .add_systems(
                PreUpdate,
                update_dynamic_grid_blocking.run_if(in_state(AppState::InGame)),
            );
    }
}

#[derive(Component)]
pub struct Path {
    pub waypoints: Vec<Vec2>,
}

#[derive(Resource)]
pub struct Grid {
    pub width: i32,
    pub height: i32,
    pub screen_width: i32,
    pub screen_height: i32,
    pub movement_costs: Vec<f32>,
    pub static_blocked: Vec<bool>,
    pub blocked: Vec<bool>,
}

impl Grid {
    pub fn new(width: i32, height: i32, screen_width: i32, screen_height: i32) -> Self {
        Self {
            width,
            height,
            screen_width,
            screen_height,
            movement_costs: vec![1.0; (width * height) as usize],
            static_blocked: vec![false; (width * height) as usize],
            blocked: vec![false; (width * height) as usize],
        }
    }

    pub fn set_blocked(&mut self, x: i32, y: i32, is_blocked: bool) {
        if self.in_bounds(x, y) {
            let idx = (y * self.width + x) as usize;
            self.static_blocked[idx] = is_blocked;
            self.blocked[idx] = is_blocked;
        }
    }

    pub fn reset_dynamic_blocked(&mut self) {
        self.blocked.copy_from_slice(&self.static_blocked);
    }

    pub fn set_dynamic_blocked(&mut self, x: i32, y: i32, is_blocked: bool) {
        if self.in_bounds(x, y) {
            self.blocked[(y * self.width + x) as usize] = is_blocked;
        }
    }

    pub fn set_movement_cost(&mut self, x: i32, y: i32, movement_cost: f32) {
        if self.in_bounds(x, y) {
            self.movement_costs[(y * self.width + x) as usize] = movement_cost.max(0.1);
        }
    }

    pub fn movement_cost(&self, x: i32, y: i32) -> f32 {
        if !self.in_bounds(x, y) {
            return f32::INFINITY;
        }
        self.movement_costs[(y * self.width + x) as usize]
    }

    pub fn is_blocked(&self, x: i32, y: i32) -> bool {
        if !self.in_bounds(x, y) {
            return true;
        }
        self.blocked[(y * self.width + x) as usize]
    }

    pub fn is_statically_blocked(&self, x: i32, y: i32) -> bool {
        if !self.in_bounds(x, y) {
            return true;
        }
        self.static_blocked[(y * self.width + x) as usize]
    }

    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        if x < 0 || x >= self.width || y < 0 || y >= self.height {
            return false;
        }

        let cx = self.width / 2;
        let cy = self.height / 2;
        let dx = x - cx;
        let dy = y - cy;

        (dx - dy).abs() <= self.screen_width && (dx + dy).abs() <= self.screen_height
    }
}

fn update_dynamic_grid_blocking(
    mut grid: ResMut<Grid>,
    q_idle_units: Query<&Transform, (With<Unit>, Without<Path>)>,
) {
    grid.reset_dynamic_blocked();
    for transform in q_idle_units.iter() {
        let gx = transform.translation.x.round() as i32;
        let gz = transform.translation.z.round() as i32;
        grid.set_dynamic_blocked(gx, gz, true);
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
struct State {
    cost: u32,
    position: (i32, i32),
}

impl Ord for State {
    fn cmp(&self, other: &Self) -> Ordering {
        other.cost.cmp(&self.cost) // Min-heap
    }
}

impl PartialOrd for State {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn heuristic(a: (i32, i32), b: (i32, i32)) -> u32 {
    ((a.0 - b.0).abs() + (a.1 - b.1).abs()) as u32
}

pub fn nearest_available_destination(start: Vec2, requested: Vec2, grid: &Grid) -> Option<Vec2> {
    const MAX_FALLBACK_RADIUS: i32 = 12;

    let requested_cell = (requested.x.round() as i32, requested.y.round() as i32);
    if !grid.in_bounds(requested_cell.0, requested_cell.1) {
        return None;
    }
    if !grid.is_blocked(requested_cell.0, requested_cell.1) {
        return Some(Vec2::new(requested_cell.0 as f32, requested_cell.1 as f32));
    }

    for radius in 1..=MAX_FALLBACK_RADIUS {
        let mut best = None;
        let mut best_distance = f32::MAX;

        for dz in -radius..=radius {
            for dx in -radius..=radius {
                if dx.abs().max(dz.abs()) != radius {
                    continue;
                }

                let cell = (requested_cell.0 + dx, requested_cell.1 + dz);
                if !grid.in_bounds(cell.0, cell.1) || grid.is_blocked(cell.0, cell.1) {
                    continue;
                }

                let candidate = Vec2::new(cell.0 as f32, cell.1 as f32);
                let distance = candidate.distance_squared(start);
                if distance < best_distance {
                    best = Some(candidate);
                    best_distance = distance;
                }
            }
        }

        if best.is_some() {
            return best;
        }
    }

    None
}

pub fn find_path(start: Vec2, end: Vec2, grid: &Grid) -> Option<Vec<Vec2>> {
    let start_pos = (start.x.round() as i32, start.y.round() as i32);
    let requested_end = (end.x.round() as i32, end.y.round() as i32);

    if !grid.in_bounds(start_pos.0, start_pos.1)
        || !grid.in_bounds(requested_end.0, requested_end.1)
    {
        return None;
    }

    let resolved_end = nearest_available_destination(start, end, grid)?;
    let end_pos = (resolved_end.x as i32, resolved_end.y as i32);

    let mut heap = BinaryHeap::new();
    let mut came_from: HashMap<(i32, i32), (i32, i32)> = HashMap::new();
    let mut cost_so_far: HashMap<(i32, i32), u32> = HashMap::new();

    heap.push(State {
        cost: 0,
        position: start_pos,
    });
    cost_so_far.insert(start_pos, 0);

    let mut found = false;
    let mut iterations = 0;

    while let Some(State { position, .. }) = heap.pop() {
        iterations += 1;
        if iterations > 3000 {
            break;
        }

        if position == end_pos {
            found = true;
            break;
        }

        for (dx, dy) in &[
            (0, 1),
            (1, 0),
            (0, -1),
            (-1, 0),
            (1, 1),
            (-1, -1),
            (1, -1),
            (-1, 1),
        ] {
            let next = (position.0 + dx, position.1 + dy);

            if grid.is_blocked(next.0, next.1) {
                continue;
            }

            // Quick check for diagonal movement to prevent cutting corners
            if dx.abs() == 1 && dy.abs() == 1 {
                if grid.is_blocked(position.0 + dx, position.1)
                    || grid.is_blocked(position.0, position.1 + dy)
                {
                    continue;
                }
            }

            let base_cost = if dx.abs() + dy.abs() == 2 { 14.0 } else { 10.0 };
            let movement_cost = grid.movement_cost(next.0, next.1);
            let new_cost = cost_so_far[&position] + (base_cost * movement_cost).round() as u32;

            if !cost_so_far.contains_key(&next) || new_cost < cost_so_far[&next] {
                cost_so_far.insert(next, new_cost);
                let priority = new_cost + heuristic(next, end_pos) * 10;
                heap.push(State {
                    cost: priority,
                    position: next,
                });
                came_from.insert(next, position);
            }
        }
    }

    if found {
        let mut path = Vec::new();
        let mut current = end_pos;
        while current != start_pos {
            path.push(Vec2::new(current.0 as f32, current.1 as f32));
            current = came_from[&current];
        }
        path.reverse();
        Some(simplify_collinear_path(start_pos, path))
    } else {
        None
    }
}

/// Removes intermediate cells along straight path segments. Keeping every A*
/// cell as a waypoint makes units brake and turn once per tile, which becomes
/// especially visible when a large formation moves together.
fn simplify_collinear_path(start: (i32, i32), path: Vec<Vec2>) -> Vec<Vec2> {
    if path.len() < 2 {
        return path;
    }

    let mut simplified = Vec::with_capacity(path.len());
    let mut previous = start;

    for index in 0..path.len() {
        let current = (path[index].x as i32, path[index].y as i32);
        let current_direction = (
            (current.0 - previous.0).signum(),
            (current.1 - previous.1).signum(),
        );
        let is_last = index + 1 == path.len();

        if is_last {
            simplified.push(path[index]);
            break;
        }

        let next = (path[index + 1].x as i32, path[index + 1].y as i32);
        let next_direction = ((next.0 - current.0).signum(), (next.1 - current.1).signum());
        if current_direction != next_direction {
            simplified.push(path[index]);
        }
        previous = current;
    }

    simplified
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocked_destination_uses_nearest_available_cell() {
        let mut grid = Grid::new(32, 32, 32, 32);
        grid.set_dynamic_blocked(10, 10, true);

        let destination =
            nearest_available_destination(Vec2::new(7.0, 10.0), Vec2::new(10.0, 10.0), &grid);

        assert_eq!(destination, Some(Vec2::new(9.0, 10.0)));
    }

    #[test]
    fn straight_paths_only_keep_the_destination() {
        let grid = Grid::new(32, 32, 32, 32);
        let path = find_path(Vec2::new(2.0, 2.0), Vec2::new(10.0, 2.0), &grid).unwrap();

        assert_eq!(path, vec![Vec2::new(10.0, 2.0)]);
    }

    #[test]
    fn path_keeps_only_actual_turns() {
        let simplified = simplify_collinear_path(
            (0, 0),
            vec![
                Vec2::new(1.0, 0.0),
                Vec2::new(2.0, 0.0),
                Vec2::new(2.0, 1.0),
                Vec2::new(2.0, 2.0),
            ],
        );

        assert_eq!(simplified, vec![Vec2::new(2.0, 0.0), Vec2::new(2.0, 2.0)]);
    }
}
