mod game;

use bevy::prelude::*;
use game::ai::AiPlugin;
use game::buildings::BuildingsPlugin;
use game::camera::CameraPlugin;
use game::combat::CombatPlugin;
use game::commands::CommandsPlugin;
use game::data::DataPlugin;
use game::economy::EconomyPlugin;
use game::fog_of_war::FogOfWarPlugin;
use game::game_state::GameStatePlugin;
use game::map::MapPlugin;
use game::orders::OrdersPlugin;
use game::pathfinding::PathfindingPlugin;
use game::player::PlayerPlugin;
use game::save_load::SaveLoadPlugin;
use game::selection::SelectionPlugin;
use game::ui::UiPlugin;
use game::units::UnitsPlugin;
use game::visuals::VisualsPlugin;

fn main() {
    App::new()
        .set_error_handler(bevy::ecs::error::warn)
        .add_plugins(DefaultPlugins)
        .add_plugins(PlayerPlugin)
        .add_plugins(DataPlugin)
        .add_plugins(MapPlugin)
        .add_plugins(CameraPlugin)
        .add_plugins(CommandsPlugin)
        .add_plugins(UnitsPlugin)
        .add_plugins(OrdersPlugin)
        .add_plugins(SelectionPlugin)
        .add_plugins(PathfindingPlugin)
        .add_plugins(CombatPlugin)
        .add_plugins(EconomyPlugin)
        .add_plugins(BuildingsPlugin)
        .add_plugins(AiPlugin)
        .add_plugins(UiPlugin)
        .add_plugins(FogOfWarPlugin)
        .add_plugins(SaveLoadPlugin)
        .add_plugins(GameStatePlugin)
        .add_plugins(VisualsPlugin)
        .run();
}
