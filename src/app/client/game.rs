use std::{
    f32,
    mem,
    cell::Ref,
    sync::Arc
};

use nalgebra::{Vector3, Vector2};

use yanyaengine::{TextureId, Transform, Key, KeyCode};

use crate::common::{
    angle_between,
    ENTITY_SCALE,
    render_info::*,
    lazy_transform::*,
    collider::*,
    watcher::*,
    damaging::*,
    particle_creator::*,
    Side1d,
    AnyEntities,
    Parent,
    Faction,
    Physical,
    PhysicalProperties,
    Entity,
    EntityInfo,
    Player,
    Inventory,
    Item,
    ItemInfo,
    ItemsInfo,
    DamagePartial,
    DamageHeight,
    InventoryItem,
    lisp::*,
    world::{TILE_SIZE, Pos3}
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
    info: PlayerInfo
}

impl Game
{
    pub fn new(game_state: &mut GameState) -> Self
    {
        let player_entity = game_state.player();

        let entities = game_state.entities_mut();
        let mouse_entity = entities.push_eager(true, EntityInfo{
            transform: Some(Transform{
                scale: Vector3::repeat(TILE_SIZE * 5.0),
                ..Default::default()
            }),
            collider: Some(ColliderInfo{
                kind: ColliderType::Point,
                ghost: true,
                ..Default::default()
            }.into()),
            ..Default::default()
        });

        let camera_entity = entities.push_eager(true, EntityInfo{
            lazy_transform: Some(LazyTransformInfo{
                connection: Connection::EaseOut{decay: 5.0, limit: None},
                ..Default::default()
            }.into()),
            parent: Some(Parent::new(player_entity, false)),
            ..Default::default()
        });

        let info = PlayerInfo::new(
            game_state.items_info.clone(),
            camera_entity,
            player_entity,
            mouse_entity
        );

        Self{info}
    }

    fn player_container<'a>(&'a mut self, game_state: &'a mut GameState) -> PlayerContainer<'a>
    {
        PlayerContainer::new(&mut self.info, game_state)
    }

    pub fn on_player_connected(&mut self, game_state: &mut GameState)
    {
        self.player_container(game_state).on_player_connected();
    }

    pub fn update(&mut self, game_state: &mut GameState, dt: f32)
    {
        self.player_container(game_state).update(dt)
    }

    pub fn on_control(&mut self, game_state: &mut GameState, state: ControlState, control: Control)
    {
        self.player_container(game_state).on_control(state, control)
    }

    pub fn on_key(&mut self, logical: Key, key: KeyCode) -> bool
    {
        if self.info.console_contents.is_some()
        {
            if key == KeyCode::Enter
            {
                let contents = self.info.console_contents.take().unwrap();
                self.console_command(contents);

                return true;
            }

            let contents = self.info.console_contents.as_mut().unwrap();

            if let Some(text) = logical.to_text()
            {
                *contents += text;
            }

            true
        } else
        {
            false
        }
    }

    fn console_command(&mut self, command: String)
    {
        /*let mut primitives = Primitives::new();

        primitives.add(
            "mouse-colliders",
            PrimitiveProcedureInfo::new_simple(0, move |_state, memory, _env, _args|
            {
                todo!();
                /*let entities = self.game_state.entities();
                entities.collider(self.info.mouse_entity)
                    .map(|x| x.collided().to_vec()).into_iter().flatten()
                    .for_each(|collided|
                    {
                        let info = format!("{:#?}", entities.info_ref(collided));
                        eprintln!("mouse colliding with {collided:?}: {info}");
                    });*/

                memory.push_return(LispValue::new_empty_list());

                Ok(())
            }));

        primitives.add(
            "set-speed",
            PrimitiveProcedureInfo::new_simple(2, move |_state, memory, _env, mut args|
            {
                todo!();
                /*let entity = args.pop(memory).as_symbol(memory)?;

                let mut anatomy = self.game_state.entities_mut()
                    .anatomy_mut(entity)
                    .unwrap();

                anatomy.set_speed(speed);*/

                memory.push_return(LispValue::new_empty_list());

                Ok(())
            }));

        primitives.add(
            "player-entity",
            PrimitiveProcedureInfo::new_simple(0, move |_state, memory, _env, _args|
            {
                todo!();

                memory.push_return(LispValue::new_empty_list());

                Ok(())
            }));

        let config = LispConfig{
            environment: None,
            lambdas: None,
            primitives: Arc::new(primitives)
        };

        let mut lisp = match unsafe{ LispRef::new_with_config(config, &command) }
        {
            Ok(x) => x,
            Err(err) =>
            {
                eprintln!("error parsing {command}: {err}");
                return;
            }
        };

        let mut memory = Lisp::default_memory();
        let result = match lisp.run_with_memory(&mut memory)
        {
            Ok(x) => x,
            Err(err) =>
            {
                eprintln!("error running {command}: {err}");
                return;
            }
        };

        eprintln!("ran command {command}, result: {result}");*/
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
    camera: Entity,
    entity: Entity,
    mouse_entity: Entity,
    other_entity: Option<Entity>,
    projectile: Option<Entity>,
    console_contents: Option<String>,
    stance_time: f32,
    attack_cooldown: f32,
    projectile_lifetime: f32,
    bash_side: Side1d,
    inventory_open: bool,
    other_inventory_open: bool,
    held_distance: f32,
    poke_distance: f32
}

