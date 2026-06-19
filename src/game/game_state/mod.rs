use crate::game::buildings::Building;
use crate::game::player::{Player, PlayerController, Players};
use crate::game::units::Owner;
use bevy::prelude::*;
// removed EconomyPlugin import
use crate::game::data::Definitions;
use crate::game::map::MapSelection;
use crate::game::map::map_registry::MapRegistry;
use crate::game::save_load::LoadRequest;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};

pub struct GameStatePlugin;

/// The top-level application state controlling what screen is shown.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    MainMenu,
    FactionSelection,
    InGame,
}

/// Run condition: returns true only while the game is still in progress.
pub fn game_is_playing(result: Res<GameResult>) -> bool {
    *result == GameResult::Playing
}

impl Plugin for GameStatePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppState>()
            .insert_resource(GameResult::Playing)
            .insert_resource(GameCheckTimer(0.0))
            .add_systems(OnEnter(AppState::MainMenu), setup_main_menu)
            .add_systems(OnExit(AppState::MainMenu), cleanup_main_menu)
            .add_systems(
                Update,
                handle_main_menu_buttons.run_if(in_state(AppState::MainMenu)),
            )
            .add_systems(OnEnter(AppState::FactionSelection), setup_faction_selection)
            .add_systems(
                OnExit(AppState::FactionSelection),
                cleanup_faction_selection,
            )
            .add_systems(
                Update,
                (
                    handle_faction_selection_buttons,
                    update_player_slots_ui,
                    update_map_selection_ui,
                    mouse_scroll_list,
                )
                    .run_if(in_state(AppState::FactionSelection)),
            )
            .add_systems(OnEnter(AppState::InGame), setup_menu_camera)
            .add_systems(
                Update,
                (
                    check_game_over_conditions,
                    game_over_ui,
                    handle_restart_button,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

#[derive(Resource, PartialEq, Eq, Clone, Copy, Debug)]
pub enum GameResult {
    Playing,
    Victory,
    Defeat,
}

/// Simple elapsed time counter — start checking after 5 seconds.
#[derive(Resource)]
struct GameCheckTimer(f32);

#[derive(Component)]
struct GameOverOverlay;

#[derive(Component)]
struct RestartButton;

// ─── Main Menu ───────────────────────────────────────────────────

#[derive(Component)]
struct MainMenuRoot;

#[derive(Component)]
struct MainMenuCamera;

#[derive(Component)]
enum MainMenuButton {
    NewGame,
    Load,
    Exit,
}

fn setup_menu_camera(_commands: Commands, _q_camera: Query<Entity, With<Camera3d>>) {
    // Placeholder — the real camera is spawned by CameraPlugin's OnEnter(InGame).
}

fn setup_main_menu(mut commands: Commands) {
    // Spawn a UI-only camera for the menu (the game camera doesn't exist yet)
    commands.spawn((Camera2d, MainMenuCamera));

    // Full-screen menu root
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(20.0),
                ..default()
            },
            BackgroundColor(Color::srgb(0.08, 0.08, 0.12)),
            GlobalZIndex(200),
            MainMenuRoot,
        ))
        .with_children(|parent| {
            // Title
            parent.spawn((
                Text::new("CRIMSON ALERT"),
                TextFont {
                    font_size: 72.0,
                    ..default()
                },
                TextColor(Color::srgb(0.9, 0.15, 0.15)),
            ));

            // Subtitle
            parent.spawn((
                Text::new("Command & Conquer"),
                TextFont {
                    font_size: 20.0,
                    ..default()
                },
                TextColor(Color::srgba(0.6, 0.6, 0.6, 0.8)),
            ));

            // Spacer
            parent.spawn(Node {
                height: Val::Px(40.0),
                ..default()
            });

            // Menu Buttons
            let buttons = [
                ("SKIRMISH", MainMenuButton::NewGame),
                ("LOAD", MainMenuButton::Load),
                ("EXIT", MainMenuButton::Exit),
            ];

            for (label, btn_type) in buttons {
                parent
                    .spawn((
                        Button,
                        Node {
                            width: Val::Px(280.0),
                            height: Val::Px(55.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.18, 0.18, 0.22)),
                        BorderColor::all(Color::srgb(0.35, 0.35, 0.4)),
                        btn_type,
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new(label),
                            TextFont {
                                font_size: 22.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.85, 0.85, 0.85)),
                        ));
                    });
            }
        });
}

