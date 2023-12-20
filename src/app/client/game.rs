use nalgebra::Vector3;

use yanyaengine::TransformContainer;

use crate::{
    client::ConnectionsHandler,
    common::{
        EntitiesController,
        NetworkEntity,
        player::Player,
        world::TILE_SIZE,
        physics::PhysicsEntity
    }
};

use super::game_state::{
    GameState,
    MousePosition,
    Control,
    object_pair::ObjectPair
};

mod object_transform;


pub trait DrawableEntity
{
    fn texture(&self) -> &str;
}

pub struct Game
{
    player: PlayerInfo
}

impl Game
{
    pub fn new(self_id: usize) -> Self
    {
        let player = PlayerInfo::new(self_id);

        Self{player}
    }

    fn player_container<'a>(&'a mut self, game_state: &'a mut GameState) -> PlayerContainer<'a>
    {
        PlayerContainer::new(&mut self.player, game_state)
    }

    pub fn on_player_connected(&mut self, game_state: &mut GameState)
    {
        self.player_container(game_state).camera_sync_instant();
    }

    pub fn update(&mut self, game_state: &mut GameState, dt: f32)
    {
        self.player_container(game_state).update(dt)
    }

    pub fn player_exists(&mut self, game_state: &mut GameState) -> bool
    {
        self.player_container(game_state).exists()
    }

    pub fn camera_sync(&mut self, game_state: &mut GameState)
    {
        self.player_container(game_state).camera_sync();
    }
}

struct PlayerInfo
{
    id: usize,
    camera_follow: f32
}

impl PlayerInfo
{
    pub fn new(id: usize) -> Self
    {
        Self{id, camera_follow: 0.25}
    }
}

struct PlayerContainer<'a>
{
    info: &'a mut PlayerInfo,
    game_state: &'a mut GameState
}

impl<'a> PlayerContainer<'a>
{
    pub fn new(info: &'a mut PlayerInfo, game_state: &'a mut GameState) -> Self
    {
        Self{info, game_state}
    }

    pub fn exists(&self) -> bool
    {
        self.game_state.entities.player_exists(self.info.id)
    }

    pub fn camera_sync(&self)
    {
        let position = self.player_ref().transform_ref().position;

        self.game_state.camera.write().translate_to(&position, self.info.camera_follow);

        self.camera_sync_z();
    }

    pub fn camera_sync_instant(&self)
    {
        let position = self.player_ref().transform_ref().position;

        self.game_state.camera.write().set_position(position.into());

        self.camera_sync_z();
    }

    fn camera_sync_z(&self)
    {
        let player_z = self.player_ref().transform_ref().position.z;

        let z = (player_z / TILE_SIZE).ceil() * TILE_SIZE;

        self.game_state.camera.write().set_position_z(z);
    }

    pub fn update(&mut self, _dt: f32)
    {
        if !self.exists()
        {
            return;
        }

        if let Some(movement) = self.movement_direction()
        {
            self.walk(movement);
        }

        self.look_at(self.game_state.mouse_position);
    }

    fn movement_direction(&self) -> Option<Vector3<f32>>
    {
        let mut movement_direction = None;

        let mut move_direction = |direction: Vector3<f32>|
        {
            if let Some(movement) = movement_direction.as_mut()
            {
                *movement += direction;
            } else
            {
                movement_direction = Some(direction);
            }
        };

        if self.game_state.pressed(Control::MoveRight)
        {
            move_direction(Vector3::x());
        }

        if self.game_state.pressed(Control::MoveLeft)
        {
            move_direction(-Vector3::x());
        }

        if self.game_state.pressed(Control::MoveUp)
        {
            move_direction(-Vector3::y());
        }

        if self.game_state.pressed(Control::MoveDown)
        {
            move_direction(Vector3::y());
        }

        if self.game_state.pressed(Control::Jump)
        {
            move_direction(Vector3::z());
        }

        if self.game_state.pressed(Control::Crouch)
        {
            move_direction(-Vector3::z());
        }

        if let Some(direction) = movement_direction.as_mut()
        {
            direction.try_normalize_mut(1.0);
        }

        movement_direction
    }

    pub fn walk(&mut self, direction: Vector3<f32>)
    {
        let mut player = self.player_mut();

        let velocity = direction * player.inner().entity.speed();

        player.set_velocity(velocity);
    }

    pub fn look_at(&mut self, mouse_position: MousePosition)
    {
        let (mouse_x, mouse_y) = (mouse_position.x - 0.5, mouse_position.y - 0.5);

        let (aspect, camera_pos) = {
            let camera_ref = self.game_state.camera.read();

            (camera_ref.aspect(), camera_ref.position().xy().coords)
        };

        let mut player_mut = self.player_mut();
        let player_pos = player_mut.position().xy();

        let player_offset = player_pos - camera_pos;

        let player_offset = (player_offset.x / aspect.0, player_offset.y / aspect.1);

        let (x, y) = (mouse_x - player_offset.0, mouse_y - player_offset.1);

        let rotation = y.atan2(x);

        player_mut.set_rotation(rotation);
    }

    fn player_ref(&self) -> &ObjectPair<Player>
    {
        self.game_state.player_ref(self.info.id)
    }

    fn player_mut(&mut self) -> NetworkEntity<'_, ConnectionsHandler, ObjectPair<Player>>
    {
        self.game_state.player_mut(self.info.id)
    }
}
