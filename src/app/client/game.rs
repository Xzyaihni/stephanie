use std::{
    fs,
    f32,
    rc::{Rc, Weak},
    cell::{RefMut, RefCell}
};

use nalgebra::{Unit, Vector3, Vector2};

use yanyaengine::{
    Transform,
    Key,
    KeyCode,
    NamedKey,
    ModelId,
    game_object::*
};

use crate::common::{
    some_or_value,
    some_or_return,
    collider::*,
    character::*,
    SpecialTile,
    AnyEntities,
    Item,
    Inventory,
    Entity,
    EntityInfo,
    entity::ClientEntities,
    lisp::{self, *},
    world::{CHUNK_VISUAL_SIZE, TILE_SIZE, Pos3, TilePos}
};

use super::game_state::{
    GameState,
    WindowCreateInfo,
    InventoryWhich,
    UiEvent,
    GameUiEvent,
    ControlState,
    Control
};


enum ConsoleOutput
{
    Quiet,
    Normal
}

impl ConsoleOutput
{
    fn is_quiet(&self) -> bool
    {
        if let Self::Quiet = self
        {
            true
        } else
        {
            false
        }
    }
}

pub struct Game
{
    game_state: Weak<RefCell<GameState>>,
    info: Rc<RefCell<PlayerInfo>>
}

impl Game
{
    pub fn new(game_state: Weak<RefCell<GameState>>) -> Self
    {
        let info = {
            let game_state = game_state.upgrade().unwrap();
            let mut game_state = game_state.borrow_mut();
            let player = game_state.player();

            let entities = game_state.entities_mut();
            let mouse_entity = entities.push_eager(true, EntityInfo{
                transform: Some(Transform{
                    scale: Vector3::repeat(TILE_SIZE * 5.0),
                    ..Default::default()
                }),
                collider: Some(ColliderInfo{
                    kind: ColliderType::RayZ,
                    layer: ColliderLayer::Mouse,
                    ghost: true,
                    ..Default::default()
                }.into()),
                ..Default::default()
            });

            PlayerInfo::new(PlayerCreateInfo{
                camera: game_state.entities.camera_entity,
                follow: game_state.entities.follow_entity,
                entity: player,
                mouse_entity
            })
        };

        let mut this = Self{info: Rc::new(RefCell::new(info)), game_state};

        let standard_code = {
            let load = |path: &str|
            {
                fs::read_to_string(path)
                    .unwrap_or_else(|err| panic!("{path} must exist ({err})"))
            };

            load("lisp/standard.scm") + &load("lisp/console.scm")
        };

        let primitives = this.console_primitives();

        {
            let mut infos = this.info.borrow_mut();
            infos.console.primitives = Some(primitives);
            infos.console.past_commands = standard_code;
        }

        this.console_command(String::new(), ConsoleOutput::Quiet);

        this
    }

    fn player_container<T>(&mut self, f: impl FnOnce(PlayerContainer) -> T) -> T
    {
        let game_state = self.game_state.upgrade().unwrap();
        let mut game_state = game_state.borrow_mut();
        let mut info = self.info.borrow_mut();

        f(PlayerContainer::new(&mut info, &mut game_state))
    }

    pub fn on_player_connected(&mut self)
    {
        {
            let game_state = self.game_state.upgrade().unwrap();
            let mut game_state = game_state.borrow_mut();
            let ui = game_state.ui.clone();
            let info = self.info.clone();

            game_state.entities_mut().on_inventory(Box::new(move |entities, entity|
            {
                let mut info = info.borrow_mut();

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
                    let entity = match which
                    {
                        InventoryWhich::Other => info.other_entity.unwrap(),
                        InventoryWhich::Player => info.entity
                    };

                    ui.borrow_mut().inventory_changed(entity);
                }
            }));
        }