fn cleanup_main_menu(
    mut commands: Commands,
    q_menu: Query<Entity, With<MainMenuRoot>>,
    q_camera: Query<Entity, With<MainMenuCamera>>,
) {
    for entity in q_menu.iter() {
        commands.entity(entity).try_despawn();
    }
    for entity in q_camera.iter() {
        commands.entity(entity).try_despawn();
    }
}

fn handle_main_menu_buttons(
    interaction_query: Query<(&Interaction, &MainMenuButton), (Changed<Interaction>, With<Button>)>,
    mut next_state: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
    mut load_events: MessageWriter<LoadRequest>,
    mut q_btn_colors: Query<
        (&Interaction, &mut BackgroundColor),
        (With<MainMenuButton>, With<Button>),
    >,
) {
    // Hover effects
    for (interaction, mut color) in &mut q_btn_colors {
        match *interaction {
            Interaction::Hovered => {
                color.0 = Color::srgb(0.28, 0.22, 0.22);
            }
            Interaction::None => {
                color.0 = Color::srgb(0.18, 0.18, 0.22);
            }
            _ => {}
        }
    }

    for (interaction, btn) in &interaction_query {
        if *interaction != Interaction::Pressed {
            continue;
        }

        match btn {
            MainMenuButton::NewGame => {
                next_state.set(AppState::FactionSelection);
            }
            MainMenuButton::Load => {
                next_state.set(AppState::InGame);
                load_events.write(LoadRequest);
            }
            MainMenuButton::Exit => {
                exit.write(AppExit::Success);
            }
        }
    }
}

// ─── Faction Selection Menu ──────────────────────────────────────

#[derive(Component)]
struct FactionSelectionRoot;

#[derive(Component)]
pub enum FactionSelectionButton {
    MapSelection(String),
    ToggleController(usize),
    ToggleFaction(usize),
    StartGame,
    Back,
}

#[derive(Component)]
pub struct PlayerSlotsContainer;

#[derive(Component, Default)]
pub struct ScrollingList {
    pub position: f32,
}

const MAP_LIST_HEIGHT: f32 = 220.0;
const MAP_LIST_WIDTH: f32 = 520.0;
const MAP_ROW_HEIGHT: f32 = 64.0;
const PLAYER_SLOT_ROW_HEIGHT: f32 = 40.0;
const PLAYER_SLOT_ROW_GAP: f32 = 10.0;

fn player_slots_height(slot_count: usize) -> f32 {
    let row_count = slot_count.max(1) as f32;
    (row_count * PLAYER_SLOT_ROW_HEIGHT) + ((row_count - 1.0) * PLAYER_SLOT_ROW_GAP)
}

pub fn mouse_scroll_list(
    mut mouse_wheel_events: MessageReader<MouseWheel>,
    mut query_list: Query<(&mut ScrollingList, &mut Node, &ComputedNode)>,
) {
    for mouse_wheel_event in mouse_wheel_events.read() {
        let dy = match mouse_wheel_event.unit {
            MouseScrollUnit::Line => mouse_wheel_event.y * 20.,
            MouseScrollUnit::Pixel => mouse_wheel_event.y,
        };
        for (mut scrolling_list, mut node, list_node) in &mut query_list {
            let items_height = list_node.size().y;
            let container_height = MAP_LIST_HEIGHT;
            let max_scroll = (items_height - container_height).max(0.);
            scrolling_list.position += dy;
            scrolling_list.position = scrolling_list.position.clamp(-max_scroll, 0.);
            node.top = Val::Px(scrolling_list.position);
        }
    }
}

