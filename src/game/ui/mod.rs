use crate::game::buildings::{Building, PowerSystem, ProductionQueue, TeamBuildingQueues};
use crate::game::commands::{
    BuildCommand, CancelBuildCommand, CancelUnitCommand, TrainUnitCommand,
};
use crate::game::data::Definitions;
use crate::game::game_state::AppState;
use crate::game::player::{LocalPlayer, Players};
use crate::game::save_load::{LoadRequest, SaveRequest};
use crate::game::ui::placement::{PlacementPlugin, PlacementState};
use bevy::prelude::*;

pub mod console;
pub mod debug;
pub mod minimap;
pub mod placement;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(PlacementPlugin)
            .add_plugins(minimap::MinimapPlugin)
            .add_plugins(debug::DebugUiPlugin)
            .add_plugins(console::ConsolePlugin)
            .insert_resource(ActiveTab::Buildings)
            .init_resource::<CursorMode>()
            .init_resource::<SidebarVisible>()
            .add_systems(OnEnter(AppState::InGame), setup_ui)
            .add_systems(
                Update,
                (
                    update_credits_ui,
                    update_power_ui,
                    handle_sidebar_toggle,
                    handle_build_buttons,
                    handle_tab_clicks,
                    handle_produce_buttons,
                    update_production_ui,
                    update_building_ui,
                    handle_save_load_buttons,
                    handle_cursor_mode_buttons,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

#[derive(Resource, PartialEq, Eq, Clone, Copy, Default)]
pub enum CursorMode {
    #[default]
    Normal,
    Sell,
    Repair,
}

#[derive(Resource, PartialEq, Eq, Clone, Copy)]
pub enum ActiveTab {
    Buildings,
    Units,
}

#[derive(Resource, PartialEq, Eq, Clone, Copy)]
pub struct SidebarVisible(pub bool);

impl Default for SidebarVisible {
    fn default() -> Self {
        Self(true)
    }
}

#[derive(Component)]
pub struct CreditsText;

#[derive(Component)]
pub struct PowerText;

#[derive(Component)]
pub struct BuildButton(pub String);

#[derive(Component)]
pub struct TabButton(pub ActiveTab);

#[derive(Component)]
pub struct BuildingsTabGrid;

#[derive(Component)]
pub struct UnitsTabGrid;

#[derive(Component)]
pub struct ProduceButton(pub String);

#[derive(Component)]
pub struct ProduceButtonText(pub String);

#[derive(Component)]
pub struct BuildButtonText(pub String);

#[derive(Component)]
pub struct SaveButton;

#[derive(Component)]
pub struct LoadButton;

#[derive(Component)]
pub struct SellButton;

#[derive(Component)]
pub struct RepairButton;

#[derive(Component)]
pub struct GameSidebar;

#[derive(Component)]
pub struct SidebarToggleButton;

#[derive(Component)]
pub struct SidebarToggleText;

fn setup_ui(
    mut commands: Commands,
    definitions: Res<Definitions>,
    players: Res<Players>,
    local_player: Res<LocalPlayer>,
) {
    commands
        .spawn((Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::SpaceBetween,
            ..default()
        },))
        .with_children(|parent| {
            // Left spacer
            parent.spawn((Node {
                width: Val::Auto,
                height: Val::Percent(100.0),
                flex_grow: 1.0,
                ..default()
            },));

            // Right Sidebar
            parent
                .spawn((
                    Node {
                        width: Val::Px(200.0),
                        height: Val::Percent(100.0),
                        flex_direction: FlexDirection::Column,
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.15, 0.15, 0.15)),
                    GameSidebar,
                ))
                .with_children(|sidebar| {
                    minimap::spawn_minimap(sidebar);

                    // Cursor Mode (Repair / Sell) Panel
                    sidebar
                        .spawn((
                            Node {
                                width: Val::Percent(100.0),
                                height: Val::Px(35.0),
                                flex_direction: FlexDirection::Row,
                                justify_content: JustifyContent::SpaceEvenly,
                                align_items: AlignItems::Center,
                                padding: UiRect::all(Val::Px(5.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgb(0.12, 0.12, 0.12)),
                        ))
                        .with_children(|menu| {
                            menu.spawn((
                                Button,
                                Node {
                                    width: Val::Percent(45.0),
                                    height: Val::Percent(100.0),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    ..default()
                                },
                                BackgroundColor(Color::srgb(0.25, 0.25, 0.25)),
                                RepairButton,
                            ))
                            .with_children(|btn| {
                                btn.spawn((
                                    Text::new("Repair"),
                                    TextFont {
                                        font_size: 14.0,
                                        ..default()
                                    },
                                ));
                            });
                            menu.spawn((
                                Button,
                                Node {
                                    width: Val::Percent(45.0),
                                    height: Val::Percent(100.0),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    ..default()
                                },
                                BackgroundColor(Color::srgb(0.25, 0.25, 0.25)),
                                SellButton,
                            ))
                            .with_children(|btn| {
                                btn.spawn((
                                    Text::new("Sell"),
                                    TextFont {
                                        font_size: 14.0,
                                        ..default()
                                    },
                                ));
                            });
                        });

                    // Status Panel
                    sidebar
                        .spawn((
                            Node {
                                width: Val::Percent(100.0),
                                padding: UiRect::all(Val::Px(10.0)),
                                flex_direction: FlexDirection::Column,
                                ..default()
                            },
                            BackgroundColor(Color::srgb(0.1, 0.1, 0.1)),
                        ))
                        .with_children(|status| {
                            status.spawn((
                                Text::new("Credits: 0"),
                                TextFont {
                                    font_size: 16.0,
                                    ..default()
                                },
                                TextColor(Color::srgb(0.0, 1.0, 0.0)),
                                CreditsText,
                            ));
                            status.spawn((
                                Text::new("Power: 0  Load: 0"),
                                TextFont {
                                    font_size: 16.0,
                                    ..default()
                                },
                                TextColor(Color::srgb(0.0, 1.0, 1.0)),
                                PowerText,
                            ));
                        });

                    // Save/Load Panel
                    sidebar
                        .spawn((
                            Node {
                                width: Val::Percent(100.0),
                                height: Val::Px(35.0),
                                flex_direction: FlexDirection::Row,
                                justify_content: JustifyContent::SpaceEvenly,
                                align_items: AlignItems::Center,
                                padding: UiRect::all(Val::Px(5.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgb(0.12, 0.12, 0.12)),
                        ))
                        .with_children(|menu| {
                            menu.spawn((
                                Button,
                                Node {
                                    width: Val::Percent(45.0),
                                    height: Val::Percent(100.0),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    ..default()
                                },
                                BackgroundColor(Color::srgb(0.25, 0.25, 0.25)),
                                SaveButton,
                            ))
                            .with_children(|btn| {
                                btn.spawn((
                                    Text::new("Save"),
                                    TextFont {
                                        font_size: 14.0,
                                        ..default()
                                    },
                                ));
                            });
                            menu.spawn((
                                Button,
                                Node {
                                    width: Val::Percent(45.0),
                                    height: Val::Percent(100.0),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    ..default()
                                },
                                BackgroundColor(Color::srgb(0.25, 0.25, 0.25)),
                                LoadButton,
                            ))
                            .with_children(|btn| {
                                btn.spawn((
                                    Text::new("Load"),
                                    TextFont {
                                        font_size: 14.0,
                                        ..default()
                                    },
                                ));
                            });
                        });

                    // Tabs
                    sidebar
                        .spawn((Node {
                            width: Val::Percent(100.0),
                            height: Val::Px(30.0),
                            flex_direction: FlexDirection::Row,
                            ..default()
                        },))
                        .with_children(|tabs| {
                            tabs.spawn((
                                Button,
                                Node {
                                    flex_grow: 1.0,
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    ..default()
                                },
                                BackgroundColor(Color::srgb(0.3, 0.3, 0.3)),
                                TabButton(ActiveTab::Buildings),
                            ))
                            .with_children(|btn| {
                                btn.spawn((
                                    Text::new("Buildings"),
                                    TextFont {
                                        font_size: 14.0,
                                        ..default()
                                    },
                                ));
                            });
                            tabs.spawn((
                                Button,
                                Node {
                                    flex_grow: 1.0,
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    ..default()
                                },
                                BackgroundColor(Color::srgb(0.2, 0.2, 0.2)),
                                TabButton(ActiveTab::Units),
                            ))
                            .with_children(|btn| {
                                btn.spawn((
                                    Text::new("Units"),
                                    TextFont {
                                        font_size: 14.0,
                                        ..default()
                                    },
                                    TextColor(Color::srgb(0.5, 0.5, 0.5)),
                                ));
                            });
                        });

                    // Build Grid (Buildings)
                    sidebar
                        .spawn((
                            Node {
                                width: Val::Percent(100.0),
                                flex_grow: 1.0,
                                display: Display::Grid,
                                grid_template_columns: vec![GridTrack::fr(1.0), GridTrack::fr(1.0)],
                                padding: UiRect::all(Val::Px(10.0)),
                                row_gap: Val::Px(10.0),
                                column_gap: Val::Px(10.0),
                                align_content: AlignContent::Start,
                                overflow: Overflow::scroll_y(),
                                ..default()
                            },
                            BuildingsTabGrid,
                        ))
                        .with_children(|grid| {
                            let player_faction_id = players
                                .players
                                .get(&local_player.0)
                                .map(|p| p.faction.clone())
                                .unwrap_or_else(|| "alliance".to_string());
                            let mut buildings = Vec::new();
                            if let Some(faction) = definitions.factions.get(&player_faction_id) {
                                for b_id in &faction.buildings {
                                    if let Some(b_def) = definitions.buildings.get(b_id) {
                                        buildings.push(b_def);
                                    }
                                }
                            }
                            buildings.sort_by_key(|b| b.cost);

                            for b in buildings {
                                grid.spawn((
                                    Button,
                                    Node {
                                        aspect_ratio: Some(1.33),
                                        justify_content: JustifyContent::Center,
                                        align_items: AlignItems::Center,
                                        ..default()
                                    },
                                    BackgroundColor(Color::srgb(
                                        b.color[0], b.color[1], b.color[2],
                                    )),
                                    BuildButton(b.id.clone()),
                                ))
                                .with_children(|btn| {
                                    btn.spawn((
                                        Text::new(b.name.clone()),
                                        TextFont {
                                            font_size: 12.0,
                                            ..default()
                                        },
                                        TextColor(Color::WHITE),
                                        BuildButtonText(b.id.clone()),
                                    ));
                                });
                            }
                        });

                    // Units Grid
                    sidebar
                        .spawn((
                            Node {
                                width: Val::Percent(100.0),
                                flex_grow: 1.0,
                                display: Display::None,
                                grid_template_columns: vec![GridTrack::fr(1.0), GridTrack::fr(1.0)],
                                padding: UiRect::all(Val::Px(10.0)),
                                row_gap: Val::Px(10.0),
                                column_gap: Val::Px(10.0),
                                align_content: AlignContent::Start,
                                overflow: Overflow::scroll_y(),
                                ..default()
                            },
                            UnitsTabGrid,
                        ))
                        .with_children(|grid| {
                            let player_faction_id = players
                                .players
                                .get(&local_player.0)
                                .map(|p| p.faction.clone())
                                .unwrap_or_else(|| "alliance".to_string());
                            let mut units = Vec::new();
                            if let Some(faction) = definitions.factions.get(&player_faction_id) {
                                for u_id in &faction.units {
                                    if let Some(u_def) = definitions.units.get(u_id) {
                                        units.push(u_def);
                                    }
                                }
                            }
                            units.sort_by_key(|u| u.cost);

                            for u in units {
                                grid.spawn((
                                    Button,
                                    Node {
                                        aspect_ratio: Some(1.33),
                                        justify_content: JustifyContent::Center,
                                        align_items: AlignItems::Center,
                                        ..default()
                                    },
                                    BackgroundColor(Color::srgb(
                                        u.color[0], u.color[1], u.color[2],
                                    )),
                                    ProduceButton(u.id.clone()),
                                ))
                                .with_children(|btn| {
                                    btn.spawn((
                                        Text::new(u.name.clone()),
                                        TextFont {
                                            font_size: 12.0,
                                            ..default()
                                        },
                                        TextColor(Color::WHITE),
                                        ProduceButtonText(u.id.clone()),
                                    ));
                                });
                            }
                        });
                });
        });

    commands
        .spawn((
            Button,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(10.0),
                right: Val::Px(210.0),
                width: Val::Px(28.0),
                height: Val::Px(32.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            ZIndex(50),
            BackgroundColor(Color::srgb(0.22, 0.22, 0.22)),
            SidebarToggleButton,
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(">"),
                TextFont {
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                SidebarToggleText,
            ));
        });
}

fn update_credits_ui(
    players: Res<Players>,
    local_player: Res<LocalPlayer>,
    mut query: Query<&mut Text, With<CreditsText>>,
) {
    if players.is_changed() {
        for mut text in query.iter_mut() {
            text.0 = format!(
                "Credits: {}",
                players
                    .players
                    .get(&local_player.0)
                    .map(|p| p.credits)
                    .unwrap_or(0)
            );
        }
    }
}

fn update_power_ui(
    power: Res<PowerSystem>,
    mut query: Query<(&mut Text, &mut TextColor), With<PowerText>>,
) {
    if power.is_changed() {
        for (mut text, mut color) in query.iter_mut() {
            text.0 = format!("Power: {}  Load: {}", power.produced, power.consumed);
            if power.consumed > power.produced {
                color.0 = Color::srgb(1.0, 0.0, 0.0);
            } else {
                color.0 = Color::srgb(0.0, 1.0, 1.0);
            }
        }
    }
}

fn handle_sidebar_toggle(
    mut sidebar_visible: ResMut<SidebarVisible>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut toggle_queries: ParamSet<(
        Query<
            (&Interaction, &mut Node, &mut BackgroundColor),
            (Changed<Interaction>, With<SidebarToggleButton>),
        >,
        Query<&mut Node, (With<SidebarToggleButton>, Without<GameSidebar>)>,
    )>,
    mut q_sidebar: Query<&mut Node, (With<GameSidebar>, Without<SidebarToggleButton>)>,
    mut q_toggle_text: Query<&mut Text, With<SidebarToggleText>>,
) {
    let mut should_toggle = keyboard.just_pressed(KeyCode::F9);

    for (interaction, mut node, mut color) in &mut toggle_queries.p0() {
        match *interaction {
            Interaction::Pressed => {
                should_toggle = true;
            }
            Interaction::Hovered => {
                color.0 = Color::srgb(0.32, 0.32, 0.32);
            }
            Interaction::None => {
                color.0 = Color::srgb(0.22, 0.22, 0.22);
            }
        }

        node.right = if sidebar_visible.0 {
            Val::Px(210.0)
        } else {
            Val::Px(10.0)
        };
    }

    if should_toggle {
        sidebar_visible.0 = !sidebar_visible.0;
    }

    if should_toggle || sidebar_visible.is_changed() {
        let display = if sidebar_visible.0 {
            Display::Flex
        } else {
            Display::None
        };
        for mut node in &mut q_sidebar {
            node.display = display;
        }

        for mut node in &mut toggle_queries.p1() {
            node.right = if sidebar_visible.0 {
                Val::Px(210.0)
            } else {
                Val::Px(10.0)
            };
        }

        for mut text in &mut q_toggle_text {
            text.0 = if sidebar_visible.0 {
                ">".to_string()
            } else {
                "<".to_string()
            };
        }
    }
}

fn handle_build_buttons(
    mut interaction_query: Query<(&Interaction, &BuildButton)>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut placement_state: ResMut<PlacementState>,
    team_queues: Res<TeamBuildingQueues>,
    definitions: Res<Definitions>,
    mut build_events: MessageWriter<BuildCommand>,
    mut cancel_events: MessageWriter<CancelBuildCommand>,
    local_player: Res<LocalPlayer>,
) {
    let queue_option = team_queues.0.get(&local_player.0);
    for (interaction, build_btn) in &mut interaction_query {
        let is_hovered_or_pressed = *interaction != Interaction::None;
        let is_left_click = is_hovered_or_pressed && mouse_buttons.just_pressed(MouseButton::Left);
        let is_right_click =
            is_hovered_or_pressed && mouse_buttons.just_pressed(MouseButton::Right);

        let Some(def) = definitions.buildings.get(&build_btn.0) else {
            continue;
        };

        if is_left_click {
            // If this building is ready, enter placement mode
            if let Some(queue) = queue_option {
                if queue.ready.as_ref() == Some(&build_btn.0) {
                    if placement_state.active && placement_state.building_id == build_btn.0 {
                        placement_state.active = false;
                        println!("Cancelled placing {}", def.name);
                    } else {
                        placement_state.active = true;
                        placement_state.building_id = build_btn.0.clone();
                        println!("Placing {}", def.name);
                    }
                    continue;
                }
            }
            // Otherwise, start building via command
            build_events.write(BuildCommand {
                player_id: local_player.0,
                building_id: build_btn.0.clone(),
            });
        } else if is_right_click {
            cancel_events.write(CancelBuildCommand {
                player_id: local_player.0,
                building_id: build_btn.0.clone(),
            });
        }
    }
}

fn handle_tab_clicks(
    mut interaction_query: Query<
        (&Interaction, &TabButton, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>),
    >,
    mut active_tab: ResMut<ActiveTab>,
    mut q_buildings_grid: Query<&mut Node, (With<BuildingsTabGrid>, Without<UnitsTabGrid>)>,
    mut q_units_grid: Query<&mut Node, (With<UnitsTabGrid>, Without<BuildingsTabGrid>)>,
) {
    for (interaction, tab_btn, mut color) in &mut interaction_query {
        match *interaction {
            Interaction::Pressed => {
                *active_tab = tab_btn.0;
            }
            Interaction::Hovered => {
                if *active_tab != tab_btn.0 {
                    color.0 = Color::srgb(0.25, 0.25, 0.25);
                }
            }
            Interaction::None => {
                if *active_tab == tab_btn.0 {
                    color.0 = Color::srgb(0.3, 0.3, 0.3);
                } else {
                    color.0 = Color::srgb(0.2, 0.2, 0.2);
                }
            }
        }
    }

    if active_tab.is_changed() {
        if let Some(mut b_node) = q_buildings_grid.iter_mut().next() {
            b_node.display = if *active_tab == ActiveTab::Buildings {
                Display::Grid
            } else {
                Display::None
            };
        }
        if let Some(mut u_node) = q_units_grid.iter_mut().next() {
            u_node.display = if *active_tab == ActiveTab::Units {
                Display::Grid
            } else {
                Display::None
            };
        }
    }
}

fn handle_produce_buttons(
    mut interaction_query: Query<(&Interaction, &ProduceButton)>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    q_buildings: Query<(
        Entity,
        &Building,
        &ProductionQueue,
        &crate::game::units::Owner,
    )>,
    definitions: Res<Definitions>,
    mut train_events: MessageWriter<TrainUnitCommand>,
    mut cancel_events: MessageWriter<CancelUnitCommand>,
    local_player: Res<LocalPlayer>,
) {
    for (interaction, produce_btn) in &mut interaction_query {
        let is_hovered_or_pressed = *interaction != Interaction::None;
        let is_left_click = is_hovered_or_pressed && mouse_buttons.just_pressed(MouseButton::Left);
        let is_right_click =
            is_hovered_or_pressed && mouse_buttons.just_pressed(MouseButton::Right);

        let Some(def) = definitions.units.get(&produce_btn.0) else {
            continue;
        };

        if is_left_click {
            let mut best_entity = None;
            let mut min_len = usize::MAX;

            for (entity, building, queue, owner) in q_buildings.iter() {
                if owner.0 == local_player.0 && def.produced_by.contains(&building.building_id) {
                    if queue.queue.len() < min_len {
                        min_len = queue.queue.len();
                        best_entity = Some(entity);
                    }
                }
            }

            if let Some(building_entity) = best_entity {
                train_events.write(TrainUnitCommand {
                    player_id: local_player.0,
                    building_entity,
                    unit_id: produce_btn.0.clone(),
                });
            } else {
                println!("No building capable of producing {}", def.name);
            }
        } else if is_right_click {
            let mut best_entity = None;
            for (entity, building, queue, owner) in q_buildings.iter() {
                if owner.0 == local_player.0 && def.produced_by.contains(&building.building_id) {
                    if queue.queue.iter().any(|u| *u == produce_btn.0) {
                        best_entity = Some(entity);
                        break;
                    }
                }
            }

            if let Some(building_entity) = best_entity {
                cancel_events.write(CancelUnitCommand {
                    player_id: local_player.0,
                    building_entity,
                    unit_id: produce_btn.0.clone(),
                });
            } else {
                println!("No {} in queue to cancel", def.name);
            }
        }
    }
}

fn update_production_ui(
    q_buildings: Query<(&ProductionQueue, &crate::game::units::Owner)>,
    mut q_texts: Query<(&mut Text, &ProduceButtonText)>,
    definitions: Res<Definitions>,
    local_player: Res<LocalPlayer>,
) {
    let mut counts = std::collections::HashMap::new();
    let mut active_progress = std::collections::HashMap::new();

    for (queue, owner) in q_buildings.iter() {
        if owner.0 != local_player.0 {
            continue;
        }
        for (i, u) in queue.queue.iter().enumerate() {
            *counts.entry(u.clone()).or_insert(0) += 1;
            if i == 0 {
                let Some(def) = definitions.units.get(u) else {
                    continue;
                };
                let p = active_progress.entry(u.clone()).or_insert(0.0);
                let current_p = queue.progress / def.build_time;
                if current_p > *p {
                    *p = current_p;
                }
            }
        }
    }

    for (mut text, btn_text) in q_texts.iter_mut() {
        let u = &btn_text.0;
        let Some(def) = definitions.units.get(u) else {
            continue;
        };
        let count = counts.get(u).copied().unwrap_or(0);

        if count > 0 {
            let prog = active_progress.get(u).copied().unwrap_or(0.0) * 100.0;
            text.0 = format!("{}\n{} ({:.0}%)", def.name, count, prog);
        } else {
            text.0 = format!("{}", def.name);
        }
    }
}

fn update_building_ui(
    team_queues: Res<TeamBuildingQueues>,
    mut q_texts: Query<(&mut Text, &BuildButtonText)>,
    definitions: Res<Definitions>,
    local_player: Res<LocalPlayer>,
) {
    let queue_option = team_queues.0.get(&local_player.0);
    for (mut text, btn_text) in q_texts.iter_mut() {
        let u = &btn_text.0;
        let Some(def) = definitions.buildings.get(u) else {
            continue;
        };

        if let Some(queue) = queue_option {
            if queue.ready.as_ref() == Some(u) {
                text.0 = format!("{}\nReady", def.name);
            } else if let Some(ref entry) = queue.current {
                if &entry.building_id == u {
                    let prog = (entry.progress / entry.build_time) * 100.0;
                    text.0 = format!("{}\nBuilding ({:.0}%)", def.name, prog);
                } else {
                    text.0 = format!("{}", def.name);
                }
            } else {
                text.0 = format!("{}", def.name);
            }
        } else {
            text.0 = format!("{}", def.name);
        }
    }
}

fn handle_save_load_buttons(
    mut interaction_query: Query<
        (&Interaction, Has<SaveButton>, &mut BackgroundColor),
        (
            Changed<Interaction>,
            With<Button>,
            Or<(With<SaveButton>, With<LoadButton>)>,
        ),
    >,
    mut save_events: MessageWriter<SaveRequest>,
    mut load_events: MessageWriter<LoadRequest>,
) {
    for (interaction, is_save, mut color) in &mut interaction_query {
        match *interaction {
            Interaction::Pressed => {
                if is_save {
                    save_events.write(SaveRequest);
                } else {
                    load_events.write(LoadRequest);
                }
            }
            Interaction::Hovered => {
                color.0 = Color::srgb(0.35, 0.35, 0.35);
            }
            Interaction::None => {
                color.0 = Color::srgb(0.25, 0.25, 0.25);
            }
        }
    }
}

fn handle_cursor_mode_buttons(
    mut interaction_query: Query<
        (&Interaction, Has<SellButton>, &mut BackgroundColor),
        (
            Changed<Interaction>,
            With<Button>,
            Or<(With<SellButton>, With<RepairButton>)>,
        ),
    >,
    mut cursor_mode: ResMut<CursorMode>,
) {
    for (interaction, is_sell, mut color) in &mut interaction_query {
        let is_active = if is_sell {
            *cursor_mode == CursorMode::Sell
        } else {
            *cursor_mode == CursorMode::Repair
        };

        match *interaction {
            Interaction::Pressed => {
                if is_sell {
                    if *cursor_mode == CursorMode::Sell {
                        *cursor_mode = CursorMode::Normal;
                    } else {
                        *cursor_mode = CursorMode::Sell;
                    }
                } else {
                    if *cursor_mode == CursorMode::Repair {
                        *cursor_mode = CursorMode::Normal;
                    } else {
                        *cursor_mode = CursorMode::Repair;
                    }
                }
            }
            Interaction::Hovered => {
                color.0 = if is_active {
                    Color::srgb(0.6, 0.3, 0.3)
                } else {
                    Color::srgb(0.35, 0.35, 0.35)
                };
            }
            Interaction::None => {
                color.0 = if is_active {
                    Color::srgb(0.5, 0.2, 0.2)
                } else {
                    Color::srgb(0.25, 0.25, 0.25)
                };
            }
        }
    }
}
