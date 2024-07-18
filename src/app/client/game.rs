use std::{
    fs,
    f32,
    rc::Rc,
    cell::{RefMut, RefCell}
};

use nalgebra::{Unit, Vector3, Vector2};

use yanyaengine::{TextureId, Transform, Key, KeyCode};

use crate::{
    client::{Ui, UiEvent},
    common::{
        some_or_return,
        render_info::*,
        lazy_transform::*,
        collider::*,
        character::*,
        AnyEntities,
        Item,
        Inventory,
        Parent,
        Entity,
        EntityInfo,
        entity::ClientEntities,
        lisp::{self, *},
        world::{TILE_SIZE, Pos3}
    }
};

use super::game_state::{
    GameState,
    EntityCreator,
    WindowWhich,
    InventoryWhich,
    UserEvent,
    ControlState,
    Control
};

mod object_transform;


pub trait DrawableEntity
{
    fn texture(&self) -> Option<TextureId>;
    fn needs_redraw(&mut self) -> bool;
}

pub struct Game
{
    game_state: Rc<RefCell<GameState>>,
    info: Rc<RefCell<PlayerInfo>>
}

impl Game
{
    pub fn new(game_state: Rc<RefCell<GameState>>) -> Self
    {
        let info = {
            let mut game_state = game_state.borrow_mut();
            let player = game_state.player();

            let entities = game_state.entities_mut();
            let mouse_entity = entities.push_eager(true, EntityInfo{
                transform: Some(Transform{
                    scale: Vector3::repeat(TILE_SIZE * 5.0),
                    ..Default::default()
                }),
                collider: Some(ColliderInfo{
                    kind: ColliderType::Point,
                    layer: ColliderLayer::Mouse,
                    ghost: true,
                    ..Default::default()
                }.into()),
                ..Default::default()
            });

            let console_entity = entities.push_eager(true, EntityInfo{
                lazy_transform: Some(LazyTransformInfo{
                    scaling: Scaling::Ignore,
                    rotation: Rotation::Ignore,
                    transform: Transform{
                        scale: Vector3::new(1.0, 0.2, 1.0),
                        ..Default::default()
                    },
                    ..Default::default()
                }.into()),
                render: Some(RenderInfo{
                    z_level: ZLevel::UiHigh,
                    ..Default::default()
                }),
                parent: Some(Parent::new(player, false)),
                ..Default::default()
            });

            PlayerInfo::new(PlayerCreateInfo{
                camera: game_state.entities.camera_entity,
                entity: player,
                mouse_entity,
                console_entity
            })
        };

        Self{info: Rc::new(RefCell::new(info)), game_state}
    }

    fn player_container<T>(&mut self, f: impl FnOnce(PlayerContainer) -> T) -> T
    {
        let mut game_state = self.game_state.borrow_mut();
        let mut info = self.info.borrow_mut();

        f(PlayerContainer::new(&mut info, &mut game_state))
    }

    pub fn on_player_connected(&mut self)
    {
        {
            let mut game_state = self.game_state.borrow_mut();
            let ui = game_state.ui.clone();
            let info = self.info.clone();

            game_state.entities_mut().on_inventory(Box::new(move |entities, entity|
            {
                let info = info.borrow();

                let which = if entity == info.entity
                {
                    Some(InventoryWhich::Player)
                } else if Some(entity) == info.other_entity
                {
                    Some(InventoryWhich::Other)
                } else
                {
                    None
                };

                if let Some(which) = which
                {
                    let mut ui = ui.borrow_mut();

                    PlayerContainer::update_inventory_inner(
                        entities,
                        &mut ui,
                        &info,
                        which
                    );
                }
            }));
        }

        self.player_container(|mut x| x.on_player_connected());
    }

    pub fn update(&mut self, dt: f32)
    {
        self.game_state.borrow_mut().update_pre(dt);

        self.player_container(|mut x| x.this_update(dt));

        let mut game_state = self.game_state.borrow_mut();
        let changed_this_frame = game_state.controls.changed_this_frame();
        let mouse_position = game_state.world_mouse_position();

        drop(game_state);

        for (state, control) in changed_this_frame
        {
            let event = UiEvent::from_control(mouse_position, state, control);
            if let Some(event) = event
            {
                let mut game_state = self.game_state.borrow_mut();
                let camera_position = game_state.camera.read().position().coords.xy();

                let captured = game_state.entities.entities.update_ui(camera_position, event);

                if captured
                {
                    continue;
                }
            }

            self.on_control(state, control);
        }

        self.game_state.borrow_mut().update(dt);
    }

