use std::{
    mem,
    cell::Ref,
    sync::Arc
};

use nalgebra::{Vector3, Vector2};

use yanyaengine::TextureId;

use crate::common::{
    Entity,
    Player,
    Inventory,
    Weapon,
    ItemsInfo,
    RenderObject,
    Damage,
    DamageDirection,
    Side2d,
    DamageHeight,
    world::TILE_SIZE
};

use super::game_state::{
    GameState,
    UserEvent,
    Control,
    RaycastInfo,
    RaycastHitId,
    ReplaceObject
};

mod object_transform;


pub trait DrawableEntity
{
    fn texture(&self) -> Option<TextureId>;
    fn needs_redraw(&mut self) -> bool;
}

pub struct Game
{
    player: PlayerInfo
}

impl Game
{
    pub fn new(game_state: &GameState, player: Entity) -> Self
    {
        let player = PlayerInfo::new(game_state.items_info.clone(), player);

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
    items_info: Arc<ItemsInfo>,
    entity: Entity,
    camera_follow: f32
}

impl PlayerInfo
{
    pub fn new(items_info: Arc<ItemsInfo>, entity: Entity) -> Self
    {
        Self{items_info, entity, camera_follow: 0.25}
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
        self.game_state.entities.player_exists()
    }

    pub fn camera_sync(&self)
    {
        let position = self.player_position();

        self.game_state.camera.write().translate_to(&position, self.info.camera_follow);

        self.camera_sync_z();
    }

    pub fn camera_sync_instant(&self)
    {
        let position = self.player_position();

        self.game_state.camera.write().set_position(position.into());

        self.camera_sync_z();
    }

    fn camera_sync_z(&self)
    {
        let player_z = self.player_position().z;

        let z = (player_z / TILE_SIZE).ceil() * TILE_SIZE;

        self.game_state.camera.write().set_position_z(z);
    }

    fn handle_user_event(&mut self, event: UserEvent)
    {
        match event
        {
            UserEvent::Wield(item) =>
            {
                let entities = &mut self.game_state.entities;
                let player = entities.main_player();
                entities.entities.player_mut(player).unwrap().holding = Some(item);

                let player = self.player();

                if let Some(holding) = player.holding
                {
                    let inventory = self.inventory();
                    let holding = inventory.get(holding);

                    let items_info = self.info.items_info.clone();
                    let weapon = &items_info.get(holding.id).weapon;

                    drop(player);
                    drop(inventory);

                    self.update_weapon(weapon);
                }
            }
        }
    }

    fn update_user_events(&mut self)
    {
        let events = mem::take(&mut *self.game_state.user_receiver.borrow_mut());

        events.into_iter().for_each(|event|
        {
            self.handle_user_event(event);
        });
    }

    fn update_weapon(&mut self, weapon: &Weapon)
    {
        let holding = self.game_state.player_entities().holding;
        let render = RenderObject::Texture{
            name: "items/weapons/pistol.png".to_owned()
        };

        self.game_state.object_change(holding, ReplaceObject::Object(render));
    }

    fn weapon_attack(&mut self, weapon: &Weapon)
    {
        let start = self.player_position();

        let mouse = self.game_state.world_mouse_position();
        
        let end = start + Vector3::new(mouse.x, mouse.y, 0.0);

        let info = RaycastInfo{
            pierce: None,
            ignore_player: true,
            ignore_end: true
        };

        let hits = self.game_state.raycast(info, &start, &end);

        let damage = weapon.damage();

        let height = DamageHeight::random();

        for hit in &hits.hits
        {
            #[allow(clippy::single_match)]
            match hit.id
            {
                RaycastHitId::Entity(id) =>
                {
                    let side = {
                        let transform = self.game_state.entities().transform(id)
                            .unwrap();

                        let hit_position = hits.hit_position(&hit);
                        Side2d::from_positions(
                            transform.rotation,
                            transform.position,
                            hit_position
                        )
                    };

                    let direction = DamageDirection{side, height};

                    let damage = Damage::new(direction, damage);

                    self.game_state.damage_entity(id, damage);
                },
                _ => ()
            }
        }
    }

    pub fn update(&mut self, _dt: f32)
    {
        if !self.exists()
        {
            return;
        }

        self.update_user_events();

        if self.game_state.clicked(Control::MainAction)
        {
            let player = self.player();

            if let Some(holding) = player.holding
            {
                let inventory = self.inventory();
                let holding = inventory.get(holding);

                let items_info = self.info.items_info.clone();
                let weapon = &items_info.get(holding.id).weapon;

                drop(player);
                drop(inventory);

                self.weapon_attack(weapon);
            }
        }

        if self.game_state.debug_mode && self.game_state.clicked(Control::DebugConsole)
        {
            dbg!("make this an actual console thingy later");

            let mut anatomy = self.game_state.entities_mut()
                .anatomy_mut(self.info.entity)
                .unwrap();

            if let Some(speed) = anatomy.speed()
            {
                anatomy.set_speed(speed * 2.0);
            }
        }

        if let Some(movement) = self.movement_direction()
        {
            self.walk(movement);
        }

        self.look_at_mouse();

        self.game_state.sync_transform(self.info.entity);
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

        movement_direction.map(|mut x|
        {
            x.z *= 0.1;

            x
        })
    }

    pub fn walk(&mut self, direction: Vector3<f32>)
    {
        let entities = self.game_state.entities_mut();

        if let Some(speed) = entities.anatomy(self.info.entity).unwrap().speed()
        {
            let mut physical = entities.physical_mut(self.info.entity).unwrap();

            let velocity = direction * (speed / physical.mass);

            physical.velocity = velocity;
        }
    }

    pub fn look_at_mouse(&mut self)
    {
        let mouse = self.game_state.world_mouse_position();

        self.look_at(mouse)
    }

    pub fn look_at(&mut self, look_position: Vector2<f32>)
    {
        let camera_pos = self.game_state.camera.read().position().xy().coords;

        let mut player_transform = self.game_state.entities_mut()
            .transform_mut(self.info.entity)
            .expect("player must have a transform");

        let player_pos = player_transform.position.xy();

        let player_offset = player_pos - camera_pos;

        let pos = look_position - player_offset;

        let rotation = pos.y.atan2(pos.x);

        player_transform.rotation = rotation;
    }

    fn player(&self) -> Ref<Player>
    {
        self.game_state.entities()
            .player(self.info.entity)
            .unwrap()
    }

    fn inventory(&self) -> Ref<Inventory>
    {
        self.game_state.entities()
            .inventory(self.info.entity)
            .unwrap()
    }

    fn player_position(&self) -> Vector3<f32>
    {
        self.game_state.entities()
            .transform(self.info.entity)
            .expect("player must have a position")
            .position
    }
}
