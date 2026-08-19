use bevy::prelude::*;

use crate::components::Player;

pub fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

pub fn spawn_player(mut commands: Commands) {
    commands.spawn((
        Sprite {
            color: Color::srgb(0.2, 0.8, 0.3),
            custom_size: Some(Vec2::splat(80.0)),
            ..default()
        },
        Player { spin_speed: 1.5 },
    ));
}