        self.player_container(|mut x| x.on_player_connected());
    }

    pub fn update(
        &mut self,
        squares: ModelId,
        info: &mut UpdateBuffersInfo,
        dt: f32
    )
    {
        let game_state = self.game_state.upgrade().unwrap();
        game_state.borrow_mut().update_pre(dt);

        self.player_container(|mut x| x.this_update(dt));

        let mut game_state_mut = game_state.borrow_mut();
        let mut changed_this_frame = game_state_mut.controls.changed_this_frame();

        game_state_mut.ui_update(&mut changed_this_frame);

        let controls: Vec<_> = game_state_mut.controls.consume_changed(changed_this_frame).collect();

        drop(game_state_mut);

        controls.into_iter().for_each(|(control, state)| self.on_control(state, control));

        game_state.borrow_mut().update(squares, info, dt);

        self.camera_sync();
    }

    pub fn on_control(&mut self, state: ControlState, control: Control)
    {
        self.player_container(|mut x| x.on_control(state, control));
    }

    pub fn on_key_state(&mut self, logical: Key, key: KeyCode, pressed: bool) -> bool
    {
        if logical == Key::Named(NamedKey::Control)
        {
            self.info.borrow_mut().ctrl_held = pressed;
        }

        if pressed
        {
            self.on_key(logical, key)
        } else
        {
            false
        }
    }

    fn on_key(&mut self, logical: Key, key: KeyCode) -> bool
    {
        if self.info.borrow().console.contents.is_some()
        {
            match key
            {
                KeyCode::KeyV =>
                {
                    let mut info = self.info.borrow_mut();
                    if info.ctrl_held
                    {
                        let contents = info.console.contents.as_mut().unwrap();

                        match self.game_state.upgrade().unwrap().borrow_mut()
                            .controls
                            .get_clipboard()
                        {
                            Ok(x) =>
                            {
                                *contents += &x;
                            },
                            Err(err) =>
                            {
                                eprintln!("error pasting from clipboard: {err}");
                            }
                        }

                        drop(info);

                        self.player_container(|mut x| x.update_console());

                        return true;
                    }
                },
                KeyCode::Enter =>
                {
                    let contents = {
                        let mut info = self.info.borrow_mut();

                        info.console.contents.take().unwrap()
                    };

                    self.console_command(contents, ConsoleOutput::Normal);

                    self.player_container(|mut x| x.update_console());

                    return true;
                },
                KeyCode::Escape =>
                {
                    self.info.borrow_mut().console.contents.take();

                    self.player_container(|mut x| x.update_console());

                    return true;
                },
                KeyCode::Backspace =>
                {
                    {
                        let mut info = self.info.borrow_mut();

                        let contents = info.console.contents.as_mut().unwrap();
                        contents.pop();
                    }

                    self.player_container(|mut x| x.update_console());

                    return true;
                },
                _ => ()
            }

            {
                let mut info = self.info.borrow_mut();

                let contents = info.console.contents.as_mut().unwrap();

                if let Some(text) = logical.to_text()
                {
                    *contents += text;
                }
            }

            self.player_container(|mut x| x.update_console());

            true
        } else
        {
            let is_debug = || -> bool
            {
                self.game_state.upgrade().map(|game_state| game_state.borrow().debug_mode)
                    .unwrap_or(false)
            };

            if key == KeyCode::Backquote && is_debug()
            {
                {
                    let mut info = self.info.borrow_mut();
                    info.console.contents = if info.console.contents.is_some()
                    {
                        None
                    } else
                    {
                        Some(String::new())
                    };
                }

                self.player_container(|mut x| x.update_console());

                let state = if self.info.borrow().console.contents.is_some() { "opened" } else { "closed" };
                eprintln!("debug console {state}");

                true
            } else
            {
                false
            }
        }
    }

    fn pop_entity(args: &mut PrimitiveArgs) -> Result<Entity, lisp::Error>
    {
        let mut values = args.next().unwrap().as_pairs_list(args.memory)?.into_iter();

        let tag = values.next().unwrap().as_symbol(args.memory)?;
        if tag != "entity"
        {
            let s = format!("(expected tag `entity` got `{tag}`)");

            return Err(lisp::Error::Custom(s));
        }

        let local = values.next().unwrap().as_bool()?;
        let id = values.next().unwrap().as_integer()?;

        let entity = Entity::from_raw(local, id as usize);

        Ok(entity)
    }

    fn push_entity(memory: &mut LispMemory, entity: Entity) -> Result<LispValue, lisp::Error>
    {
        let tag = memory.new_symbol("entity");
        let local = LispValue::new_bool(entity.local());
        let id = LispValue::new_integer(entity.id() as i32);

        memory.cons_list([tag, local, id])
    }

    fn add_simple_setter<F>(&self, primitives: &mut Primitives, name: &str, f: F)
    where
        F: Fn(
            &mut ClientEntities,
            Entity,
            &mut LispMemory,
            LispValue
        ) -> Result<(), lisp::Error> + 'static
    {
        let game_state = self.game_state.clone();

        primitives.add(
            name,
            PrimitiveProcedureInfo::new_simple(2, Effect::Impure, move |mut args|
            {
                let game_state = game_state.upgrade().unwrap();
                let mut game_state = game_state.borrow_mut();
                let entities = game_state.entities_mut();

                let entity = Self::pop_entity(&mut args)?;
                let value = args.next().unwrap();

                f(entities, entity, args.memory, value)?;

                Ok(().into())
            }));
    }

    fn maybe_print_component(
        game_state: &Weak<RefCell<GameState>>,
        args: &mut PrimitiveArgs,
        print: bool
    ) -> Result<LispValue, lisp::Error>
    {
        let game_state = game_state.upgrade().unwrap();
        let game_state = game_state.borrow();
        let entities = game_state.entities();

        let entity = Self::pop_entity(args)?;
        let component = args.next().unwrap().as_symbol(args.memory)?;

        let maybe_info = entities.component_info(entity, &component);

        let found = maybe_info.is_some();

        if print
        {
            if let Some(info) = maybe_info
            {
                eprintln!("{component}: {info}");
            } else
            {
                eprintln!("{component} doesnt exist");
            }
        }

        Ok(found.into())
    }

    fn console_primitives(&mut self) -> Rc<Primitives>
    {
        macro_rules! get_component_mut
        {
            ($name:ident, $entities:expr, $entity:expr) =>
            {
                {
                    let name = stringify!($name).trim_end_matches("_mut");

                    some_or_value!(
                        $entities.$name($entity),
                        Err(lisp::Error::Custom(format!("component {name} is missing")))
                    )
                }
            }
        }

        let mut primitives = Primitives::default();

        {
            let game_state = self.game_state.clone();

            primitives.add(
                "entity-collided",
                PrimitiveProcedureInfo::new_simple(1, Effect::Pure, move |mut args|
                {
                    let game_state = game_state.upgrade().unwrap();
                    let game_state = game_state.borrow();
                    let entities = game_state.entities();

                    let entity = Self::pop_entity(&mut args)?;
                    let collided = entities.collider(entity)
                        .map(|x| x.collided().to_vec()).into_iter().flatten()
                        .next();

                    if let Some(collided) = collided
                    {
                        Self::push_entity(args.memory, collided)
                    } else
                    {
                        Ok(().into())
                    }
                }));
        }

        {
            let game_state = self.game_state.clone();

            primitives.add(
                "all-entities-query",
                PrimitiveProcedureInfo::new_simple(0, Effect::Impure, move |args|
                {
                    let game_state = game_state.upgrade().unwrap();
                    let game_state = game_state.borrow();
                    let entities = game_state.entities();

                    let mut normal_entities = Vec::new();
                    let mut local_entities = Vec::new();

                    let mut total = 0;
                    entities.for_each_entity(|entity|
                    {
                        total += 1;
                        let id = LispValue::new_integer(entity.id() as i32);

                        if entity.local()
                        {
                            local_entities.push(id);
                        } else
                        {
                            normal_entities.push(id);
                        }
                    });

                    let memory = args.memory;

                    let restore = memory.with_saved_registers([Register::Value, Register::Temporary]);

                    memory.make_vector(Register::Temporary, normal_entities)?;
                    memory.make_vector(Register::Value, local_entities)?;

                    memory.cons(Register::Value, Register::Temporary, Register::Value)?;

                    memory.set_register(Register::Temporary, total - 1);

                    memory.cons(Register::Value, Register::Temporary, Register::Value)?;

                    let value = memory.get_register(Register::Value);

                    restore(memory)?;

                    Ok(value)
                }));
        }

        primitives.add(
            "query-entity-next",
            PrimitiveProcedureInfo::new_simple(1, Effect::Impure, move |mut args|
            {
                let query_arg = args.next().unwrap();
                let query = query_arg.as_list(args.memory)?;
                let query_id = query_arg.as_list_id()?;

                let index = query.car().as_integer()?;
                let entities = query.cdr().as_list(args.memory)?;

                let normal_entities = entities.car().as_vector_ref(args.memory)?;
                let local_entities = entities.cdr().as_vector_ref(args.memory)?;

                if index < 0
                {
                    return Ok(().into());
                }

                let index = index as usize;

                let entity = if index >= normal_entities.len()
                {
                    let index = index - normal_entities.len();

                    Entity::from_raw(true, local_entities[index].as_integer()? as usize)
                } else
                {
                    Entity::from_raw(false, normal_entities[index].as_integer()? as usize)
                };

                // set to next index
                args.memory.set_car(query_id, (index as i32 - 1).into());

                Self::push_entity(args.memory, entity)
            }));

        {
            let game_state = self.game_state.clone();

            primitives.add(
                "print-chunk-of",
                PrimitiveProcedureInfo::new_simple(1..=2, Effect::Impure, move |mut args|
                {
                    let game_state = game_state.upgrade().unwrap();
                    let game_state = game_state.borrow();

                    let entity = Self::pop_entity(&mut args)?;
                    let position = game_state.entities().transform(entity).unwrap().position;

                    let visual = args.next().map(|x| x.as_bool()).unwrap_or(Ok(false))?;

                    eprintln!(
                        "entity info: {}",
                        game_state.world.debug_chunk(position.into(), visual)
                    );

                    Ok(().into())
                }));
        }

        self.add_simple_setter(&mut primitives, "set-floating", |entities, entity, _memory, value|
        {
            let state = value.as_bool()?;

            get_component_mut!(physical_mut, entities, entity).set_floating(state);

            Ok(())
        });

        self.add_simple_setter(&mut primitives, "set-speed", |entities, entity, _memory, value|
        {
            let speed = value.as_float()?;

            get_component_mut!(anatomy_mut, entities, entity).set_speed(speed);

            Ok(())
        });

        self.add_simple_setter(&mut primitives, "set-ghost", |entities, entity, _memory, value|
        {
            let state = value.as_bool()?;

            get_component_mut!(collider_mut, entities, entity).ghost = state;

            Ok(())
        });

        self.add_simple_setter(&mut primitives, "set-position", |entities, entity, memory, value|
        {
            let mut list = value.as_list(memory);

            let mut next_float = ||
            {
                let current = list.clone()?;
                let value = current.car().as_float();

                list = current.cdr().as_list(memory);

                value
            };

            let position = Vector3::new(next_float()?, next_float()?, next_float()?);

            get_component_mut!(target, entities, entity).position = position;

            Ok(())
        });

        self.add_simple_setter(&mut primitives, "set-rotation", |entities, entity, _memory, value|
        {
            get_component_mut!(target, entities, entity).rotation = value.as_float()?;

            Ok(())
        });

        self.add_simple_setter(&mut primitives, "set-faction", |entities, entity, memory, value|
        {
            let faction = value.as_symbol(memory)?;
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

            get_component_mut!(character_mut, entities, entity).faction = faction;

            Ok(())
        });

        {
            let game_state = self.game_state.clone();

            primitives.add(
                "add-item",
                PrimitiveProcedureInfo::new_simple(2, Effect::Impure, move |mut args|
                {
                    let game_state = game_state.upgrade().unwrap();
                    let game_state = game_state.borrow_mut();
                    let entities = game_state.entities();

                    let entity = Self::pop_entity(&mut args)?;
                    let name = args.next().unwrap().as_symbol(args.memory)?.replace('_', " ");

                    let mut inventory = entities.inventory_mut(entity).unwrap();

                    let id = game_state.items_info.get_id(&name).ok_or_else(||
                    {
                        lisp::Error::Custom(format!("item named {name} doesnt exist"))
                    })?;

                    inventory.push(Item{id});

                    Ok(().into())
                }));
        }

        {
            let player_entity = self.info.borrow().entity;

            primitives.add(
                "player-entity",
                PrimitiveProcedureInfo::new_simple(0, Effect::Impure, move |args|
                {
                    Self::push_entity(args.memory, player_entity)
                }));
        }

        {
            let mouse_entity = self.info.borrow().mouse_entity;

            primitives.add(
                "mouse-entity",
                PrimitiveProcedureInfo::new_simple(0, Effect::Impure, move |args|
                {
                    Self::push_entity(args.memory, mouse_entity)
                }));
        }

        {
            let game_state = self.game_state.clone();

            primitives.add(
                "children-of",
                PrimitiveProcedureInfo::new_simple(1, Effect::Pure, move |mut args|
                {
                    let game_state = game_state.upgrade().unwrap();
                    let game_state = game_state.borrow();
                    let entities = game_state.entities();

                    let entity = Self::pop_entity(&mut args)?;

                    args.memory.cons_list_with(|memory|
                    {
                        let mut count = 0;
                        entities.children_of(entity).try_for_each(|x|
                        {
                            count += 1;
                            let value = Self::push_entity(memory, x)?;

                            memory.push_stack(value)?;

                            Ok(())
                        })?;

                        Ok(count)
                    })
                }));
        }

        {
            let game_state = self.game_state.clone();

            primitives.add(
                "position-entity",
                PrimitiveProcedureInfo::new_simple(1, Effect::Impure, move |mut args|
                {
                    let game_state = game_state.upgrade().unwrap();
                    let game_state = game_state.borrow();
                    let entities = game_state.entities();

                    let entity = Self::pop_entity(&mut args)?;

                    let position = entities.transform(entity).unwrap().position;

                    args.memory.cons_list([position.x, position.y, position.z])
                }));
        }

        {
            let game_state = self.game_state.clone();

            primitives.add(
                "print-component",
                PrimitiveProcedureInfo::new_simple(2, Effect::Impure, move |mut args|
                {
                    Self::maybe_print_component(&game_state, &mut args, true)
                }));
        }

        {
            let game_state = self.game_state.clone();

            primitives.add(
                "has-component",
                PrimitiveProcedureInfo::new_simple(2, Effect::Impure, move |mut args|
                {
                    Self::maybe_print_component(&game_state, &mut args, false)
                }));
        }

        {
            let game_state = self.game_state.clone();

            primitives.add(
                "print-entity-info",
                PrimitiveProcedureInfo::new_simple(1, Effect::Impure, move |mut args|
                {
                    let game_state = game_state.upgrade().unwrap();
                    let game_state = game_state.borrow();
                    let entities = game_state.entities();

                    let entity = Self::pop_entity(&mut args)?;

                    eprintln!("entity info: {}", entities.info_ref(entity));

                    Ok(().into())
                }));
        }

        {
            let mut infos: Vec<(_, _)> = primitives.iter_infos().collect();

            infos.sort_unstable_by_key(|x| x.0);

            let help_message = infos.into_iter().map(|(name, args)|
            {
                format!("{name} with {args} arguments")
            }).reduce(|acc, x|
            {
                acc + "\n" + &x
            }).unwrap_or_default();

            primitives.add(
                "help",
                PrimitiveProcedureInfo::new_simple(0, Effect::Impure, move |_args|
                {
                    println!("{help_message}");

                    Ok(().into())
                }));
        }

        Rc::new(primitives)
    }

    fn console_command(&mut self, command: String, output: ConsoleOutput)
    {
        let mut infos = self.info.borrow_mut();

        let config = LispConfig{
            type_checks: true,
            memory: LispMemory::new(infos.console.primitives.as_ref().unwrap().clone(), 2048, 1 << 14)
        };

        let source = infos.console.past_commands.clone() + &command;
        let mut lisp = match Lisp::new_with_config(config, &source)
        {
            Ok(x) => x,
            Err(err) =>
            {
                eprintln!("error parsing {command}: {err}");
                Lisp::print_highlighted(&source, err.position);
                return;
            }
        };

        let result = match lisp.run()
        {
            Ok(x) => x,
            Err(err) =>
            {
                eprintln!("error running {command}: {err}");
                Lisp::print_highlighted(&source, err.position);
                return;
            }
        };

        if !output.is_quiet()
        {
            eprintln!("ran command {command}, result: {result}");
        }

        let defined_this = result.into_memory().defined_values().unwrap().len();
        let changed_environment = defined_this > infos.console.standard_definitions;
        if changed_environment
        {
            infos.remember_command(defined_this, &command);
        }
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
    pub follow: Entity,
    pub entity: Entity,
    pub mouse_entity: Entity
}

