use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::game::camera::RtsCamera;
use crate::game::data::Definitions;
use crate::game::fog_of_war::FogOfWar;
use crate::game::game_state::AppState;
use crate::game::player::{LocalPlayer, Players};
use crate::game::units::{Owner, Unit, spawn_unit_of_type};

pub struct ConsolePlugin;

impl Plugin for ConsolePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ConsoleData>()
            .add_systems(OnEnter(AppState::InGame), setup_console)
            .add_systems(
                Update,
                (toggle_console, handle_console_input, update_console_ui)
                    .run_if(in_state(AppState::InGame))
                    .run_if(resource_exists::<FogOfWar>),
            );
    }
}

#[derive(Resource, Default)]
pub struct ConsoleData {
    pub is_open: bool,
    pub input_buffer: String,
    pub log: Vec<String>,
    pub history: Vec<String>,
    pub history_index: Option<usize>,
}

impl ConsoleData {
    pub fn print(&mut self, text: &str) {
        self.log.push(text.to_string());
        if self.log.len() > 50 {
            self.log.remove(0);
        }
    }
}

#[derive(Component)]
struct ConsoleRoot;

#[derive(Component)]
struct ConsoleLogText;

#[derive(Component)]
struct ConsoleInputText;

fn setup_console(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(0.0),
                left: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(40.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(10.0)),
                display: Display::None,
                ..default()
            },
            ZIndex(1000), // Very high to sit on top of everything
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.85)),
            ConsoleRoot,
        ))
        .with_children(|parent| {
            // Log Area
            parent
                .spawn((Node {
                    width: Val::Percent(100.0),
                    flex_grow: 1.0,
                    flex_direction: FlexDirection::Column,
                    overflow: Overflow::clip_y(),
                    justify_content: JustifyContent::FlexEnd,
                    ..default()
                },))
                .with_children(|log_container| {
                    log_container.spawn((
                        Text::new(""),
                        TextFont {
                            font_size: 16.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.8, 0.8, 0.8)),
                        ConsoleLogText,
                    ));
                });

            // Input Line
            parent
                .spawn((Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(30.0),
                    align_items: AlignItems::Center,
                    margin: UiRect::top(Val::Px(5.0)),
                    ..default()
                },))
                .with_children(|input_container| {
                    input_container.spawn((
                        Text::new("> "),
                        TextFont {
                            font_size: 18.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                    input_container.spawn((
                        Text::new(""),
                        TextFont {
                            font_size: 18.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.9, 0.9, 1.0)),
                        ConsoleInputText,
                    ));
                });
        });
}