    pub fn on_control(&mut self, state: ControlState, control: Control)
    {
        self.player_container(|mut x| x.on_control(state, control));
    }

    pub fn on_key(&mut self, logical: Key, key: KeyCode) -> bool
    {
        if self.info.borrow().console_contents.is_some()
        {
            match key
            {
                KeyCode::Enter =>
                {
                    let contents = {
                        let mut info = self.info.borrow_mut();

                        info.console_contents.take().unwrap()
                    };

                    self.console_command(contents);

                    self.player_container(|mut x| x.update_console());

                    return true;
                },
                KeyCode::Backspace =>
                {
                    {
                        let mut info = self.info.borrow_mut();

                        let contents = info.console_contents.as_mut().unwrap();
                        contents.pop();
                    }

                    self.player_container(|mut x| x.update_console());

                    return true;
                },
                _ => ()
            }

            {
                let mut info = self.info.borrow_mut();

                let contents = info.console_contents.as_mut().unwrap();

                if let Some(text) = logical.to_text()
                {
                    *contents += text;
                }
            }

            self.player_container(|mut x| x.update_console());

            true
        } else
        {
            false
        }
    }

    fn pop_entity(args: &mut ArgsWrapper, memory: &mut LispMemory) -> Result<Entity, lisp::Error>
    {
        let lst = args.pop(memory).as_list(memory)?;

        let tag = lst.car().as_symbol(memory)?;
        if tag != "entity"
        {
            let s = format!("(expected tag `entity` got `{tag}`)");

            return Err(lisp::Error::Custom(s));
        }

        let tail = lst.cdr().as_list(memory)?;

        let local = tail.car().as_bool()?;
        let id = tail.cdr().as_integer()?;

        let entity = Entity::from_raw(local, id as usize);

        Ok(entity)
    }

    fn push_entity(env: &Environment, memory: &mut LispMemory, entity: Entity) -> LispValue
    {
        let tag = memory.new_symbol(env, "entity");
        let local = LispValue::new_bool(entity.local());
        let id = LispValue::new_integer(entity.id() as i32);

        let tail = memory.cons(env, local, id);

        memory.cons(env, tag, tail)
    }

    fn add_simple_setter<F>(&self, primitives: &mut Primitives, name: &str, f: F)
    where
        F: Fn(&mut ClientEntities, Entity, &mut LispMemory, ArgsWrapper) -> Result<(), lisp::Error> + 'static
    {
        let game_state = self.game_state.clone();

        primitives.add(
            name,
            PrimitiveProcedureInfo::new_simple_effect(2, move |_state, memory, _env, mut args|
            {
                let mut game_state = game_state.borrow_mut();
                let entities = game_state.entities_mut();

                let entity = Self::pop_entity(&mut args, memory)?;
                f(entities, entity, memory, args)?;

                memory.push_return(LispValue::new_empty_list());

                Ok(())
            }));
    }