struct ConsoleInfo
{
    contents: Option<String>,
    primitives: Option<Rc<Primitives>>,
    standard_definitions: usize,
    past_commands: String
}

impl ConsoleInfo
{
    pub fn new() -> Self
    {
        Self{
            contents: None,
            primitives: None,
            standard_definitions: 0,
            past_commands: String::new()
        }
    }
}

struct PlayerInfo
{
    camera: Entity,
    follow: Entity,
    entity: Entity,
    mouse_entity: Entity,
    other_entity: Option<Entity>,
    console: ConsoleInfo,
    previous_stamina: Option<f32>,
    previous_cooldown: (f32, f32),
    ctrl_held: bool,
    interacted: bool
}

impl PlayerInfo
{
    pub fn new(info: PlayerCreateInfo) -> Self
    {
        let console = ConsoleInfo::new();

        Self{
            camera: info.camera,
            follow: info.follow,
            entity: info.entity,
            mouse_entity: info.mouse_entity,
            other_entity: None,
            console,
            previous_stamina: None,
            previous_cooldown: (0.0, 0.0),
            ctrl_held: false,
            interacted: false
        }
    }

    pub fn remember_command(&mut self, definitions: usize, command: &str)
    {
        self.console.standard_definitions = definitions;
        self.console.past_commands += command;
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
        let mut player_transform = self.game_state.entities().target(self.info.entity).unwrap();
        let mut position = Vector3::repeat(CHUNK_VISUAL_SIZE / 2.0);
        position.z = -TILE_SIZE + (player_transform.scale.z / 2.0);

        player_transform.position = position;
        drop(player_transform);

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
            transform.position = entities.transform(self.info.follow).unwrap().position;
        }