fn setup_faction_selection(
    mut commands: Commands,
    definitions: Res<Definitions>,
    map_registry: Res<MapRegistry>,
    mut players: ResMut<Players>,
    mut map_selection: ResMut<MapSelection>,
) {
    if players.players.is_empty() {
        if let Some(first_faction) = definitions.factions.keys().next() {
            players.players.insert(
                0,
                Player {
                    id: 0,
                    team_id: 0,
                    faction: first_faction.clone(),
                    controller: PlayerController::LocalHuman,
                    color: Color::srgb(0.0, 0.0, 1.0),
                    credits: 8000,
                },
            );
            players.players.insert(
                1,
                Player {
                    id: 1,
                    team_id: 1,
                    faction: first_faction.clone(),
                    controller: PlayerController::AI,
                    color: Color::srgb(1.0, 0.0, 0.0),
                    credits: 8000,
                },
            );
        }
    }
    if map_selection.0.is_empty() {
        if let Some(first_map) = map_registry.maps.first() {
            map_selection.0 = first_map.file_name.clone();
        }
    }

    // Ensure players match the selected map
    let mut recommended_players = 2;
    if let Some(map_info) = map_registry
        .maps
        .iter()
        .find(|m| m.file_name == map_selection.0)
    {
        recommended_players = map_info.recommended_players;
    }
    let first_faction = definitions
        .factions
        .keys()
        .next()
        .cloned()
        .unwrap_or_else(|| "soviet".to_string());
    let colors = [
        Color::srgb(0.0, 0.0, 1.0),
        Color::srgb(1.0, 0.0, 0.0),
        Color::srgb(0.0, 1.0, 0.0),
        Color::srgb(1.0, 1.0, 0.0),
        Color::srgb(1.0, 0.0, 1.0),
    ];
    for i in 0..recommended_players {
        if !players.players.contains_key(&i) {
            let color = colors.get(i).copied().unwrap_or(Color::WHITE);
            players.players.insert(
                i,
                Player {
                    id: i,
                    team_id: i,
                    faction: first_faction.clone(),
                    controller: PlayerController::AI,
                    color,
                    credits: 8000,
                },
            );
        }
    }
    players.players.retain(|&id, _| id < recommended_players);
    let max_player_slots = map_registry
        .maps
        .iter()
        .map(|map_info| map_info.recommended_players)
        .max()
        .unwrap_or(recommended_players);

    commands.spawn((Camera2d, MainMenuCamera, FactionSelectionRoot));

    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            FactionSelectionRoot,
        ))
        .with_children(|parent| {
            // Title
            parent.spawn((
                Text::new("Skirmish Setup"),
                TextFont {
                    font_size: 40.0,
                    ..default()
                },
                TextColor(Color::srgb(0.9, 0.9, 0.9)),
            ));

            parent.spawn(Node {
                height: Val::Px(30.0),
                ..default()
            });

            // Map Selection Row
            parent.spawn((
                Text::new("Map:"),
                TextFont {
                    font_size: 24.0,
                    ..default()
                },
                TextColor(Color::srgb(0.8, 0.8, 0.8)),
            ));

            parent
                .spawn(Node {
                    width: Val::Px(MAP_LIST_WIDTH),
                    height: Val::Px(MAP_LIST_HEIGHT),
                    flex_direction: FlexDirection::Column,
                    overflow: Overflow::clip_y(),
                    margin: UiRect::bottom(Val::Px(20.0)),
                    ..default()
                })
                .with_children(|scroll_container| {
                    scroll_container
                        .spawn((
                            Node {
                                width: Val::Percent(100.0),
                                flex_direction: FlexDirection::Column,
                                row_gap: Val::Px(6.0),
                                ..default()
                            },
                            ScrollingList::default(),
                        ))
                        .with_children(|list| {
                            for map_info in &map_registry.maps {
                                let is_selected = map_info.file_name == map_selection.0;
                                list.spawn((
                                    Button,
                                    Node {
                                        width: Val::Percent(100.0),
                                        height: Val::Px(MAP_ROW_HEIGHT),
                                        padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                                        justify_content: JustifyContent::SpaceBetween,
                                        align_items: AlignItems::Center,
                                        ..default()
                                    },
                                    BackgroundColor(if is_selected {
                                        Color::srgb(0.12, 0.42, 0.16)
                                    } else {
                                        Color::srgb(0.18, 0.18, 0.22)
                                    }),
                                    BorderColor::all(Color::srgb(0.35, 0.35, 0.4)),
                                    FactionSelectionButton::MapSelection(
                                        map_info.file_name.clone(),
                                    ),
                                ))
                                .with_children(|btn| {
                                    btn.spawn(Node {
                                        width: Val::Px(360.0),
                                        flex_direction: FlexDirection::Column,
                                        justify_content: JustifyContent::Center,
                                        row_gap: Val::Px(3.0),
                                        ..default()
                                    })
                                    .with_children(
                                        |details| {
                                            details.spawn((
                                                Text::new(map_info.name.clone()),
                                                TextFont {
                                                    font_size: 20.0,
                                                    ..default()
                                                },
                                                TextColor(Color::srgb(0.9, 0.9, 0.9)),
                                            ));
                                            details.spawn((
                                                Text::new(map_info.description.clone()),
                                                TextFont {
                                                    font_size: 13.0,
                                                    ..default()
                                                },
                                                TextColor(Color::srgb(0.68, 0.68, 0.72)),
                                            ));
                                        },
                                    );

                                    btn.spawn((
                                        Text::new(format!("{}P", map_info.recommended_players)),
                                        TextFont {
                                            font_size: 18.0,
                                            ..default()
                                        },
                                        TextColor(Color::srgb(0.85, 0.85, 0.85)),
                                        Node {
                                            width: Val::Px(56.0),
                                            ..default()
                                        },
                                    ));
                                });
                            }
                        });
                });

            parent.spawn(Node {
                height: Val::Px(20.0),
                ..default()
            });

            // Player Slots Container
            parent.spawn((
                Node {
                    height: Val::Px(player_slots_height(max_player_slots)),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::FlexStart,
                    row_gap: Val::Px(10.0),
                    ..default()
                },
                PlayerSlotsContainer,
            ));

            parent.spawn(Node {
                height: Val::Px(40.0),
                ..default()
            });

            // Start Game & Back
            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(20.0),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        Button,
                        Node {
                            width: Val::Px(200.0),
                            height: Val::Px(60.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.2, 0.6, 0.2)),
                        BorderColor::all(Color::srgb(0.3, 0.7, 0.3)),
                        FactionSelectionButton::StartGame,
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new("Start Game"),
                            TextFont {
                                font_size: 28.0,
                                ..default()
                            },
                            TextColor(Color::WHITE),
                        ));
                    });

                    row.spawn((
                        Button,
                        Node {
                            width: Val::Px(200.0),
                            height: Val::Px(60.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.6, 0.2, 0.2)),
                        BorderColor::all(Color::srgb(0.7, 0.3, 0.3)),
                        FactionSelectionButton::Back,
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new("Back"),
                            TextFont {
                                font_size: 28.0,
                                ..default()
                            },
                            TextColor(Color::WHITE),
                        ));
                    });
                });
        });
}