    fn console_command(&mut self, command: String)
    {
        let mut primitives = Primitives::new();

        {
            let game_state = self.game_state.clone();
            let mouse_entity = self.info.borrow().mouse_entity;

            primitives.add(
                "mouse-collided",
                PrimitiveProcedureInfo::new_simple(0, move |_state, memory, env, _args|
                {
                    let game_state = game_state.borrow();
                    let entities = game_state.entities();

                    let collided = entities.collider(mouse_entity)
                        .map(|x| x.collided().to_vec()).into_iter().flatten()
                        .next();

                    let entity = collided.map(|collided| Self::push_entity(env, memory, collided))
                        .unwrap_or_else(LispValue::new_empty_list);

                    memory.push_return(entity);

                    Ok(())
                }));
        }

        self.add_simple_setter(&mut primitives, "set-floating", |entities, entity, memory, mut args|
        {
            let state = args.pop(memory).as_bool()?;

            let mut physical = entities.physical_mut(entity).unwrap();

            physical.floating = state;

            Ok(())
        });

        self.add_simple_setter(&mut primitives, "set-speed", |entities, entity, memory, mut args|
        {
            let speed = args.pop(memory).as_float()?;

            let mut anatomy = entities.anatomy_mut(entity).unwrap();

            anatomy.set_speed(speed);

            Ok(())
        });

        self.add_simple_setter(&mut primitives, "set-faction", |entities, entity, memory, mut args|
        {
            let faction = args.pop(memory).as_symbol(memory)?;
            let faction: String = faction.to_lowercase().chars().enumerate().map(|(i, c)|
            {
                if i == 0
                {
                    c.to_ascii_uppercase()
                } else
                {
                    c
                }
            }).collect();

            let faction = format!("\"{faction}\"");
            let faction = serde_json::from_str(&faction).map_err(|_|
            {
                lisp::Error::Custom(format!("cant deserialize {faction} as Faction"))
            })?;

            let mut character = entities.character_mut(entity).unwrap();

            character.faction = faction;

            Ok(())
        });

        {
            let game_state = self.game_state.clone();

            primitives.add(
                "add-item",
                PrimitiveProcedureInfo::new_simple_effect(2, move |_state, memory, _env, mut args|
                {
                    let game_state = game_state.borrow_mut();
                    let entities = game_state.entities();

                    let entity = Self::pop_entity(&mut args, memory)?;
                    let name = args.pop(memory).as_symbol(memory)?.replace('_', " ");

                    let mut inventory = entities.inventory_mut(entity).unwrap();

                    let id = game_state.items_info.get_id(&name).ok_or_else(||
                    {
                        lisp::Error::Custom(format!("item named {name} doesnt exist"))
                    })?;

                    inventory.push(Item{id});

                    memory.push_return(LispValue::new_empty_list());

                    Ok(())
                }));
        }

        {
            let player_entity = self.info.borrow().entity;

            primitives.add(
                "player-entity",
                PrimitiveProcedureInfo::new_simple(0, move |_state, memory, env, _args|
                {
                    let entity = Self::push_entity(env, memory, player_entity);

                    memory.push_return(entity);

                    Ok(())
                }));
        }

        {
            let game_state = self.game_state.clone();

            primitives.add(
                "print-entity-info",
                PrimitiveProcedureInfo::new_simple_effect(1, move |_state, memory, _env, mut args|
                {
                    let game_state = game_state.borrow();
                    let entities = game_state.entities();

                    let entity = Self::pop_entity(&mut args, memory)?;

                    eprintln!("entity info: {}", entities.info_ref(entity));

                    memory.push_return(LispValue::new_empty_list());

                    Ok(())
                }));
        }

        let standard_code = {
            let path = "lisp/standard.scm";

            fs::read_to_string(path)
                .unwrap_or_else(|err| panic!("{path} must exist ({err})"))
        };

        let (environment, lambdas) = Lisp::new_mappings_lambdas(&standard_code)
            .unwrap_or_else(|err| panic!("error in stdlib: {err}"));

        let config = LispConfig{
            environment: Some(Rc::new(RefCell::new(environment))),
            lambdas: Some(lambdas),
            primitives: Rc::new(primitives)
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

        eprintln!("ran command {command}, result: {result}");
    }

    pub fn player_exists(&mut self) -> bool
    {
        self.player_container(|x| x.exists())
    }

    pub fn camera_sync(&mut self)
    {
        self.player_container(|mut x| x.camera_sync());
    }
}

struct PlayerCreateInfo
{
    pub camera: Entity,
    pub entity: Entity,
    pub mouse_entity: Entity,
    pub console_entity: Entity
}

struct PlayerInfo
{
    camera: Entity,
    entity: Entity,
    mouse_entity: Entity,
    other_entity: Option<Entity>,
    console_entity: Entity,
    console_contents: Option<String>,
    previous_stamina: Option<f32>,
    previous_cooldown: (f32, f32),
    inventory_open: bool,
    other_inventory_open: bool
}

impl PlayerInfo
{
    pub fn new(info: PlayerCreateInfo) -> Self
    {
        Self{
            camera: info.camera,
            entity: info.entity,
            mouse_entity: info.mouse_entity,
            other_entity: None,
            console_entity: info.console_entity,
            console_contents: None,
            previous_stamina: None,
            previous_cooldown: (0.0, 0.0),
            inventory_open: false,
            other_inventory_open: false
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
        self.update_inventory(InventoryWhich::Other);
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
            if let Some(parent_transform) = entities.parent_transform(self.info.camera)
            {
                transform.position = parent_transform.position;
            }
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
        if let Control::Crawl = control
        {
            let entities = self.game_state.entities();
            if let Some(mut anatomy) = entities.anatomy_mut(self.info.entity)
            {
                anatomy.override_crawling(state.to_bool());

                drop(anatomy);
                entities.anatomy_changed(self.info.entity);
            }
        }

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

                self.character_action(CharacterAction::Bash);
            },
            Control::Poke =>
            {
                self.character_action(CharacterAction::Poke);
            },
            Control::Shoot =>
            {
                let mut target = self.mouse_position();
                target.z = self.player_position().z;

                self.character_action(CharacterAction::Ranged(target));
            },
            Control::Throw =>
            {
                let mouse_transform = self.game_state.entities()
                    .transform(self.info.mouse_entity)
                    .unwrap();

                self.character_action(CharacterAction::Throw(mouse_transform.position));
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

                self.update_console();

                let state = if self.info.console_contents.is_some() { "opened" } else { "closed" };
                eprintln!("debug console {state}");
            },
            _ => ()
        }
    }