impl PlayerInfo
{
    pub fn new(
        items_info: Arc<ItemsInfo>,
        camera: Entity,
        entity: Entity,
        mouse_entity: Entity
    ) -> Self
    {
        Self{
            items_info,
            camera,
            entity,
            mouse_entity,
            other_entity: None,
            projectile: None,
            console_contents: None,
            stance_time: 0.0,
            attack_cooldown: 0.0,
            projectile_lifetime: 0.0,
            bash_side: Side1d::Left,
            inventory_open: false,
            other_inventory_open: false,
            held_distance: 0.1,
            poke_distance: 0.75
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

    pub fn on_player_connected(&mut self)
    {
        let position = self.player_position();
        let current_tile = self.game_state.tile_of(position.into());

        let r = 3;
        for y in 0..r
        {
            for x in 0..r
            {
                let hr = r / 2;
                let pos = Pos3::new(x - hr, y - hr, 0);

                self.game_state.destroy_tile(current_tile.offset(pos));
            }
        }

        self.camera_sync_instant();
        self.update_inventory(InventoryWhich::Player);
        self.unstance();
    }

    pub fn camera_sync(&mut self)
    {
        let position = self.game_state.entities().transform(self.info.camera)
            .map(|transform| transform.position);

        if let Some(position) = position
        {
            self.game_state.camera.write().set_position(position.into());

            self.game_state.camera_moved(position.into());

            self.camera_sync_z();
        }
    }

    pub fn camera_sync_instant(&mut self)
    {
        let entities = self.game_state.entities();
        if let Some(mut transform) = entities.transform_mut(self.info.camera)
        {
            let parent_transform = entities.parent_transform(self.info.camera);

            *transform = entities.lazy_transform_mut(self.info.camera)
                .unwrap()
                .target_global(parent_transform.as_ref());
        }

        self.camera_sync();
    }

    fn camera_sync_z(&self)
    {
        let camera_z = self.game_state.entities().transform(self.info.camera).unwrap().position.z;

        let z = (camera_z / TILE_SIZE).ceil() * TILE_SIZE;

        let mut camera = self.game_state.camera.write();
        camera.set_position_z(z);
        camera.update();
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
                    .and_then(|x| x.collided().first().copied())
                {
                    if entities.within_interactable_distance(self.info.entity, mouse_touched)
                        && entities.is_lootable(mouse_touched)
                    {
                        self.info.other_entity = Some(mouse_touched);

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
                self.info.console_contents = if self.info.console_contents.is_some()
                {
                    None
                } else
                {
                    Some(String::new())
                };

                let state = if self.info.console_contents.is_some() { "opened" } else { "closed" };
                eprintln!("debug console {state}");
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
                todo!();
                // entities.player_mut(player).unwrap().holding = Some(item);
            },
            UserEvent::Take(item) =>
            {
                if let Some(other_entity) = self.info.other_entity
                {
                    {
                        let entities = self.game_state.entities();
                        let mut inventory = entities.inventory_mut(other_entity).unwrap();

                        let taken = inventory.remove(item);

                        entities.inventory_mut(self.info.entity).unwrap().push(taken);
                    }

                    self.update_inventory(InventoryWhich::Player);
                    self.update_inventory(InventoryWhich::Other);
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
        let ui = &mut self.game_state.ui;
        let inventory_ui = match which
        {
            InventoryWhich::Player => &mut ui.player_inventory,
            InventoryWhich::Other => &mut ui.other_inventory
        };

        let inventory = inventory_ui.body();

        let is_open = match which
        {
            InventoryWhich::Player => self.info.inventory_open,
            InventoryWhich::Other => self.info.other_inventory_open
        };
        
        if is_open
        {
            {
                let entity = match which
                {
                    InventoryWhich::Other => self.info.other_entity.unwrap(),
                    InventoryWhich::Player => self.info.entity
                };

                let mut entity_creator = self.game_state.entities.entity_creator();
                inventory_ui.full_update(&mut entity_creator, entity);
            }

            let entities = self.game_state.entities_mut();
            entities.set_collider(inventory, Some(ColliderInfo{
                kind: ColliderType::Aabb,
                layer: ColliderLayer::Ui,
                move_z: false,
                ..Default::default()
            }.into()));

            *entities.visible_target(inventory).unwrap() = true;

            let mut lazy = entities.lazy_transform_mut(inventory).unwrap();
            lazy.target().scale = Vector3::repeat(0.2);
        } else
        {
            let entities = self.game_state.entities_mut();
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
        /*let player = self.info.entity;

        let entities = self.game_state.entities();
        let held = some_or_return!(entities.player_mut(player).and_then(|mut x| x.holding.take()));

        if let Some(item_info) = self.item_info(held)
        {
            let entity_info = {
                let mouse_transform = entities.transform(self.info.mouse_entity).unwrap();

                let holding_entity = self.game_state.player_entities().holding;
                let holding_transform = entities.transform(holding_entity).unwrap();

                let direction = {
                    let rotation = angle_between(
                        mouse_transform.position,
                        holding_transform.position
                    );

                    Vector3::new(rotation.cos(), -rotation.sin(), 0.0)
                };

                let dust_texture = self.game_state.common_textures.dust;

                let mut physical: Physical = PhysicalProperties{
                    mass: item_info.mass,
                    friction: 0.99,
                    floating: false
                }.into();

                let mass = physical.mass;

                let strength = self.player().newtons() * 0.4;
                let throw_limit = 0.1 * strength;
                let throw_amount = (strength / mass).min(throw_limit);
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
                    damaging: Some(DamagingInfo{
                        damage: DamagingType::Mass(mass),
                        faction: Some(Faction::Player),
                        ..Default::default()
                    }.into()),
                    watchers: Some(Watchers::new(vec![
                        Watcher{
                            kind: WatcherType::Lifetime(2.5.into()),
                            action: WatcherAction::Explode(Box::new(ExplodeInfo{
                                keep: false,
                                info: ParticlesInfo{
                                    amount: 3..5,
                                    speed: ParticleSpeed::Random(0.1),
                                    decay: ParticleDecay::Random(3.5..=5.0),
                                    position: ParticlePosition::Spread(1.0),
                                    rotation: ParticleRotation::Random,
                                    scale: ParticleScale::Spread{
                                        scale: Vector3::repeat(ENTITY_SCALE * 0.4),
                                        variation: 0.1
                                    },
                                    min_scale: ENTITY_SCALE * 0.02
                                },
                                prototype: EntityInfo{
                                    physical: Some(PhysicalProperties{
                                        mass: 0.01,
                                        friction: 0.1,
                                        floating: true
                                    }.into()),
                                    render: Some(RenderInfo{
                                        object: Some(RenderObject::TextureId{
                                            id: dust_texture
                                        }),
                                        z_level: ZLevel::BelowFeet,
                                        ..Default::default()
                                    }),
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
                z_level: ZLevel::Elbow,
                ..Default::default()
            };

            self.game_state.entities.entity_creator().push(entity_info, render_info);

            self.game_state.entities().inventory_mut(player).unwrap().remove(held);
            self.game_state.update_inventory();
        }*/
        todo!();
    }

    fn unstance(&mut self)
    {
        let reminder = "";

        /*let start_rotation = self.default_held_rotation();
        if let Some(mut lazy) = self.game_state.entities().lazy_transform_mut(self.holding_entity())
        {
            lazy.target().rotation = start_rotation;
        }*/
    }

    fn bash_projectile(&mut self, item: Item)
    {
        let item_info = self.game_state.items_info.get(item.id);
        let item_scale = item_info.scale3().y;
        let over_scale = self.info.held_distance + item_scale;
        let scale = 1.0 + over_scale * 2.0;

        let holding_entity = self.holding_entity();

        let damage = DamagePartial{
            data: item_info.bash_damage(),
            height: DamageHeight::random()
        };

        let angle = self.info.bash_side.to_angle() - f32::consts::FRAC_PI_2;

        self.info.projectile_lifetime = 0.2;
        self.info.projectile = Some(self.game_state.entities_mut().push(
            true,
            EntityInfo{
                follow_rotation: Some(FollowRotation::new(
                    holding_entity,
                    Rotation::Instant
                )),
                lazy_transform: Some(LazyTransformInfo{
                    transform: Transform{
                        scale: Vector3::repeat(scale),
                        ..Default::default()
                    },
                    ..Default::default()
                }.into()),
                parent: Some(Parent::new(self.info.entity, true)),
                collider: Some(ColliderInfo{
                    kind: ColliderType::Circle,
                    layer: ColliderLayer::Damage,
                    ghost: true,
                    ..Default::default()
                }.into()),
                damaging: Some(DamagingInfo{
                    damage: DamagingType::Damage{
                        angle,
                        damage
                    },
                    predicate: DamagingPredicate::ParentAngleLess(f32::consts::PI),
                    faction: Some(Faction::Player),
                    ..Default::default()
                }.into()),
                ..Default::default()
            }
        ));
    }

    fn poke_projectile(&mut self, item: Item)
    {
        let item_info = self.game_state.items_info.get(item.id);
        let item_scale = item_info.scale3().y;
        let mut scale = Vector3::repeat(1.0);

        let projectile_scale = self.info.poke_distance / item_scale;
        scale.y += projectile_scale;

        let offset = projectile_scale / 2.0;

        let holding_entity = self.holding_entity();

        let damage = DamagePartial{
            data: item_info.poke_damage(),
            height: DamageHeight::random()
        };

        self.info.projectile_lifetime = 0.2;
        self.info.projectile = Some(self.game_state.entities_mut().push(
            true,
            EntityInfo{
                follow_rotation: Some(FollowRotation::new(
                    holding_entity,
                    Rotation::Instant
                )),
                lazy_transform: Some(LazyTransformInfo{
                    transform: Transform{
                        position: Vector3::new(0.0, offset, 0.0),
                        scale,
                        ..Default::default()
                    },
                    ..Default::default()
                }.into()),
                parent: Some(Parent::new(holding_entity, true)),
                collider: Some(ColliderInfo{
                    kind: ColliderType::Circle,
                    layer: ColliderLayer::Damage,
                    ghost: true,
                    ..Default::default()
                }.into()),
                damaging: Some(DamagingInfo{
                    damage: DamagingType::Damage{
                        angle: 0.0,
                        damage
                    },
                    faction: Some(Faction::Player),
                    ..Default::default()
                }.into()),
                ..Default::default()
            }
        ));

        if let Some(mut lazy) = self.game_state.entities().lazy_transform_mut(holding_entity)
        {
            lazy.connection = Connection::Spring(
                SpringConnection{
                    physical: PhysicalProperties{
                        mass: 0.5,
                        friction: 0.4,
                        floating: true
                    }.into(),
                    limit: 0.004,
                    damping: 0.02,
                    strength: 6.0
                }
            );
        }
    }

    fn bash_attack(&mut self, item: Item)
    {
        if self.info.attack_cooldown > 0.0
        {
            return;
        }

        self.info.attack_cooldown = 0.5;
        self.info.stance_time = self.info.attack_cooldown * 2.0;

        self.info.bash_side = self.info.bash_side.opposite();

        self.bash_projectile(item);

        let holding_entity = self.holding_entity();

        let start_rotation = self.default_held_rotation();
        if let Some(mut lazy) = self.game_state.entities().lazy_transform_mut(holding_entity)
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

            let mut watchers = self.game_state.entities().watchers_mut(holding_entity).unwrap();

            watchers.push(Watcher{
                kind: WatcherType::Lifetime(0.2.into()),
                action: WatcherAction::SetLazyRotation(Rotation::EaseOut(
                    EaseOutRotation{
                        decay: 7.0,
                        speed_significant: 10.0,
                        momentum: 0.5
                    }.into()
                )),
                ..Default::default()
            });
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

    fn poke_attack(&mut self, item: Item)
    {
        /*if self.info.attack_cooldown > 0.0
        {
            return;
        }

        self.unstance();

        self.info.attack_cooldown = 0.5;

        self.poke_projectile(item);

        let entities = self.game_state.entities();

        let holding_entity = self.holding_entity();

        if let Some(mut lazy) = entities.lazy_transform_mut(holding_entity)
        {
            let distance = self.info.poke_distance;

            let lifetime = self.info.attack_cooldown;
            lazy.connection = Connection::Timed{
                lifetime: lifetime.into(),
                remaining: 0.99,
                begin: 0.5
            };

            let held_position = self.held_item_position().unwrap();

            lazy.target().position.x = held_position.x + distance;

            let parent_transform = entities.parent_transform(holding_entity);
            let new_target = lazy.target_global(parent_transform.as_ref());

            entities.transform_mut(holding_entity).unwrap().position = new_target.position;

            let mut watchers = entities.watchers_mut(holding_entity).unwrap();

            let extend_time = 0.2;

            watchers.push(Watcher{
                kind: WatcherType::Lifetime(extend_time.into()),
                action: WatcherAction::SetTargetPosition(held_position),
                ..Default::default()
            });

            watchers.push(Watcher{
                kind: WatcherType::Lifetime(lifetime.into()),
                action: WatcherAction::SetLazyConnection(Connection::Spring(
                    SpringConnection{
                        physical: PhysicalProperties{
                            mass: 0.5,
                            friction: 0.4,
                            floating: true
                        }.into(),
                        limit: 0.004,
                        damping: 0.02,
                        strength: 6.0
                    }
                )),
                ..Default::default()
            });
        }*/
        todo!();
    }

    fn ranged_attack(&mut self, item: Item)
    {
        /*let items_info = self.info.items_info.clone();
        let ranged = some_or_return!(&items_info.get(item.id).ranged);

        self.unstance();

        let start = self.player_position();
        let mut end = self.mouse_position();
        end.z = start.z;
        
        let info = RaycastInfo{
            pierce: None,
            layer: ColliderLayer::Damage,
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
                    let transform = self.game_state.entities().transform(id)
                        .unwrap();

                    let hit_position = hits.hit_position(hit);

                    let angle = angle_between(hit_position, transform.position);

                    let damage = DamagePartial{
                        data: damage,
                        height
                    };

                    drop(transform);
                    self.game_state.damage_entity(angle, id, Faction::Player, damage);
                },
                _ => ()
            }
        }*/
        todo!();
    }

    fn decrease_timer(time_variable: &mut f32, dt: f32) -> bool
    {
        if *time_variable > 0.0
        {
            *time_variable -= dt;

            if *time_variable <= 0.0
            {
                return true;
            }
        }

        false
    }

    pub fn update(&mut self, dt: f32)
    {
        if !self.exists()
        {
            return;
        }

        let decrease_timer = |value|
        {
            Self::decrease_timer(value, dt)
        };

        if Self::decrease_timer(&mut self.info.stance_time, dt)
        {
            self.unstance();
        }

        decrease_timer(&mut self.info.attack_cooldown);

        if decrease_timer(&mut self.info.projectile_lifetime)
        {
            if let Some(entity) = self.info.projectile.take()
            {
                self.game_state.entities_mut().remove(entity);
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

        self.game_state.entities_mut().update_mouse_highlight(
            self.info.entity,
            self.info.mouse_entity
        );

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

            let new_velocity = (physical.velocity + velocity).zip_map(&velocity, |value, limit|
            {
                let limit = limit.abs();

                value.min(limit).max(-limit)
            });

            physical.velocity.x = new_velocity.x;
            physical.velocity.y = new_velocity.y;
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
        /*self.game_state.entities().exists(self.info.entity).then(||
        {
            let player = self.player();
            let inventory = self.inventory();

            player.holding.and_then(|holding| inventory.get(holding).cloned())
        }).flatten()*/
        todo!();
    }

    fn holding_entity(&self) -> Entity
    {
        todo!();
        // self.game_state.player_entities().holding
    }

    fn player_position(&self) -> Vector3<f32>
    {
        self.game_state.entities()
            .transform(self.info.entity)
            .expect("player must have a position")
            .position
    }

    fn mouse_position(&self) -> Vector3<f32>
    {
        self.game_state.entities()
            .transform(self.info.mouse_entity)
            .expect("mouse must have a position")
            .position
    }
}