        self.camera_sync();
    }

    fn camera_sync_z(&self)
    {
        let camera_z = self.game_state.entities().transform(self.info.camera).unwrap().position.z;

        // slighly shift the camera down so its not right at the tile height
        let shift = 0.001;
        let z = (camera_z / TILE_SIZE).ceil() * TILE_SIZE - shift;

        let mut camera = self.game_state.camera.write();
        camera.set_position_z(z);
        camera.update();
    }

    pub fn on_control(&mut self, state: ControlState, control: Control)
    {
        let is_floating = self.game_state.entities().physical(self.info.entity).map(|x|
        {
            x.floating()
        }).unwrap_or(false);

        match control
        {
            Control::Crawl if !is_floating =>
            {
                let entities = self.game_state.entities();
                if let Some(mut anatomy) = entities.anatomy_mut(self.info.entity)
                {
                    anatomy.override_crawling(state.to_bool());

                    drop(anatomy);
                    entities.anatomy_changed(self.info.entity);
                }
            },
            Control::Poke =>
            {
                self.character_action(CharacterAction::Poke{state: !state.to_bool()});
            },
            Control::Shoot =>
            {
                let mut target = some_or_return!(self.mouse_position());
                target.z = some_or_return!(self.player_position()).z;

                self.character_action(CharacterAction::Ranged{state: !state.to_bool(), target});
            },
            Control::Interact =>
            {
                self.info.interacted = state == ControlState::Pressed;
            },
            _ => ()
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
                        if let Some(other) = self.info.other_entity
                        {
                            self.game_state.ui.borrow_mut().close_inventory(other);
                        }

                        self.info.other_entity = Some(mouse_touched);

                        /*let id = self.game_state.open_inventory(WindowCreateInfo::Inventory{
                            spawn_position: self.game_state.ui_mouse_position(),
                            entity: mouse_touched,
                            on_click: Box::new(|_anchor, item|
                            {
                                UserEvent::UiAction(Rc::new(move |game_state|
                                {
                                    game_state.create_popup(vec![
                                        UserEvent::Take(item),
                                        UserEvent::Info{which: InventoryWhich::Other, item}
                                    ]);
                                }))
                            })
                        });

                        self.info.inventories.other = Some(id);*/todo!();

                        return;
                    }
                }

                self.character_action(CharacterAction::Bash);
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
        self.game_state.ui.borrow_mut().set_console(self.info.console.contents.clone());
    }

    fn handle_ui_event(&mut self, event: UiEvent)
    {
        match event
        {
            UiEvent::Action(action) =>
            {
                action(self.game_state);
            },
            UiEvent::Game(event) => self.handle_game_ui_event(event)
        }
    }

    fn handle_game_ui_event(&mut self, event: GameUiEvent)
    {
        let player = self.info.entity;

        // self.game_state.close_popup();
        todo!();

        match event
        {
            GameUiEvent::Info{which, item} =>
            {
                if let Some(item) = self.get_inventory(which)
                    .and_then(|inventory| inventory.get(item).cloned())
                {
                    /*self.game_state.add_window(WindowCreateInfo::ItemInfo{
                        spawn_position: self.game_state.ui_mouse_position(),
                        item
                    });*/todo!()
                } else
                {
                    eprintln!("tried to show info for an item that doesnt exist");
                }
            },
            GameUiEvent::Drop{which, item} =>
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
            GameUiEvent::Wield(item) =>
            {
                self.game_state.entities().character_mut(player).unwrap().set_holding(Some(item));
            },
            GameUiEvent::Take(item) =>
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
            self.handle_ui_event(event);
        });
    }

    fn toggle_inventory(&mut self)
    {
        let mut ui = self.game_state.ui.borrow_mut();
        let this = self.info.entity;

        if !ui.close_inventory(this)
        {
            ui.open_inventory(this, Box::new(|_, item|
            {
                dbg!(item);

                todo!()
            }));
        }
        /*if self.info.inventories.player.take().and_then(|window|
        {
            window.upgrade().map(|window| self.game_state.remove_window(window).is_ok())
        }).is_none()
        {
            let window = self.game_state.add_window(WindowCreateInfo::Inventory{
                spawn_position: self.game_state.ui_mouse_position(),
                entity: self.info.entity,
                on_click: Box::new(|_anchor, item|
                {
                    UserEvent::UiAction(Rc::new(move |game_state|
                    {
                        game_state.create_popup(vec![
                            UserEvent::Wield(item),
                            UserEvent::Drop{which: InventoryWhich::Player, item},
                            UserEvent::Info{which: InventoryWhich::Player, item}
                        ]);
                    }))
                })
            });

            self.info.inventories.player = Some(window);
        }*/
    }

    pub fn this_update(&mut self, dt: f32)
    {
        if !self.exists()
        {
            return;
        }

        self.update_user_events();

        let mouse_position = self.game_state.world_mouse_position();
        let mouse_position = Vector3::new(mouse_position.x, mouse_position.y, 0.0);
        let camera_position = self.game_state.camera.read().position().coords;

        {
            let entities = self.game_state.entities_mut();

            entities.transform_mut(self.info.mouse_entity).unwrap()
                .position = camera_position + mouse_position;

            entities.update_mouse_highlight(
                self.info.entity,
                self.info.mouse_entity
            );

            let player_position = entities.transform(self.info.entity).unwrap().position;

            let follow_position = if mouse_position.magnitude() > CHUNK_VISUAL_SIZE * 2.0
            {
                player_position
            } else
            {
                player_position + mouse_position / 5.0
            };

            entities.transform_mut(self.info.follow).unwrap().position = follow_position;
        }

        let entities = &mut self.game_state.entities.entities;
        if let Some((current_stamina, current_cooldown)) = entities.character(self.info.entity).map(|x|
        {
            (x.stamina_fraction(entities), x.attack_cooldown())
        })
        {
            let delay = 0.7;

            if self.info.previous_stamina != current_stamina
            {
                let was_none = self.info.previous_stamina.is_none();
                self.info.previous_stamina = current_stamina;

                if !was_none
                {
                    let stamina = current_stamina.unwrap_or(0.0);
                    dbg!("show stamina here");
                }
            }

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

                let fraction = if self.info.previous_cooldown.0 > 0.0
                {
                    current_cooldown / self.info.previous_cooldown.0
                } else
                {
                    0.0
                };

                dbg!("show weapon cooldown here");
            }
        }

        if let Some(movement) = self.movement_direction()
        {
            if let Some(mut character) = self.game_state.entities()
                .character_mut(self.info.entity)
            {
                character.sprinting = self.game_state.pressed(Control::Sprint);
            }

            self.walk(movement, dt);
        }

        let able_to_move = self.game_state.entities()
            .anatomy(self.info.entity)
            .map(|anatomy| anatomy.speed().is_some())
            .unwrap_or(false);

        if able_to_move
        {
            self.look_at_mouse();
        }

        if let Some(other_entity) = self.info.other_entity
        {
            if !self.game_state.entities().within_interactable_distance(self.info.entity, other_entity)
            {
                self.game_state.ui.borrow_mut().close_inventory(other_entity);
            }
        }

        let mut tile_info = None;
        self.colliding_info(|mut colliding|
        {
            let world = &self.game_state.world;

            let stairs: Option<TilePos> = world.tiles_inside(&colliding, |tile|
            {
                tile.map(|tile|
                {
                    world.tile_info(*tile).special == Some(SpecialTile::StairsUp)
                }).unwrap_or(false)
            }).next();

            let interact_button = ||
            {
                self.game_state.controls.key_for(&Control::Interact).map(ToString::to_string)
                    .unwrap_or_else(|| "unassigned".to_owned())
            };

            if let Some(stairs) = stairs
            {
                let above = stairs.offset(Pos3::new(0, 0, 1));

                if world.tile(above).and_then(|tile|
                {
                    if world.tile_info(*tile).special == Some(SpecialTile::StairsDown)
                    {
                        world.tile(above.offset(Pos3::new(0, 0, 1))).map(|tile|
                        {
                            !world.tile_info(*tile).colliding
                        })
                    } else
                    {
                        None
                    }
                }).unwrap_or(false)
                {
                    tile_info = Some(format!("press {} to go up", interact_button()));

                    if self.info.interacted
                    {
                        let mut transform = self.game_state.entities()
                            .target(self.info.entity)
                            .unwrap();

                        transform.position.z += TILE_SIZE * 2.0;
                    }
                }
            }

            colliding.transform.position.z -= TILE_SIZE;

            let stairs = world.tiles_inside(&colliding, |tile: Option<&_>|
            {
                tile.map(|tile|
                {
                    world.tile_info(*tile).special == Some(SpecialTile::StairsDown)
                }).unwrap_or(false)
            }).next();

            if stairs.and_then(|stairs: TilePos|
            {
                world.tile(stairs.offset(Pos3::new(0, 0, -1))).map(|tile|
                {
                    !world.tile_info(*tile).colliding
                })
            }).unwrap_or(false)
            {
                tile_info = Some(format!("press {} to go down", interact_button()));

                if self.info.interacted
                {
                    let mut transform = self.game_state.entities()
                        .target(self.info.entity)
                        .unwrap();

                    transform.position.z -= TILE_SIZE * 2.0;
                }
            }
        });

        if let Some(text) = tile_info
        {
            self.show_tile_tooltip(text);
        }

        self.game_state.sync_character(self.info.entity);

        self.info.interacted = false;
    }

    fn show_tile_tooltip(&mut self, text: String)
    {
    }

    fn colliding_info(&self, f: impl FnOnce(CollidingInfo))
    {
        let entities = self.game_state.entities();

        let transform = some_or_return!(entities.transform(self.info.entity)).clone();
        let mut collider = some_or_return!(entities.collider_mut(self.info.entity));

        f(CollidingInfo{
            entity: Some(self.info.entity),
            transform,
            collider: &mut collider
        });
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

        let is_flying = self.game_state.entities().physical(self.info.entity).map(|x|
        {
            x.floating()
        }).unwrap_or(false);

        let add_flight = |direction: Option<Vector3<f32>>|
        {
            if is_flying
            {
                let flight_speed = 0.1;

                let z = if self.game_state.pressed(Control::Jump)
                {
                    Some(flight_speed)
                } else if self.game_state.pressed(Control::Crawl)
                {
                    Some(-flight_speed)
                } else
                {
                    None
                };

                if let Some(z) = z
                {
                    let mut direction = direction.unwrap_or_default();
                    direction.z = z;

                    Some(direction)
                } else
                {
                    direction
                }
            } else
            {
                direction
            }
        };

        add_flight(movement_direction.and_then(|x|
        {
            Unit::try_new(x, 0.1).as_deref().copied()
        }))
    }

    pub fn walk(&mut self, direction: Vector3<f32>, dt: f32)
    {
        let entities = self.game_state.entities();
        if let Some(character) = entities.character(self.info.entity)
        {
            let anatomy = some_or_return!(entities.anatomy(self.info.entity));
            let mut physical = some_or_return!(entities.physical_mut(self.info.entity));

            let direction = some_or_return!(Unit::try_new(direction, 0.01));
            character.walk(&anatomy, &mut physical, direction, dt);
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

    fn player_position(&self) -> Option<Vector3<f32>>
    {
        self.game_state.entities()
            .transform(self.info.entity)
            .map(|x| x.position)
    }

    fn mouse_position(&self) -> Option<Vector3<f32>>
    {
        self.game_state.entities()
            .transform(self.info.mouse_entity)
            .map(|x| x.position)
    }
}