    fn character_action(&self, action: CharacterAction)
    {
        if let Some(mut character) = self.game_state.entities().character_mut(self.info.entity)
        {
            character.push_action(action);
        }
    }

    fn update_console(&mut self)
    {
        self.game_state.entities()
            .parent_mut(self.info.console_entity)
            .unwrap()
            .visible = self.info.console_contents.is_some();

        let text = self.info.console_contents.clone().unwrap_or_default();

        let object = RenderObjectKind::Text{
            text,
            font_size: 30,
            font: FontStyle::Sans,
            align: TextAlign::centered()
        }.into();

        self.game_state.entities().set_deferred_render_object(self.info.console_entity, object);
    }

    fn handle_user_event(&mut self, event: UserEvent)
    {
        let player = self.info.entity;

        self.game_state.close_popup();
        match event
        {
            UserEvent::Popup{anchor, responses} =>
            {
                self.game_state.create_popup(anchor, responses);
            },
            UserEvent::Info{which, item} =>
            {
                if let Some(item) = self.get_inventory(which)
                    .and_then(|inventory| inventory.get(item).cloned())
                {
                    self.game_state.create_info_window(item);
                } else
                {
                    eprintln!("tried to show info for an item that doesnt exist");
                }
            },
            UserEvent::Drop{which, item} =>
            {
                if self.get_inventory(which)
                    .and_then(|mut inventory| inventory.remove(item))
                    .is_some() && which == InventoryWhich::Player
                {
                    if let Some(mut character) = self.game_state.entities()
                        .character_mut(self.info.entity)
                    {
                        character.dropped_item(item);
                    }
                } else
                {
                    eprintln!("tried to drop item that doesnt exist");
                }
            },
            UserEvent::Close(which) =>
            {
                match which
                {
                    WindowWhich::ItemInfo =>
                    {
                        self.game_state.close_info_window();
                    },
                    WindowWhich::Inventory(which) =>
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
                    }
                }
            },
            UserEvent::Wield(item) =>
            {
                self.game_state.entities().character_mut(player).unwrap().set_holding(Some(item));
            },
            UserEvent::Take(item) =>
            {
                if let Some(taken) = self.get_inventory(InventoryWhich::Other)
                    .and_then(|mut inventory| inventory.remove(item))
                {
                    self.game_state.entities()
                        .inventory_mut(self.info.entity)
                        .unwrap()
                        .push(taken);
                } else
                {
                    eprintln!("tried to take item that doesnt exist");
                }
            }
        }
    }

    fn get_inventory_entity(&self, which: InventoryWhich) -> Option<Entity>
    {
        match which
        {
            InventoryWhich::Other => self.info.other_entity,
            InventoryWhich::Player => Some(self.info.entity)
        }
    }

    fn get_inventory(&self, which: InventoryWhich) -> Option<RefMut<Inventory>>
    {
        let entity = self.get_inventory_entity(which);

        entity.and_then(|entity| self.game_state.entities().inventory_mut(entity))
    }

    fn update_user_events(&mut self)
    {
        let events = self.game_state.user_receiver.borrow_mut().consume();
        events.for_each(|event|
        {
            self.handle_user_event(event);
        });
    }

    fn toggle_inventory(&mut self)
    {
        self.info.inventory_open = !self.info.inventory_open;

        self.update_inventory(InventoryWhich::Player);
    }

    fn update_inventory(&mut self, which: InventoryWhich)
    {
        let entities = &mut self.game_state.entities.entities;
        let mut ui = self.game_state.ui.borrow_mut();

        Self::update_inventory_inner(
            entities,
            &mut ui,
            self.info,
            which
        );
    }

    fn update_inventory_inner(
        entities: &mut ClientEntities,
        ui: &mut Ui,
        info: &PlayerInfo,
        which: InventoryWhich
    )
    {
        let is_open = match which
        {
            InventoryWhich::Player => info.inventory_open,
            InventoryWhich::Other => info.other_inventory_open
        };
        
        if is_open
        {
            let entity = match which
            {
                InventoryWhich::Other => info.other_entity.unwrap(),
                InventoryWhich::Player => info.entity
            };

            let mut entity_creator = EntityCreator{entities};

            let inventory = match which
            {
                InventoryWhich::Player => &mut ui.player_inventory,
                InventoryWhich::Other => &mut ui.other_inventory
            };

            inventory.full_update(&mut entity_creator, entity);
        }

        ui.set_inventory_state(entities, which, is_open);
    }

    pub fn this_update(&mut self, _dt: f32)
    {
        if !self.exists()
        {
            return;
        }

        self.update_user_events();

        let mouse_position = self.game_state.world_mouse_position();
        let mouse_position = Vector3::new(mouse_position.x, mouse_position.y, 0.0);
        let camera_position = self.game_state.camera.read().position().coords;

        self.game_state.entities()
            .transform_mut(self.info.mouse_entity)
            .unwrap()
            .position = camera_position + mouse_position;

        self.game_state.entities_mut().update_mouse_highlight(
            self.info.entity,
            self.info.mouse_entity
        );

        if let Some(character) = self.game_state.entities().character(self.info.entity)
        {
            let delay = 0.7;

            let current_stamina = character.stamina_fraction(self.game_state.entities());

            if self.info.previous_stamina != current_stamina
            {
                self.info.previous_stamina = current_stamina;

                let id = self.game_state.ui_notifications.stamina;

                self.game_state.set_bar(id, current_stamina.unwrap_or(0.0));
                self.game_state.activate_notification(id, delay);
            }

            let current_cooldown = character.attack_cooldown();
            if self.info.previous_cooldown.1 != current_cooldown
            {
                self.info.previous_cooldown.1 = current_cooldown;

                self.info.previous_cooldown.0 = if current_cooldown <= 0.0
                {
                    0.0
                } else
                {
                    self.info.previous_cooldown.0.max(current_cooldown)
                };

                let id = self.game_state.ui_notifications.weapon_cooldown;

                let fraction = if self.info.previous_cooldown.0 > 0.0
                {
                    current_cooldown / self.info.previous_cooldown.0
                } else
                {
                    0.0
                };
                
                self.game_state.set_bar(id, fraction);
                self.game_state.activate_notification(id, delay);
            }
        }

        if let Some(movement) = self.movement_direction()
        {
            if let Some(mut character) = self.game_state.entities()
                .character_mut(self.info.entity)
            {
                character.sprinting = self.game_state.pressed(Control::Sprint);
            }

            self.walk(movement);
        }

        let able_to_move = self.game_state.entities()
            .anatomy(self.info.entity)
            .map(|anatomy| anatomy.speed().is_some())
            .unwrap_or(false);

        if able_to_move
        {
            self.look_at_mouse();
        }

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

        movement_direction.and_then(|x|
        {
            Unit::try_new(x, 0.1).as_deref().copied()
        }).map(|mut direction|
        {
            if self.game_state.entities().physical(self.info.entity).map(|x|
            {
                x.floating
            }).unwrap_or(false)
            {
                let flight_speed = 0.1;

                direction.z = if self.game_state.pressed(Control::Jump)
                {
                    flight_speed
                } else if self.game_state.pressed(Control::Crawl)
                {
                    -flight_speed
                } else
                {
                    0.0
                };
            }

            direction
        })
    }

    pub fn walk(&mut self, direction: Vector3<f32>)
    {
        let entities = self.game_state.entities();
        if let Some(character) = entities.character(self.info.entity)
        {
            let anatomy = some_or_return!(entities.anatomy(self.info.entity));
            let mut physical = some_or_return!(entities.physical_mut(self.info.entity));

            character.walk(&anatomy, &mut physical, direction);
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

        let mut character = self.game_state.entities().character_mut(self.info.entity).unwrap();
        let player_transform = self.game_state.entities()
            .transform(self.info.entity)
            .expect("player must have a transform");

        let player_pos = player_transform.position.xy();

        let player_offset = player_pos - camera_pos;

        let pos = look_position - player_offset;

        let rotation = pos.y.atan2(pos.x);

        character.rotation = rotation;
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