fn cleanup_faction_selection(
    mut commands: Commands,
    q_menu: Query<Entity, With<FactionSelectionRoot>>,
    q_camera: Query<Entity, With<MainMenuCamera>>,
) {
    for entity in q_menu.iter() {
        commands.entity(entity).try_despawn();
    }
    for entity in q_camera.iter() {
        commands.entity(entity).try_despawn();
    }
}

fn handle_faction_selection_buttons(
    interaction_query: Query<
        (&Interaction, &FactionSelectionButton),
        (Changed<Interaction>, With<Button>),
    >,
    mut next_state: ResMut<NextState<AppState>>,
    mut players: ResMut<Players>,
    mut map_selection: ResMut<MapSelection>,
    definitions: Res<Definitions>,
    mut local_player: ResMut<crate::game::player::LocalPlayer>,
) {
    for (interaction, btn) in &interaction_query {
        if *interaction != Interaction::Pressed {
            continue;
        }

        match btn {
            FactionSelectionButton::ToggleController(id) => {
                let current = players.players.get(id).map(|p| p.controller.clone());
                if let Some(ctrl) = current {
                    let next = match ctrl {
                        PlayerController::LocalHuman => PlayerController::AI,
                        PlayerController::AI => PlayerController::None,
                        PlayerController::None => PlayerController::LocalHuman,
                        _ => PlayerController::AI,
                    };

                    if matches!(next, PlayerController::LocalHuman) {
                        for (pid, p) in players.players.iter_mut() {
                            if pid != id && matches!(p.controller, PlayerController::LocalHuman) {
                                p.controller = PlayerController::AI;
                            }
                        }
                    }

                    if let Some(player) = players.players.get_mut(id) {
                        player.controller = next;
                    }
                }
            }
            FactionSelectionButton::ToggleFaction(id) => {
                if let Some(player) = players.players.get_mut(id) {
                    let mut factions: Vec<_> = definitions.factions.keys().cloned().collect();
                    factions.sort(); // Ensure deterministic order
                    if let Some(pos) = factions.iter().position(|x| x == &player.faction) {
                        let next_pos = (pos + 1) % factions.len();
                        player.faction = factions[next_pos].clone();
                    } else if let Some(first) = factions.first() {
                        player.faction = first.clone();
                    }
                }
            }
            FactionSelectionButton::MapSelection(name) => {
                map_selection.0 = name.clone();
            }
            FactionSelectionButton::StartGame => {
                if let Some(human) = players.players.values().find(|p| {
                    matches!(
                        p.controller,
                        crate::game::player::PlayerController::LocalHuman
                    )
                }) {
                    local_player.0 = human.id;
                }
                next_state.set(AppState::InGame);
            }
            FactionSelectionButton::Back => {
                next_state.set(AppState::MainMenu);
            }
        }
    }
}

