use nalgebra::Vector3;

use yanyaengine::TransformContainer;

use crate::common::{
    EntitiesController,
    physics::PhysicsEntity
};

use super::game_state::{
    GameState,
    MousePosition,
    Control
};

mod object_transform;


pub trait DrawableEntity
{
    fn texture(&self) -> &str;
}

pub struct Game
{
    player: PlayerContainer
}

impl Game
{
    pub fn new(self_id: usize) -> Self
    {
        let player = PlayerContainer::new(self_id);

        Self{player}
    }

    pub fn on_player_connected(&mut self, game_state: &mut GameState)
    {
        self.player.camera_sync_instant(game_state);
    }

    pub fn update(&mut self, game_state: &mut GameState, dt: f32)
    {
        if !self.player_exists(game_state)
        {
            return;
        }

        let (movement_direction, moved) = Self::movement_direction(game_state);

        if moved
        {
            self.player.walk(game_state, dt, movement_direction);
        }

        self.player.look_at(game_state, game_state.mouse_position);
    }

    pub fn player_exists(&mut self, game_state: &mut GameState) -> bool
    {
        self.player.exists(game_state)
    }

    pub fn camera_sync(&mut self, game_state: &mut GameState)
    {
        self.player.camera_sync(game_state);
    }

    fn movement_direction(game_state: &mut GameState) -> (Vector3<f32>, bool)
    {
        let mut movement_direction = Vector3::zeros();
        let mut moved = false;

        if game_state.pressed(Control::MoveRight)
        {
            movement_direction.x += 1.0;
            moved = true;
        }

        if game_state.pressed(Control::MoveLeft)
        {
            movement_direction.x -= 1.0;
            moved = true;
        }

        if game_state.pressed(Control::MoveUp)
        {
            movement_direction.y -= 1.0;
            moved = true;
        }

        if game_state.pressed(Control::MoveDown)
        {
            movement_direction.y += 1.0;
            moved = true;
        }

        if game_state.pressed(Control::Jump)
        {
            movement_direction.z += 0.1;
            moved = true;
        }

        if game_state.pressed(Control::Crouch)
        {
            movement_direction.z -= 0.1;
            moved = true;
        }

        (movement_direction, moved)
    }
}

struct PlayerContainer
{
    id: usize,
    camera_follow: f32
}

impl PlayerContainer
{
    pub fn new(id: usize) -> Self
    {
        Self{id, camera_follow: 0.25}
    }

    pub fn exists(&self, game_state: &mut GameState) -> bool
    {
        game_state.entities.player_exists(self.id)
    }

    pub fn walk(&self, game_state: &mut GameState, dt: f32, direction: Vector3<f32>)
    {
        let mut player = game_state.player_mut(self.id);

        let change = direction * player.inner().entity.speed() * dt;

        player.velocity_add(change);
    }

    pub fn look_at(&self, game_state: &mut GameState, mouse_position: MousePosition)
    {
        let (mouse_x, mouse_y) = (mouse_position.x - 0.5, mouse_position.y - 0.5);

        let (aspect, camera_pos) = {
            let camera_ref = game_state.camera.read();

            (camera_ref.aspect(), camera_ref.position().xy().coords)
        };

        let mut player_mut = game_state.player_mut(self.id);
        let player_pos = player_mut.position().xy();

        let player_offset = player_pos - camera_pos;

        let player_offset = (player_offset.x / aspect.0, player_offset.y / aspect.1);

        let (x, y) = (mouse_x - player_offset.0, mouse_y - player_offset.1);

        let rotation = y.atan2(x);

        player_mut.set_rotation(rotation);
    }

    pub fn camera_sync(&self, game_state: &mut GameState)
    {
        let player = game_state.player_ref(self.id);

        let position = player.transform_ref().position;

        let mut camera_mut = game_state.camera.write();
        camera_mut.translate_to(&position, self.camera_follow);
        camera_mut.set_position_z(position.z);
    }

    pub fn camera_sync_instant(&self, game_state: &mut GameState)
    {
        let player = game_state.player_ref(self.id);

        let position = player.transform_ref().position;

        game_state.camera.write().set_position(position.into());
    }
}
