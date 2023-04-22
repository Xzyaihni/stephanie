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
    Control,
    MousePosition
};

pub use object_factory::ObjectFactory;

pub mod object;
pub mod object_factory;

mod object_transform;
pub mod camera;


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
        self.camera_sync(game_state);
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

        self.player.world_moved(game_state);

        self.player.look_at(game_state, game_state.mouse_position);
    }

    pub fn player_exists(&mut self, game_state: &mut GameState) -> bool
    {
        self.player.exists(game_state)
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

        (movement_direction, moved)
    }

    pub fn camera_sync(&mut self, game_state: &mut GameState)
    {
        self.player.camera_sync(game_state);
    }

    #[allow(dead_code)]
    #[cfg(debug_assertions)]
    pub fn print_camera_pos_positions(&self, game_state: &GameState)
    {
        self.player.print_camera_pos_positions(game_state);
    }
}

struct PlayerContainer
{
    id: usize
}

impl PlayerContainer
{
    pub fn new(id: usize) -> Self
    {
        Self{id}
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
        let (x, y) = (mouse_position.x - 0.5, mouse_position.y - 0.5);

        let rotation = y.atan2(x);

        game_state.player_mut(self.id).set_rotation(rotation);
    }

    pub fn camera_sync(&self, game_state: &mut GameState)
    {
        let player = game_state.player_ref(self.id);

        let position = player.transform_ref().position;

        game_state.camera.write().set_position(position);
    }

    pub fn world_moved(&self, game_state: &mut GameState)
    {
        let player = game_state.player_ref(self.id);

        let position = player.transform_ref().position;
        game_state.player_moved(position.into());
    }

    #[allow(dead_code)]
    #[cfg(debug_assertions)]
    pub fn print_camera_pos_positions(&self, game_state: &GameState)
    {
        eprintln!("========================");
        eprintln!("position: {:#?}", game_state.player_ref(self.id).transform_ref());
        eprintln!("camera:   {:#?}", game_state.camera.read().transform_ref());
        eprintln!("========================");
    }
}