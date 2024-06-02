use std::{
    mem,
    cell::Ref,
    sync::Arc
};

use nalgebra::{Vector3, Vector2};

use yanyaengine::{TextureId, Transform};

use crate::common::{
    ENTITY_SCALE,
    render_info::*,
    lazy_transform::*,
    collider::*,
    watcher::*,
    AnyEntities,
    Physical,
    PhysicalProperties,
    Entity,
    EntityInfo,
    Player,
    Inventory,
    Weapon,
    ItemInfo,
    ItemsInfo,
    Damage,
    DamageDirection,
    Side2d,
    DamageHeight,
    InventoryItem,
    world::TILE_SIZE
};

use super::game_state::{
    GameState,
    UserEvent,
    ControlState,
    Control,
    RaycastInfo,
    RaycastHitId
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
        let mut container = self.player_container(game_state);
        container.camera_sync_instant();
        container.update_inventory();
    }

    pub fn update(&mut self, game_state: &mut GameState, dt: f32)
    {
        self.player_container(game_state).update(dt)
    }

    pub fn on_control(&mut self, game_state: &mut GameState, state: ControlState, control: Control)
    {
        self.player_container(game_state).on_control(state, control)
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
    inventory_open: bool,
    camera_follow: f32
}

impl PlayerInfo
{
    pub fn new(items_info: Arc<ItemsInfo>, entity: Entity) -> Self
    {
        Self{items_info, entity, inventory_open: false, camera_follow: 0.25}
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

    pub fn on_control(&mut self, state: ControlState, control: Control)
    {
        if state != ControlState::Pressed
        {
            return;
        }

        match control
        {
            Control::MainAction =>
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
            },
            Control::Throw =>
            {
                self.throw_held();
            },
            Control::Inventory =>
            {
                self.toggle_inventory();
            },
            Control::DebugConsole if self.game_state.debug_mode =>
            {
                dbg!("make this an actual console thingy later");

                let mut anatomy = self.game_state.entities_mut()
                    .anatomy_mut(self.info.entity)
                    .unwrap();

                if let Some(speed) = anatomy.speed()
                {
                    anatomy.set_speed(speed * 2.0);
                }
            },
            _ => ()
        }
    }