fn update_player_slots_ui(
    mut commands: Commands,
    map_selection: Res<MapSelection>,
    map_registry: Res<MapRegistry>,
    mut players: ResMut<Players>,
    definitions: Res<Definitions>,
    q_container: Query<(Entity, Option<&Children>), With<PlayerSlotsContainer>>,
) {
    if !map_selection.is_changed() && !players.is_changed() {
        return;
    }

    let Some((container_entity, children)) = q_container.iter().next() else {
        return;
    };

    let mut recommended_players = 2;
    if let Some(map_info) = map_registry
        .maps
        .iter()
        .find(|m| m.file_name == map_selection.0)
    {
        recommended_players = map_info.recommended_players;
    }

    let mut needs_mutation = false;
    for i in 0..recommended_players {
        if !players.players.contains_key(&i) {
            needs_mutation = true;
            break;
        }
    }
    if players.players.len() > recommended_players {
        needs_mutation = true;
    }
    if !players.players.is_empty()
        && !players
            .players
            .values()
            .any(|p| matches!(p.controller, PlayerController::LocalHuman))
    {
        needs_mutation = true;
    }

    if needs_mutation {
        let first_faction = definitions
            .factions
            .keys()
            .next()
            .cloned()
            .unwrap_or_else(|| "soviet".to_string());
        let colors = [
            Color::srgb(0.0, 0.0, 1.0),
            Color::srgb(1.0, 0.0, 0.0),
            Color::srgb(0.0, 1.0, 0.0),
            Color::srgb(1.0, 1.0, 0.0),
            Color::srgb(1.0, 0.0, 1.0),
        ];
        for i in 0..recommended_players {
            if !players.players.contains_key(&i) {
                let color = colors.get(i).copied().unwrap_or(Color::WHITE);
                players.players.insert(
                    i,
                    Player {
                        id: i,
                        team_id: i,
                        faction: first_faction.clone(),
                        controller: PlayerController::AI,
                        color,
                        credits: 8000,
                    },
                );
            }
        }
        players.players.retain(|&id, _| id < recommended_players);

        // Ensure there is exactly one LocalHuman
        if !players.players.is_empty()
            && !players
                .players
                .values()
                .any(|p| matches!(p.controller, PlayerController::LocalHuman))
        {
            if let Some(first) = players.players.get_mut(&0) {
                first.controller = PlayerController::LocalHuman;
            }
        }
    }

    if let Some(children) = children {
        for child in children.iter() {
            commands.entity(child).try_despawn();
        }
    }

    commands.entity(container_entity).with_children(|parent| {
        for i in 0..recommended_players {
            let player = players.players.get(&i).unwrap();
            let controller_text = match player.controller {
                PlayerController::LocalHuman => "Human",
                PlayerController::AI => "AI",
                PlayerController::None => "None",
                _ => "Unknown",
            };
            let faction_text = definitions
                .factions
                .get(&player.faction)
                .map_or("Unknown", |f| &f.name);

            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(15.0),
                    align_items: AlignItems::Center,
                    height: Val::Px(PLAYER_SLOT_ROW_HEIGHT),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        Text::new(format!("Player {}:", i + 1)),
                        TextFont {
                            font_size: 20.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.8, 0.8, 0.8)),
                        Node {
                            width: Val::Px(100.0),
                            ..default()
                        },
                    ));

                    row.spawn((
                        Button,
                        Node {
                            width: Val::Px(120.0),
                            height: Val::Px(40.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.18, 0.18, 0.22)),
                        FactionSelectionButton::ToggleController(i),
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new(controller_text.to_string()),
                            TextFont {
                                font_size: 18.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.85, 0.85, 0.85)),
                        ));
                    });

                    row.spawn((
                        Button,
                        Node {
                            width: Val::Px(150.0),
                            height: Val::Px(40.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.18, 0.18, 0.22)),
                        FactionSelectionButton::ToggleFaction(i),
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new(faction_text.to_string()),
                            TextFont {
                                font_size: 18.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.85, 0.85, 0.85)),
                        ));
                    });
                });
        }
    });
}

