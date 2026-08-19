mod components;
mod game_state;
mod systems;

use bevy::prelude::*;

use game_state::GameState;
use systems::{movement, setup};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "MegaGate bevy game".to_string(),
                resolution: (960., 540.).into(),
                ..default()
            }),
            ..default()
        }))
        .init_state::<GameState>()
        .add_systems(Startup, setup::spawn_camera)
        .add_systems(OnEnter(GameState::InGame), setup::spawn_player)
        .add_systems(Update, movement::spin)
        .run();
}