fn toggle_console(
    mut keyboard_input_events: MessageReader<KeyboardInput>,
    mut console_data: ResMut<ConsoleData>,
    mut q_console: Query<&mut Node, With<ConsoleRoot>>,
) {
    for event in keyboard_input_events.read() {
        if event.state == ButtonState::Pressed {
            if let Key::Character(ref c) = event.logical_key {
                if c == "`" || c == "~" {
                    console_data.is_open = !console_data.is_open;

                    if let Some(mut node) = q_console.iter_mut().next() {
                        node.display = if console_data.is_open {
                            Display::Flex
                        } else {
                            Display::None
                        };
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_console_input(
    mut commands: Commands,
    mut keyboard_input_events: MessageReader<KeyboardInput>,
    mut console_data: ResMut<ConsoleData>,
    mut players: ResMut<Players>,
    local_player: Res<LocalPlayer>,
    definitions: Res<Definitions>,
    q_units: Query<(Entity, &Owner), With<Unit>>,
    mut fog: ResMut<FogOfWar>,
    q_selected: Query<Entity, With<crate::game::selection::Selected>>,
    mut show_grid: ResMut<crate::game::map::ShowMapGrid>,
    q_window: Query<&Window, With<PrimaryWindow>>,
    q_camera: Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
) {
    if !console_data.is_open {
        // Discard events so they don't buffer while closed
        keyboard_input_events.clear();
        return;
    }

    for event in keyboard_input_events.read() {
        if event.state == ButtonState::Pressed {
            match &event.logical_key {
                Key::Backspace => {
                    console_data.input_buffer.pop();
                }
                Key::Enter => {
                    let cmd = console_data.input_buffer.clone();
                    console_data.input_buffer.clear();

                    if cmd.trim().is_empty() {
                        continue;
                    }

                    if console_data.history.last() != Some(&cmd) {
                        console_data.history.push(cmd.clone());
                    }
                    console_data.history_index = None;

                    console_data.print(&format!("> {}", cmd));
                    let cursor_world_position = cursor_ground_position(&q_window, &q_camera);
                    execute_command(
                        cmd,
                        &mut commands,
                        &mut console_data,
                        &mut players,
                        &local_player,
                        &definitions,
                        &q_units,
                        &mut fog,
                        &q_selected,
                        &mut show_grid,
                        cursor_world_position,
                    );
                }
                Key::ArrowUp => {
                    if !console_data.history.is_empty() {
                        let new_index = if let Some(idx) = console_data.history_index {
                            idx.saturating_sub(1)
                        } else {
                            console_data.history.len().saturating_sub(1)
                        };
                        console_data.history_index = Some(new_index);
                        console_data.input_buffer = console_data.history[new_index].clone();
                    }
                }
                Key::ArrowDown => {
                    if let Some(idx) = console_data.history_index {
                        if idx + 1 < console_data.history.len() {
                            console_data.history_index = Some(idx + 1);
                            console_data.input_buffer = console_data.history[idx + 1].clone();
                        } else {
                            console_data.history_index = None;
                            console_data.input_buffer.clear();
                        }
                    }
                }
                Key::Character(c) => {
                    if c != "`" && c != "~" {
                        console_data.input_buffer.push_str(c.as_str());
                    }
                }
                Key::Space => {
                    console_data.input_buffer.push(' ');
                }
                _ => {}
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn execute_command(
    cmd: String,
    commands: &mut Commands,
    console_data: &mut ConsoleData,
    players: &mut Players,
    local_player: &LocalPlayer,
    definitions: &Definitions,
    q_units: &Query<(Entity, &Owner), With<Unit>>,
    fog: &mut FogOfWar,
    q_selected: &Query<Entity, With<crate::game::selection::Selected>>,
    show_grid: &mut crate::game::map::ShowMapGrid,
    cursor_world_position: Option<Vec3>,
) {
    let parts: Vec<&str> = cmd.trim().split_whitespace().collect();
    if parts.is_empty() {
        return;
    }

    match parts[0].to_lowercase().as_str() {
        "help" => {
            console_data.print("Available commands:");
            console_data.print("  help - Show this message");
            console_data.print("  clear - Clear the console log");
            console_data.print("  ping - Replies with pong");
            console_data.print("  credits <amount> - Add credits to local player");
            console_data.print("  spawn <unit_id> [x,y] [count] - Spawn at cursor or coordinates");
            console_data.print("  kill - Destroy selected units/buildings");
            console_data.print("  killall - Destroy all ENEMY units");
            console_data.print("  fog - Toggle Fog of War");
            console_data.print("  explore - Reveal the entire map as explored");
            console_data.print("  grid - Toggle map grid");
        }
        "clear" => {
            console_data.log.clear();
        }
        "ping" => {
            console_data.print("pong");
        }
        "fog" => {
            fog.is_disabled = !fog.is_disabled;
            console_data.print(&format!("Fog of War disabled: {}", fog.is_disabled));
        }
        "explore" => {
            for state in fog.states.iter_mut() {
                if *state == crate::game::fog_of_war::VisibilityState::Unexplored {
                    *state = crate::game::fog_of_war::VisibilityState::Explored;
                }
            }
            console_data.print("Map fully explored.");
        }
        "grid" => {
            show_grid.0 = !show_grid.0;
            console_data.print(&format!("Map grid visible: {}", show_grid.0));
        }
        "credits" => {
            if parts.len() < 2 {
                console_data.print("Usage: credits <amount>");
                return;
            }
            if let Ok(amount) = parts[1].parse::<i32>() {
                if let Some(player) = players.players.get_mut(&local_player.0) {
                    player.credits = (player.credits as i32 + amount).max(0) as u32;
                    console_data.print(&format!(
                        "Added {} credits. New total: {}",
                        amount, player.credits
                    ));
                } else {
                    console_data.print("Local player not found.");
                }
            } else {
                console_data.print("Invalid amount.");
            }
        }
        "spawn" => {
            if parts.len() < 2 {
                print_spawn_usage(console_data);
                return;
            }
            let unit_id = parts[1].to_string();

            if !definitions.units.contains_key(&unit_id) {
                console_data.print(&format!("Unknown unit id: {}", unit_id));
                return;
            }

            let Ok((coordinates, count)) = parse_spawn_options(&parts[2..]) else {
                print_spawn_usage(console_data);
                return;
            };
            let center = match coordinates {
                Some(position) => position,
                None => {
                    let Some(cursor_position) = cursor_world_position else {
                        console_data.print("Unable to determine cursor world position.");
                        return;
                    };
                    Vec2::new(cursor_position.x, cursor_position.z)
                }
            };

            for position in spawn_formation_positions(center, count) {
                spawn_unit_of_type(
                    commands,
                    unit_id.clone(),
                    definitions,
                    Vec3::new(position.x, 0.5, position.y),
                    Owner(local_player.0),
                );
            }
            console_data.print(&format!(
                "Spawned {} x{} at ({:.1}, {:.1})",
                unit_id, count, center.x, center.y
            ));
        }
        "kill" => {
            let mut count = 0;
            for entity in q_selected.iter() {
                commands.entity(entity).try_despawn();
                count += 1;
            }
            if count > 0 {
                console_data.print(&format!("Destroyed {} selected entity/entities.", count));
            } else {
                console_data.print("No entities currently selected.");
            }
        }
        "killall" => {
            let mut count = 0;
            for (entity, owner) in q_units.iter() {
                if owner.0 != local_player.0 {
                    commands.entity(entity).try_despawn();
                    count += 1;
                }
            }
            console_data.print(&format!("Destroyed {} enemy units.", count));
        }
        _ => {
            console_data.print(&format!("Unknown command: {}", parts[0]));
        }
    }
}

fn print_spawn_usage(console_data: &mut ConsoleData) {
    console_data.print("Usage:");
    console_data.print("  spawn <unit_id>");
    console_data.print("  spawn <unit_id> <count>");
    console_data.print("  spawn <unit_id> <x,y> [count]");
}

fn parse_spawn_options(options: &[&str]) -> Result<(Option<Vec2>, usize), ()> {
    match options {
        [] => Ok((None, 1)),
        [value] if value.contains(',') => Ok((Some(parse_coordinates(value)?), 1)),
        [count] => Ok((None, parse_spawn_count(count)?)),
        [coordinates, count] => Ok((
            Some(parse_coordinates(coordinates)?),
            parse_spawn_count(count)?,
        )),
        _ => Err(()),
    }
}

fn parse_coordinates(value: &str) -> Result<Vec2, ()> {
    let (x, y) = value.split_once(',').ok_or(())?;
    if y.contains(',') {
        return Err(());
    }
    Ok(Vec2::new(
        x.parse().map_err(|_| ())?,
        y.parse().map_err(|_| ())?,
    ))
}

fn parse_spawn_count(value: &str) -> Result<usize, ()> {
    value
        .parse::<usize>()
        .ok()
        .filter(|count| *count > 0)
        .ok_or(())
}

fn spawn_formation_positions(center: Vec2, count: usize) -> Vec<Vec2> {
    const SPACING: f32 = 1.5;

    let columns = (count as f32).sqrt().ceil() as usize;
    let rows = count.div_ceil(columns);
    let mut positions = Vec::with_capacity(count);

    for row in 0..rows {
        let row_start = row * columns;
        let row_len = columns.min(count - row_start);
        for column in 0..row_len {
            let x_offset = (column as f32 - (row_len - 1) as f32 * 0.5) * SPACING;
            let y_offset = (row as f32 - (rows - 1) as f32 * 0.5) * SPACING;
            positions.push(center + Vec2::new(x_offset, y_offset));
        }
    }

    positions
}

fn cursor_ground_position(
    q_window: &Query<&Window, With<PrimaryWindow>>,
    q_camera: &Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
) -> Option<Vec3> {
    let window = q_window.single().ok()?;
    let cursor_position = window.cursor_position()?;
    let (camera, camera_transform) = q_camera.single().ok()?;
    let ray = camera
        .viewport_to_world(camera_transform, cursor_position)
        .ok()?;
    if ray.direction.y.abs() < 0.001 {
        return None;
    }

    let distance = -ray.origin.y / ray.direction.y;
    (distance >= 0.0).then_some(ray.origin + *ray.direction * distance)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_spawn_command_variants() {
        assert_eq!(parse_spawn_options(&[]), Ok((None, 1)));
        assert_eq!(parse_spawn_options(&["4"]), Ok((None, 4)));
        assert_eq!(
            parse_spawn_options(&["12.5,-8"]),
            Ok((Some(Vec2::new(12.5, -8.0)), 1))
        );
        assert_eq!(
            parse_spawn_options(&["12.5,-8", "4"]),
            Ok((Some(Vec2::new(12.5, -8.0)), 4))
        );
    }

    #[test]
    fn rejects_invalid_spawn_options() {
        assert!(parse_spawn_options(&["0"]).is_err());
        assert!(parse_spawn_options(&["12", "8"]).is_err());
        assert!(parse_spawn_options(&["12,8", "many"]).is_err());
    }

    #[test]
    fn multiple_spawns_are_centered_and_separated() {
        let center = Vec2::new(10.0, 20.0);
        let positions = spawn_formation_positions(center, 4);

        assert_eq!(positions.len(), 4);
        let average = positions.iter().copied().sum::<Vec2>() / positions.len() as f32;
        assert_eq!(average, center);
        for (index, position) in positions.iter().enumerate() {
            for other in positions.iter().skip(index + 1) {
                assert!(position.distance(*other) >= 1.5);
            }
        }
    }
}

fn update_console_ui(
    console_data: Res<ConsoleData>,
    mut q_log: Query<&mut Text, (With<ConsoleLogText>, Without<ConsoleInputText>)>,
    mut q_input: Query<&mut Text, (With<ConsoleInputText>, Without<ConsoleLogText>)>,
) {
    if console_data.is_changed() {
        if let Some(mut log_text) = q_log.iter_mut().next() {
            log_text.0 = console_data.log.join("\n");
        }
        if let Some(mut input_text) = q_input.iter_mut().next() {
            input_text.0 = console_data.input_buffer.clone();
            // Optional: simulate blinking cursor
            if (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
                / 500)
                % 2
                == 0
            {
                input_text.0.push('_');
            }
        }
    }
}
