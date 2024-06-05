use std::{
    f32,
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
    Side1d,
    AnyEntities,
    Physical,
    PhysicalProperties,
    Entity,
    EntityInfo,
    Player,
    Inventory,
    Item,
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
    InventoryWhich,
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
    pub fn new(game_state: &mut GameState) -> Self
    {
        let player_entity = game_state.player();
        let mouse_entity = game_state.entities_mut().push(true, EntityInfo{
            transform: Some(Transform{
                scale: Vector3::repeat(0.1),
                ..Default::default()
            }),
            collider: Some(ColliderInfo{
                kind: ColliderType::Point,
                ghost: true,
                ..Default::default()
            }.into()),
            ..Default::default()
        });

        let player = PlayerInfo::new(
            game_state.items_info.clone(),
            player_entity,
            mouse_entity
        );

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
        container.update_inventory(InventoryWhich::Player);
        container.unstance();
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
    mouse_entity: Entity,
    stance_time: f32,
    bash_side: Side1d,
    inventory_open: bool,
    other_inventory_open: bool,
    camera_follow: f32
}

impl PlayerInfo
{
    pub fn new(
        items_info: Arc<ItemsInfo>,
        entity: Entity,
        mouse_entity: Entity
    ) -> Self
    {
        Self{
            items_info,
            entity,
            mouse_entity,
            stance_time: 0.0,
            bash_side: Side1d::Left,
            inventory_open: false,
            other_inventory_open: false,
            camera_follow: 0.25
        }
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
                let entities = self.game_state.entities();

                if let Some(mouse_touched) = entities.collider(self.info.mouse_entity)
                    .and_then(|x| *x.collided())
                {
                    if entities.is_lootable(mouse_touched)
                    {
                        self.info.other_inventory_open = true;
                        self.update_inventory(InventoryWhich::Other);

                        return;
                    }
                }

                if let Some(holding) = self.held_item()
                {
                    self.bash_attack(holding);
                }
            },
            Control::Poke =>
            {
                if let Some(holding) = self.held_item()
                {
                    self.poke_attack(holding);
                }
            },
            Control::Shoot =>
            {
                if let Some(holding) = self.held_item()
                {
                    self.ranged_attack(holding);
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
        let entities = self.game_state.entities_mut();
        let player = self.info.entity;

        match event
        {
            UserEvent::Close(which) =>
            {
                match which
                {
                    InventoryWhich::Player =>
                    {
                        self.info.inventory_open = false;
                    },
                    InventoryWhich::Other =>
                    {
                        self.info.other_inventory_open = false;
                    }
                }

                self.update_inventory(which);
            },
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
            if let Some(item) = self.item_info(holding)
            {
                let assets = self.game_state.assets.lock();
                let texture = assets.texture(item.texture);

                let mut lazy_transform = entities.lazy_transform_mut(holding_entity).unwrap();
                let target = lazy_transform.target();
                let scale = item.scale3();

                let distance_from_player = 0.1;

                let offset = scale.y / 2.0 + 0.5 + distance_from_player;

                target.position = Vector3::new(offset, 0.0, 0.0);
                target.scale = scale;

                render.set_texture(texture.clone());

                drop(parent);
                let parent_transform = entities.parent_transform(holding_entity);
                let new_target = lazy_transform.target_global(parent_transform.as_ref());

                let mut transform = entities.transform_mut(holding_entity).unwrap();
                transform.scale = new_target.scale;
                transform.position = new_target.position;
            } else
            {
                parent.visible = false;
            }
        }
    }

    fn toggle_inventory(&mut self)
    {
        self.info.inventory_open = !self.info.inventory_open;

        self.update_inventory(InventoryWhich::Player);
    }

    fn update_inventory(
        &mut self,
        which: InventoryWhich
    )
    {
        let ui = &self.game_state.ui;
        let inventory_id = match which
        {
            InventoryWhich::Player => &ui.player_inventory,
            InventoryWhich::Other => &ui.other_inventory
        };

        let inventory = inventory_id.body();
        let entities = self.game_state.entities_mut();

        let is_open = match which
        {
            InventoryWhich::Player => self.info.inventory_open,
            InventoryWhich::Other => self.info.other_inventory_open
        };
        
        if is_open
        {
            entities.set_collider(inventory, Some(ColliderInfo{
                kind: ColliderType::Aabb,
                layer: ColliderLayer::Ui,
                ..Default::default()
            }.into()));

            *entities.visible_target(inventory).unwrap() = true;

            let mut lazy = entities.lazy_transform_mut(inventory).unwrap();
            lazy.target().scale = Vector3::repeat(0.2);
        } else
        {
            entities.set_collider(inventory, None);

            let current_scale;
            {
                let mut lazy = entities.lazy_transform_mut(inventory).unwrap();
                current_scale = lazy.target_ref().scale;
                lazy.target().scale = Vector3::zeros();
            }

            let watchers = entities.watchers_mut(inventory);
            if let Some(mut watchers) = watchers
            {
                let near = 0.2 * current_scale.max();

                let watcher = Watcher{
                    kind: WatcherType::ScaleDistance{from: Vector3::zeros(), near},
                    action: WatcherAction::SetVisible(false),
                    ..Default::default()
                };

                watchers.push(watcher);
            }
        }
    }

    fn throw_held(&mut self)
    {
        let player = self.info.entity;

        let entities = self.game_state.entities();
        let held = entities.player_mut(player).unwrap().holding.take();
        let held = if let Some(x) = held
        {
            x
        } else
        {
            return;
        };

        if let Some(item_info) = self.item_info(held)
        {
            let entity_info = {
                let holding_entity = self.game_state.player_entities().holding;
                let holding_transform = entities.transform(holding_entity).unwrap();

                let direction = {
                    let rotation = entities.transform(player).unwrap().rotation;

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
                            kind: WatcherType::Lifetime(2.5.into()),
                            action: WatcherAction::Explode(Box::new(ExplodeInfo{
                                keep: false,
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
                                        z_level: ZLevel::Lower,
                                        ..Default::default()
                                    }),
                                    watchers: Some(Watchers::new(vec![
                                        Watcher{
                                            kind: WatcherType::Instant,
                                            action: WatcherAction::SetTargetScale(Vector3::zeros()),
                                            ..Default::default()
                                        },
                                        Watcher{
                                            kind: WatcherType::ScaleDistance{
                                                from: Vector3::zeros(),
                                                near: 0.01
                                            },
                                            action: WatcherAction::Remove,
                                            ..Default::default()
                                        }
                                    ])),
                                    ..Default::default()
                                }
                            })),
                            ..Default::default()
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

            self.game_state.entities().inventory_mut(player).unwrap().remove(held);
            self.game_state.update_inventory();

            self.update_weapon();
        }
    }

    fn unstance(&mut self)
    {
        let start_rotation = self.default_held_rotation();
        if let Some(mut lazy) = self.game_state.entities().lazy_transform_mut(self.holding_entity())
        {
            match &mut lazy.rotation
            {
                Rotation::EaseOut(x) => x.set_decay(7.0),
                _ => ()
            }

            lazy.target().rotation = start_rotation;
        }
    }

    fn bash_attack(&mut self, item: Item)
    {
        self.info.stance_time = 5.0;

        eprintln!("hitting from {:?}", self.info.bash_side);

        self.info.bash_side = self.info.bash_side.opposite();

        let start_rotation = self.default_held_rotation();
        if let Some(mut lazy) = self.game_state.entities().lazy_transform_mut(self.holding_entity())
        {
            let edge = 0.4;

            let new_rotation = match self.info.bash_side
            {
                Side1d::Left =>
                {
                    f32::consts::FRAC_PI_2 - edge
                },
                Side1d::Right =>
                {
                    -f32::consts::FRAC_PI_2 + edge
                }
            };

            match &mut lazy.rotation
            {
                Rotation::EaseOut(x) => x.set_decay(30.0),
                _ => ()
            }

            lazy.target().rotation = start_rotation - new_rotation;
        }
    }

    fn default_held_rotation(&self) -> f32
    {
        let origin_rotation = self.game_state.entities()
            .lazy_transform(self.holding_entity())
            .unwrap()
            .origin_rotation();

        -origin_rotation
    }

    fn poke_attack(&self, item: Item)
    {
        todo!()
    }

    fn ranged_attack(&self, item: Item)
    {
        let items_info = self.info.items_info.clone();
        let ranged = if let Some(x) = &items_info.get(item.id).ranged
        {
            x
        } else
        {
            return;
        };

        let start = self.player_position();

        let mouse = self.game_state.world_mouse_position();
        
        let end = start + Vector3::new(mouse.x, mouse.y, 0.0);

        let info = RaycastInfo{
            pierce: None,
            ignore_player: true,
            ignore_end: true
        };

        let hits = self.game_state.raycast(info, &start, &end);

        let damage = ranged.damage();

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

    pub fn update(&mut self, dt: f32)
    {
        if !self.exists()
        {
            return;
        }

        if self.info.stance_time > 0.0
        {
            self.info.stance_time -= dt;

            if self.info.stance_time <= 0.0
            {
                self.unstance();
            }
        }

        self.update_user_events();

        let mouse_position = self.game_state.world_mouse_position();
        let mouse_position = Vector3::new(mouse_position.x, mouse_position.y, 0.0);
        let camera_position = self.game_state.camera.read().position().coords;

        self.game_state.entities_mut()
            .transform_mut(self.info.mouse_entity)
            .unwrap()
            .position = camera_position + mouse_position;

        self.game_state.entities_mut().update_mouse_highlight(self.info.mouse_entity);

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

    fn item_info(&self, id: InventoryItem) -> Option<&ItemInfo>
    {
        let inventory = self.inventory();
        inventory.get(id).map(|x| self.game_state.items_info.get(x.id))
    }

    fn held_item(&self) -> Option<Item>
    {
        let player = self.player();
        let inventory = self.inventory();

        player.holding.and_then(|holding| inventory.get(holding).cloned())
    }

    fn holding_entity(&self) -> Entity
    {
        self.game_state.player_entities().holding
    }

    fn player_position(&self) -> Vector3<f32>
    {
        self.game_state.entities()
            .transform(self.info.entity)
            .expect("player must have a position")
            .position
    }
}