    fn handle_user_event(&mut self, event: UserEvent)
    {
        let entities = &mut self.game_state.entities.entities;
        let player = self.info.entity;

        match event
        {
            UserEvent::Wield(item) =>
            {
                entities.player_mut(player).unwrap().holding = Some(item);

                self.update_weapon();
            },
            UserEvent::Take(item) =>
            {
                todo!();
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

    fn update_weapon(&mut self)
    {
        let holding_entity = self.game_state.player_entities().holding;

        let entities = self.game_state.entities();
        let mut render = entities.render_mut(holding_entity).unwrap();
        let mut parent = entities.parent_mut(holding_entity).unwrap();

        let player = self.player();

        parent.visible = player.holding.is_some();
        if let Some(holding) = player.holding
        {
            let item = self.item_info(holding);

            let assets = self.game_state.assets.lock();
            let texture = assets.texture(item.texture);

            render.set_texture(texture.clone());

            entities.target(holding_entity).unwrap().scale = item.scale3();
        }
    }

    fn toggle_inventory(&mut self)
    {
        self.info.inventory_open = !self.info.inventory_open;

        self.update_inventory();
    }

    fn update_inventory(&mut self)
    {
        let inventory = self.game_state.ui.player_inventory.body();
        let local_entities = &mut self.game_state.entities.local_entities;

        if self.info.inventory_open 
        {
            local_entities.set_collider(inventory, Some(ColliderInfo{
                kind: ColliderType::Aabb,
                layer: ColliderLayer::Ui,
                ..Default::default()
            }.into()));

            *local_entities.visible_target(inventory).unwrap() = true;

            let mut lazy = local_entities.lazy_transform_mut(inventory).unwrap();
            lazy.target().scale = Vector3::repeat(0.2);
        } else
        {
            local_entities.set_collider(inventory, None);

            {
                let mut lazy = local_entities.lazy_transform_mut(inventory).unwrap();
                lazy.target().scale = Vector3::zeros();
            }

            let watchers = local_entities.watchers_mut(inventory);
            if let Some(mut watchers) = watchers
            {
                let watcher = Watcher{
                    kind: WatcherType::ScaleDistance{from: Vector3::zeros(), near: 0.2},
                    action: WatcherAction::SetVisible(false)
                };

                watchers.push(watcher);
            }
        }
    }

    fn throw_held(&mut self)
    {
        let player = self.info.entity;

        let held = self.game_state.entities_mut().player_mut(player).unwrap().holding.take();
        if let Some(held) = held
        {
            let item_info = self.item_info(held);
            let entity_info = {
                let holding_entity = self.game_state.player_entities().holding;
                let holding_transform = self.game_state.entities()
                    .transform(holding_entity)
                    .unwrap();

                let direction = {
                    let origin_rotation = self.game_state.entities()
                        .lazy_transform(holding_entity)
                        .unwrap()
                        .origin_rotation();

                    let rotation = holding_transform.rotation + origin_rotation;

                    Vector3::new(rotation.cos(), rotation.sin(), 0.0)
                };

                let dust_texture = self.game_state.common_textures.dust;

                let mut physical: Physical = PhysicalProperties{
                    mass: item_info.mass,
                    friction: 0.5,
                    floating: false
                }.into();

                let strength = self.player().newtons();
                let throw_limit = 0.1 * strength;
                let throw_amount = (strength / physical.mass).min(throw_limit);
                physical.velocity = direction * throw_amount;

                EntityInfo{
                    physical: Some(physical),
                    lazy_transform: Some(LazyTransformInfo{
                        deformation: Deformation::Stretch(StretchDeformation{
                            animation: ValueAnimation::EaseOut(2.0),
                            limit: 2.0,
                            onset: 0.05,
                            strength: 2.0
                        }),
                        transform: Transform{
                            position: holding_transform.position,
                            rotation: holding_transform.rotation,
                            scale: item_info.scale3() * ENTITY_SCALE,
                            ..Default::default()
                        },
                        ..Default::default()
                    }.into()),
                    collider: Some(ColliderInfo{
                        kind: ColliderType::Circle,
                        ..Default::default()
                    }.into()),
                    watchers: Some(Watchers::new(vec![
                        Watcher{
                            kind: WatcherType::Lifetime(2.5),
                            action: WatcherAction::Explode(ExplodeInfo{
                                amount: 3..5,
                                speed: 0.1,
                                info: EntityInfo{
                                    physical: Some(PhysicalProperties{
                                        mass: 0.05,
                                        friction: 0.1,
                                        floating: true
                                    }.into()),
                                    lazy_transform: Some(LazyTransformInfo{
                                        scaling: Scaling::EaseOut{decay: 4.0},
                                        transform: Transform{
                                            scale: Vector3::repeat(ENTITY_SCALE * 0.4),
                                            ..Default::default()
                                        },
                                        ..Default::default()
                                    }.into()),
                                    render: Some(RenderInfo{
                                        object: Some(RenderObject::TextureId{
                                            id: dust_texture
                                        }),
                                        z_level: ZLevel::Low,
                                        ..Default::default()
                                    }),
                                    watchers: Some(Watchers::new(vec![
                                        Watcher{
                                            kind: WatcherType::Instant,
                                            action: WatcherAction::SetTargetScale(Vector3::zeros())
                                        },
                                        Watcher{
                                            kind: WatcherType::ScaleDistance{
                                                from: Vector3::zeros(),
                                                near: 0.01
                                            },
                                            action: WatcherAction::Remove
                                        }
                                    ])),
                                    ..Default::default()
                                }
                            })
                        }
                    ])),
                    ..Default::default()
                }
            };

            let render_info = RenderInfo{
                object: Some(RenderObject::TextureId{
                    id: item_info.texture
                }),
                z_level: ZLevel::Middle,
                ..Default::default()
            };

            self.game_state.entities.entity_creator().push(entity_info, render_info);

            self.game_state.entities_mut().inventory_mut(player).unwrap().remove(held);
            self.game_state.update_inventory();

            self.update_weapon();
        }
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

                        let hit_position = hits.hit_position(hit);
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
            x.z *= TILE_SIZE;

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

            physical.velocity = (physical.velocity + velocity).zip_map(&velocity, |value, limit|
            {
                let limit = limit.abs();

                value.min(limit).max(-limit)
            });
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

    fn item_info(&self, id: InventoryItem) -> &ItemInfo
    {
        let inventory = self.inventory();
        self.game_state.items_info.get(inventory.get(id).id)
    }

    fn player_position(&self) -> Vector3<f32>
    {
        self.game_state.entities()
            .transform(self.info.entity)
            .expect("player must have a position")
            .position
    }
}
