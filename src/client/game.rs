use std::{
    sync::Arc
};

use parking_lot::{
    RwLock,
    RwLockWriteGuard
};

use nalgebra::{
    Vector3
};

use crate::common::{
    EntitiesController
};

use super::game_state::{
    GameState,
    Control
};

use crate::common::TransformContainer;

pub use object_factory::ObjectFactory;

pub mod object;
pub mod object_factory;

mod object_transform;
pub mod camera;


pub struct Game
{
    game_state: Arc<RwLock<GameState>>,
    player: PlayerContainer
}

impl Game
{
    pub fn new(
        game_state: Arc<RwLock<GameState>>,
        self_id: usize
    ) -> Self
    {
        let player = PlayerContainer::new(game_state.clone(), self_id);

        Self{game_state, player}
    }

    pub fn player_connected(&mut self)
    {
        self.player.camera_sync(self.game_state.write());
    }

    pub fn update(&mut self, dt: f32)
    {
        if !self.player.exists()
        {
            return;
        }

        let mut movement_direction = Vector3::zeros();
        let mut moved = false;

        {
            let state = self.game_state.read();

            if state.pressed(Control::MoveRight)
            {
                movement_direction.x += 1.0;
                moved = true;
            }

            if state.pressed(Control::MoveLeft)
            {
                movement_direction.x -= 1.0;
                moved = true;
            }

            if state.pressed(Control::MoveUp)
            {
                movement_direction.y -= 1.0;
                moved = true;
            }

            if state.pressed(Control::MoveDown)
            {
                movement_direction.y += 1.0;
                moved = true;
            }
        }

        if moved
        {
            self.player.walk(dt, movement_direction);
        }
    }
}

struct PlayerContainer
{
    game_state: Arc<RwLock<GameState>>,
    id: usize,
    camera_origin: Vector3<f32>
}

impl PlayerContainer
{
    pub fn new(game_state: Arc<RwLock<GameState>>, id: usize) -> Self
    {
        let camera_origin = -game_state.read().camera.read().origin();

        Self{game_state, id, camera_origin}
    }

    pub fn exists(&self) -> bool
    {
        self.game_state.read().entities.player_exists(self.id)
    }

    pub fn walk(&mut self, dt: f32, direction: Vector3<f32>)
    {
        let direction = direction * dt;

        let mut writer = self.game_state.write();
        writer.player_mut(self.id).translate(direction);

        self.camera_sync(writer);
    }

    fn camera_sync(&self, mut lock: RwLockWriteGuard<GameState>)
    {
        let position = lock.player_mut(self.id).transform_ref().position;
        lock.camera.write().set_position(position + self.camera_origin);
    }
}