fn update_map_selection_ui(
    map_selection: Res<MapSelection>,
    mut q_buttons: Query<(&FactionSelectionButton, &mut BackgroundColor)>,
) {
    if !map_selection.is_changed() {
        return;
    }

    for (btn, mut color) in &mut q_buttons {
        if let FactionSelectionButton::MapSelection(name) = btn {
            if *name == map_selection.0 {
                color.0 = Color::srgb(0.12, 0.42, 0.16);
            } else {
                color.0 = Color::srgb(0.18, 0.18, 0.22);
            }
        }
    }
}

// ─── In-Game: Win/Lose Checks ────────────────────────────────────

fn check_game_over_conditions(
    q_buildings: Query<(&Building, &Owner)>,
    mut game_result: ResMut<GameResult>,
    mut timer: ResMut<GameCheckTimer>,
    time: Res<Time>,
    local_player: Res<crate::game::player::LocalPlayer>,
) {
    if *game_result != GameResult::Playing {
        return;
    }

    timer.0 += time.delta_secs();
    if timer.0 < 5.0 {
        return;
    }

    let mut player_has_buildings = false;
    let mut enemy_has_buildings = false;

    for (_building, team) in q_buildings.iter() {
        if team.0 == local_player.0 {
            player_has_buildings = true;
        } else {
            enemy_has_buildings = true;
        }
        if player_has_buildings && enemy_has_buildings {
            return; // Both sides alive, game continues
        }
    }

    // Determine result
    let result = if !player_has_buildings {
        GameResult::Defeat
    } else if !enemy_has_buildings {
        GameResult::Victory
    } else {
        return;
    };

    *game_result = result;
}

fn game_over_ui(
    mut commands: Commands,
    game_result: Res<GameResult>,
    q_overlay: Query<Entity, With<GameOverOverlay>>,
) {
    if !game_result.is_changed() || *game_result == GameResult::Playing {
        return;
    }

    let result = *game_result;

    // Spawn overlay UI
    let (title, title_color, subtitle, bg_color) = match result {
        GameResult::Victory => (
            "MISSION ACCOMPLISHED",
            Color::srgb(0.2, 1.0, 0.3),
            "All enemy structures have been eliminated.",
            Color::srgba(0.0, 0.05, 0.0, 0.85),
        ),
        GameResult::Defeat => (
            "MISSION FAILED",
            Color::srgb(1.0, 0.2, 0.2),
            "All your structures have been destroyed.",
            Color::srgba(0.1, 0.0, 0.0, 0.85),
        ),
        _ => return,
    };

    if !q_overlay.is_empty() {
        return;
    }

    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(20.0),
                ..default()
            },
            BackgroundColor(bg_color),
            GlobalZIndex(100),
            GameOverOverlay,
        ))
        .with_children(|parent| {
            // Title
            parent.spawn((
                Text::new(title),
                TextFont {
                    font_size: 64.0,
                    ..default()
                },
                TextColor(title_color),
            ));

            // Subtitle
            parent.spawn((
                Text::new(subtitle),
                TextFont {
                    font_size: 24.0,
                    ..default()
                },
                TextColor(Color::srgba(0.8, 0.8, 0.8, 0.9)),
            ));

            // Spacer
            parent.spawn(Node {
                height: Val::Px(30.0),
                ..default()
            });

            // Exit button
            parent
                .spawn((
                    Button,
                    Node {
                        width: Val::Px(200.0),
                        height: Val::Px(50.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.25, 0.25, 0.3)),
                    RestartButton,
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new("EXIT GAME"),
                        TextFont {
                            font_size: 20.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });
        });
}

fn handle_restart_button(
    interaction_query: Query<&Interaction, (Changed<Interaction>, With<RestartButton>)>,
    mut exit: MessageWriter<AppExit>,
) {
    for interaction in &interaction_query {
        if *interaction == Interaction::Pressed {
            exit.write(AppExit::Success);
        }
    }
}
