use bevy::prelude::*;
use std::collections::HashMap;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Players>()
            .insert_resource(LocalPlayer(0));
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlayerController {
    LocalHuman,
    NetworkHuman,
    AI,
    None,
}

#[derive(Clone, Debug)]
pub struct Player {
    pub id: usize,
    pub team_id: usize,
    pub faction: String,
    pub controller: PlayerController,
    pub color: Color,
    pub credits: u32,
}

#[derive(Resource, Default)]
pub struct Players {
    pub players: HashMap<usize, Player>,
}

#[derive(Resource)]
pub struct LocalPlayer(pub usize);
