use bevy::prelude::*;

use crate::components::Player;

pub fn spin(mut q: Query<(&mut Transform, &Player)>, time: Res<Time>) {
    for (mut t, player) in &mut q {
        t.rotate_z(player.spin_speed * time.delta().as_secs_f32());
    }
}
