use std::{
    sync::Arc
};

use parking_lot::RwLock;

use nalgebra::{
    Vector3
};

use crate::common::{
    EntitiesController,
    TransformContainer,
    physics::PhysicsEntity
};

use super::game_state::{
    GameState,
    Control
};

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
        self.player.camera_sync();
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

        self.player.camera_sync();
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

    pub fn walk(&self, dt: f32, direction: Vector3<f32>)
    {
        self.game_state.write().player_mut(self.id).velocity_add(direction * dt);
    }

    pub fn camera_sync(&self)
    {
        let mut writer = self.game_state.write();

        let position = writer.player_mut(self.id).transform_ref().position;
        writer.camera.write().set_position(position + self.camera_origin);
    }
}