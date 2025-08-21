use std::{
    fs,
    f32,
    ops::ControlFlow,
    rc::{Rc, Weak},
    cell::{RefMut, RefCell}
};

use nalgebra::{Unit, Vector3};

use yanyaengine::{
    Transform,
    KeyCode,
    game_object::*
};

use crate::common::{
    some_or_value,
    some_or_return,
    collider::*,
    character::*,
    Damageable,
    SpecialTile,
    AnyEntities,
    Item,
    Inventory,
    Drug,
    Entity,
    EntityInfo,
    OnChangeInfo,
    entity::ClientEntities,
    lisp::{self, *},
    world::{CHUNK_VISUAL_SIZE, TILE_SIZE, Pos3, TilePos}
};

use super::game_state::{
    GameState,
    NotificationInfo,
    NotificationKindInfo,
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

fn with_game_state<T>(
    game_state: &Weak<RefCell<GameState>>,
    f: impl FnOnce(&mut GameState) -> T
) -> T
{
    let game_state = game_state.upgrade().unwrap();
    let mut game_state = game_state.borrow_mut();

    f(&mut game_state)
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

    #[allow(dead_code)]
    pub fn mouse_entity(&self) -> Entity
    {
        self.info.borrow().mouse_entity
    }

    fn player_container<T>(&mut self, f: impl FnOnce(PlayerContainer) -> T) -> T
    {
        let mut info = self.info.borrow_mut();

        with_game_state(&self.game_state, |game_state| f(PlayerContainer::new(&mut info, game_state)))
    }

    pub fn on_player_connected(&mut self)
    {
        let info = self.info.clone();
        with_game_state(&self.game_state, move |game_state|
        {
            let ui = game_state.ui.clone();

            game_state.entities_mut().on_inventory(Box::new(move |OnChangeInfo{entity, ..}|
            {
                let info = info.borrow_mut();

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
        });

        self.player_container(|mut x|
        {
            x.on_player_connected()
        })
    }

    pub fn update(
        &mut self,
        info: &mut UpdateBuffersInfo,
        dt: f32
    ) -> bool
    {
        with_game_state(&self.game_state, |game_state|
        {
            crate::frame_time_this!{
                update_pre,
                game_state.update_pre(dt)
            };
        });

        let keep_running = self.player_container(|mut x|
        {
            x.this_update(dt)
        });

        if !keep_running
        {
            return false;
        }

        let controls: Vec<_> = with_game_state(&self.game_state, |game_state|
        {
            let mut changed_this_frame = game_state.controls.changed_this_frame();

            crate::frame_time_this!{
                ui_update,
                game_state.ui_update(&mut changed_this_frame)
            };

            game_state.controls.consume_changed(changed_this_frame).collect()
        });

        self.player_container(|mut x|
        {
            controls.into_iter().for_each(|(control, state)| x.on_control(state, control));
        });

        with_game_state(&self.game_state, |game_state|
        {
            crate::frame_time_this!{
                game_state_update,
                game_state.update(info, dt)
            };
        });

        self.player_container(|mut x|
        {
            if !x.is_dead() && !x.game_state.is_loading()
            {
                x.camera_sync();
            }
        });

        true
    }

    fn pop_position(args: &mut PrimitiveArgs) -> Result<Vector3<f32>, lisp::Error>
    {
        let value = args.next().unwrap();

        Self::parse_position(OutputWrapperRef::new(args.memory, value))
    }

    fn parse_position(value: OutputWrapperRef) -> Result<Vector3<f32>, lisp::Error>
    {
        let mut list = value.as_list();

        let mut next_float = ||
        {
            let current = list.clone()?;
            let value = current.car().as_float();

            list = current.cdr().as_list();

            value
        };

        Ok(Vector3::new(next_float()?, next_float()?, next_float()?))
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
            OutputWrapperRef
        ) -> Result<(), lisp::Error> + 'static
    {
        let game_state = self.game_state.clone();

        primitives.add(
            name,
            PrimitiveProcedureInfo::new_simple(2, Effect::Impure, move |mut args|
            {
                with_game_state(&game_state, |game_state|
                {
                    let entities = game_state.entities_mut();

                    let entity = Self::pop_entity(&mut args)?;

                    let value = args.next().unwrap();
                    let value = OutputWrapperRef::new(args.memory, value);

                    f(entities, entity, value)?;

                    Ok(().into())
                })
            }));
    }

    fn maybe_print_component(
        game_state: &Weak<RefCell<GameState>>,
        args: &mut PrimitiveArgs,
        print: bool
    ) -> Result<LispValue, lisp::Error>
    {
        with_game_state(game_state, |game_state|
        {
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
        })
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
                    with_game_state(&game_state, |game_state|
                    {
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
                    })
                }));
        }

        {
            let game_state = self.game_state.clone();

            primitives.add(
                "all-entities-query",
                PrimitiveProcedureInfo::new_simple(0, Effect::Impure, move |args|
                {
                    with_game_state(&game_state, |game_state|
                    {
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
                    })
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
                "print-chunk-at",
                PrimitiveProcedureInfo::new_simple(1..=2, Effect::Impure, move |mut args|
                {
                    with_game_state(&game_state, |game_state|
                    {
                        let position = Self::pop_position(&mut args)?;

                        let visual = args.next().map(|x| x.as_bool()).unwrap_or(Ok(false))?;

                        eprintln!(
                            "entity info: {}",
                            game_state.world.debug_chunk(position.into(), visual)
                        );

                        Ok(().into())
                    })
                }));
        }

        {
            let game_state = self.game_state.clone();

            primitives.add(
                "debug-visual-overmap",
                PrimitiveProcedureInfo::new_simple(0, Effect::Impure, move |_args|
                {
                    with_game_state(&game_state, |game_state|
                    {
                        game_state.world.debug_visual_overmap();

                        Ok(().into())
                    })
                }));
        }

        self.add_simple_setter(&mut primitives, "set-floating", |entities, entity, value|
        {
            let state = value.as_bool()?;

            get_component_mut!(physical_mut, entities, entity).set_floating(state);

            Ok(())
        });

        self.add_simple_setter(&mut primitives, "set-speed", |entities, entity, value|
        {
            let speed = value.as_float()?;

            get_component_mut!(anatomy_mut, entities, entity).set_speed(speed);

            Ok(())
        });

        self.add_simple_setter(&mut primitives, "set-ghost", |entities, entity, value|
        {
            let state = value.as_bool()?;

            get_component_mut!(collider_mut, entities, entity).ghost = state;

            Ok(())
        });

        self.add_simple_setter(&mut primitives, "set-position", |entities, entity, value|
        {
            let position = Self::parse_position(value)?;

            get_component_mut!(target, entities, entity).position = position;

            Ok(())
        });

        self.add_simple_setter(&mut primitives, "set-rotation", |entities, entity, value|
        {
            get_component_mut!(target, entities, entity).rotation = value.as_float()?;

            Ok(())
        });

        self.add_simple_setter(&mut primitives, "set-faction", |entities, entity, value|
        {
            let faction = value.as_symbol()?;
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
                "set-time-speed",
                PrimitiveProcedureInfo::new_simple(1, Effect::Impure, move |mut args|
                {
                    with_game_state(&game_state, |game_state|
                    {
                        let speed = args.next().unwrap().as_float()?;
                        game_state.world.set_time_speed(speed as f64);

                        Ok(().into())
                    })
                }));
        }

        {
            let game_state = self.game_state.clone();

            primitives.add(
                "add-item",
                PrimitiveProcedureInfo::new_simple(2, Effect::Impure, move |mut args|
                {
                    with_game_state(&game_state, |game_state|
                    {
                        let entities = game_state.entities();

                        let entity = Self::pop_entity(&mut args)?;
                        let name = args.next().unwrap().as_symbol(args.memory)?.replace('_', " ");

                        let mut inventory = entities.inventory_mut(entity).unwrap();

                        let id = game_state.data_infos.items_info.get_id(&name).ok_or_else(||
                        {
                            lisp::Error::Custom(format!("item named {name} doesnt exist"))
                        })?;

                        inventory.push(Item::new(&game_state.data_infos.items_info, id));

                        Ok(().into())
                    })
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
                    with_game_state(&game_state, |game_state|
                    {
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
                    })
                }));
        }

        {
            let game_state = self.game_state.clone();

            primitives.add(
                "position-entity",
                PrimitiveProcedureInfo::new_simple(1, Effect::Impure, move |mut args|
                {
                    with_game_state(&game_state, |game_state|
                    {
                        let entities = game_state.entities();

                        let entity = Self::pop_entity(&mut args)?;

                        let position = entities.transform(entity).unwrap().position;

                        args.memory.cons_list([position.x, position.y, position.z])
                    })
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
                    with_game_state(&game_state, |game_state|
                    {
                        let entities = game_state.entities();

                        let entity = Self::pop_entity(&mut args)?;

                        eprintln!("entity info: {}", entities.info_ref(entity));

                        Ok(().into())
                    })
                }));
        }

        #[cfg(debug_assertions)]
        {
            use crate::common::message::{Message, DebugMessage};

            let game_state = self.game_state.clone();

            primitives.add(
                "send-debug-message",
                PrimitiveProcedureInfo::new_simple(1, Effect::Impure, move |mut args|
                {
                    with_game_state(&game_state, |game_state|
                    {
                        let message = args.next().unwrap().as_string(args.memory)?;
                        let message: DebugMessage = serde_json::from_str(&message).map_err(|_|
                        {
                            lisp::Error::Custom(format!("cant deserialize {message} as DebugMessage"))
                        })?;

                        game_state.send_message(Message::DebugMessage(message));

                        Ok(().into())
                    })
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

    pub fn on_key_state(&mut self, key: KeyCode, pressed: bool) -> bool
    {
        if pressed
        {
            self.on_key(key)
        } else
        {
            false
        }
    }

    fn on_key(&mut self, key: KeyCode) -> bool
    {
        if !with_game_state(&self.game_state, |game_state| game_state.debug_mode)
        {
            return false;
        }

        if let Some(contents) = self.get_console_contents()
        {
            match key
            {
                KeyCode::Enter =>
                {
                    self.console_command(contents, ConsoleOutput::Normal);

                    self.set_console_contents(None);

                    true
                },
                KeyCode::Escape =>
                {
                    self.set_console_contents(None);

                    true
                },
                _ => false
            }
        } else
        {
            if key == KeyCode::Backquote
            {
                let value = if self.get_console_contents().is_some()
                {
                    None
                } else
                {
                    Some(String::new())
                };

                let state = if value.is_some() { "opened" } else { "closed" };
                eprintln!("debug console {state}");

                self.set_console_contents(value);

                true
            } else
            {
                false
            }
        }
    }

    fn get_console_contents(&self) -> Option<String>
    {
        with_game_state(&self.game_state, |x| x.ui.borrow().get_console().clone())
    }

    fn set_console_contents(&self, contents: Option<String>)
    {
        with_game_state(&self.game_state, |x| x.ui.borrow_mut().set_console(contents));
    }

    fn console_command(&mut self, command: String, output: ConsoleOutput)
    {
        let mut info = self.info.borrow_mut();
        let console = &info.console;

        let config = LispConfig{
            type_checks: true,
            memory: LispMemory::new(console.primitives.as_ref().unwrap().clone(), 2048, 1 << 14)
        };

        let lisp = match Lisp::new_with_config(config, &[&console.past_commands, &command])
        {
            Ok(x) => x,
            Err(err) =>
            {
                eprintln!("error parsing {command}: {err}");
                Lisp::print_highlighted(&command, err.position);
                return;
            }
        };

        let result = match lisp.run()
        {
            Ok(x) => x,
            Err(err) =>
            {
                eprintln!("error running {command}: {err}");
                Lisp::print_highlighted(&command, err.position);
                return;
            }
        };

        if !output.is_quiet()
        {
            eprintln!("ran command {command}, result: {result}");
        }

        let defined_this = result.into_memory().defined_values().unwrap().len();
        let changed_environment = defined_this > console.standard_definitions;
        if changed_environment
        {
            info.remember_command(defined_this, &command);
        }
    }

    pub fn player_exists(&mut self) -> bool
    {
        self.player_container(|x| x.exists())
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
    primitives: Option<Rc<Primitives>>,
    standard_definitions: usize,
    past_commands: String
}

impl ConsoleInfo
{
    pub fn new() -> Self
    {
        Self{
            primitives: None,
            standard_definitions: 0,
            past_commands: String::new()
        }
    }
}

struct PlayerAnimation
{
    duration: f32,
    action: Option<(f32, Box<dyn FnOnce(&mut PlayerContainer)>)>
}

struct PlayerInfo
{
    camera: Entity,
    follow: Entity,
    entity: Entity,
    mouse_entity: Entity,
    other_entity: Option<Entity>,
    console: ConsoleInfo,
    animation: Option<PlayerAnimation>,
    previous_stamina: Option<f32>,
    previous_cooldown: (f32, f32),
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
            animation: None,
            previous_stamina: None,
            previous_cooldown: (0.0, 0.0),
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
        if !self.update_camera_follow() { return; }

        let entities = self.game_state.entities();

        {
            let mut transform = some_or_return!(self.game_state.entities().transform_mut(self.info.camera));

            transform.position = some_or_return!(entities.transform(self.info.follow)).position;
        }

        self.camera_sync();
    }

    fn update_camera_follow(&self) -> bool
    {
        let mouse_position = self.game_state.world_mouse_position();
        let mouse_position = Vector3::new(mouse_position.x, mouse_position.y, 0.0);

        let entities = self.game_state.entities();

        let player_position = some_or_value!(entities.transform(self.info.entity), false).position;

        let follow_position = if mouse_position.magnitude() > CHUNK_VISUAL_SIZE * 2.0
        {
            player_position
        } else
        {
            player_position + mouse_position / 5.0
        };

        some_or_value!(entities.transform_mut(self.info.follow), false).position = follow_position;

        true
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
        if control == Control::Pause && state.is_down()
        {
            self.game_state.pause();
        }

        let is_animating = self.info.animation.is_some();

        if state.is_down() && is_animating
        {
            return;
        }

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
                }
            },
            Control::Poke =>
            {
                if is_animating
                {
                    return;
                }

                self.character_action(CharacterAction::Poke{state: !state.to_bool()});
            },
            Control::Shoot =>
            {
                if is_animating
                {
                    return;
                }

                let mut target = some_or_return!(self.mouse_position());
                target.z = some_or_return!(self.player_position()).z;

                self.character_action(CharacterAction::Ranged{state: !state.to_bool(), target});
            },
            Control::Throw =>
            {
                if is_animating
                {
                    return;
                }

                let mouse_transform = self.game_state.entities()
                    .transform(self.info.mouse_entity)
                    .unwrap();

                self.character_action(CharacterAction::Throw{state: !state.to_bool(), target: mouse_transform.position});
            },
            Control::Interact =>
            {
                self.info.interacted = state.is_down();
            },
            Control::Sprint =>
            {
                if let Some(character) = self.game_state.entities().character_mut_no_change(self.info.entity).as_mut()
                {
                    character.set_sprinting(state.is_down());
                }
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

                        self.game_state.ui.borrow_mut().open_inventory(mouse_touched, Box::new(move |_item, info, item_id|
                        {
                            let mut actions = vec![
                                GameUiEvent::Take(item_id),
                                GameUiEvent::Info{which: InventoryWhich::Other, item: item_id}
                            ];

                            if let Some(usage) = info.usage()
                            {
                                actions.insert(1, GameUiEvent::Use{usage, which: InventoryWhich::Other, item: item_id});
                            }

                            actions
                        }));

                        return;
                    }
                }

                self.character_action(CharacterAction::Bash);
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

    fn handle_ui_event(&mut self, event: UiEvent)
    {
        match event
        {
            UiEvent::Restart => unreachable!(),
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

        match event
        {
            GameUiEvent::Info{which, item} =>
            {
                if let Some(item) = self.get_inventory(which)
                    .map(|inventory| inventory[item].clone())
                {
                    self.game_state.ui.borrow_mut().open_item_info(item);
                } else
                {
                    eprintln!("tried to show info for an item that doesnt exist");
                }
            },
            GameUiEvent::Use{which, item, ..} =>
            {
                if let Some(id) = self.get_inventory(which)
                    .map(|inventory| inventory[item].id)
                {
                    let info = self.game_state.data_infos.items_info.get(id);

                    if let Some(drug) = info.drug.as_ref()
                    {
                        let mut anatomy = some_or_return!(self.game_state.entities().anatomy_mut(self.info.entity));

                        let consumed = match drug
                        {
                            Drug::Heal{amount} =>
                            {
                                let is_full = anatomy.is_full();
                                if !is_full
                                {
                                    anatomy.heal(*amount);
                                }

                                !is_full
                            }
                        };

                        if consumed
                        {
                            self.get_inventory(which).unwrap().remove(item);
                        }
                    }
                } else
                {
                    eprintln!("tried to use an item that doesnt exist");
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

    fn update_user_events(&mut self) -> bool
    {
        let mut events = self.game_state.user_receiver.borrow_mut().consume();
        events.try_for_each(|event|
        {
            if let UiEvent::Restart = event
            {
                return ControlFlow::Break(());
            }

            self.handle_ui_event(event);

            ControlFlow::Continue(())
        }).is_continue()
    }

    fn toggle_inventory(&mut self)
    {
        let mut ui = self.game_state.ui.borrow_mut();
        let this = self.info.entity;

        if !ui.close_inventory(this)
        {
            ui.open_inventory(this, Box::new(move |_item, info, item_id|
            {
                let mut actions = vec![
                    GameUiEvent::Wield(item_id),
                    GameUiEvent::Drop{which: InventoryWhich::Player, item: item_id},
                    GameUiEvent::Info{which: InventoryWhich::Player, item: item_id}
                ];

                if let Some(usage) = info.usage()
                {
                    actions.insert(1, GameUiEvent::Use{usage, which: InventoryWhich::Player, item: item_id});
                }

                actions
            }));
        }
    }

    pub fn this_update(&mut self, dt: f32) -> bool
    {
        if !self.exists()
        {
            return true;
        }

        if !self.update_user_events()
        {
            return false;
        }

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
        }

        self.update_camera_follow();

        let entities = &mut self.game_state.entities.entities;
        if let Some((current_stamina, current_cooldown)) = entities.character(self.info.entity).map(|x|
        {
            (x.stamina_fraction(entities), x.attack_cooldown())
        })
        {
            if self.info.previous_stamina != current_stamina
            {
                let was_none = self.info.previous_stamina.is_none();
                self.info.previous_stamina = current_stamina;

                if !was_none
                {
                    let stamina = current_stamina.unwrap_or(0.0);

                    self.game_state.ui.borrow_mut().set_stamina(stamina);
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

                self.game_state.ui.borrow_mut().set_cooldown(fraction);
            }
        }

        if self.info.animation.is_none()
        {
            let movement_direction = self.movement_direction();

            if let Some(movement) = movement_direction
            {
                self.walk(movement, dt);
            }
        }

        let able_to_move = self.game_state.entities()
            .anatomy(self.info.entity)
            .map(|anatomy| anatomy.speed() != 0.0)
            .unwrap_or(false) && self.info.animation.is_none();

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

        let interact_button = ||
        {
            self.game_state.controls.key_name(&Control::Interact)
        };

        let animation_duration = 0.7;

        let mut tile_info = None;
        let mut new_animation = None;

        {
            let entities = self.game_state.entities();

            if let Some(collider) = entities.collider(self.info.entity)
            {
                if let Some(door_entity) = collider.collided().iter().find(|x| entities.door_exists(**x)).copied()
                {
                    if self.info.interacted
                    {
                        let mut door = entities.door_mut(door_entity).unwrap();

                        let new_state = !door.is_open();

                        door.set_open(entities, door_entity, self.info.entity, new_state);
                    } else
                    {
                        tile_info = Some(interact_button());
                    }
                }
            }
        }

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
                    if self.info.interacted
                    {
                        new_animation = Some(PlayerAnimation{
                            duration: animation_duration,
                            action: Some((animation_duration * 0.5, Box::new(|this|
                            {
                                let mut transform = this.game_state.entities()
                                    .target(this.info.entity)
                                    .unwrap();

                                transform.position.z += TILE_SIZE * 2.0;
                            })))
                        });
                    } else
                    {
                        tile_info = Some(interact_button());
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
                tile_info = Some(interact_button());

                if self.info.interacted
                {
                    new_animation = Some(PlayerAnimation{
                        duration: animation_duration,
                        action: Some((animation_duration * 0.5, Box::new(|this|
                        {
                            let mut transform = this.game_state.entities()
                                .target(this.info.entity)
                                .unwrap();

                            transform.position.z -= TILE_SIZE * 2.0;
                        })))
                    });
                }
            }
        });

        if new_animation.is_some()
        {
            self.info.animation = new_animation;
            if let Some(mut physical) = self.game_state.entities().physical_mut(self.info.entity)
            {
                physical.set_velocity_raw(Vector3::zeros());
            }
        }

        self.game_state.ui.borrow_mut().set_fade(self.info.animation.is_some());

        if self.info.animation.is_some()
        {
            {
                let animation = self.info.animation.as_mut().unwrap();
                animation.duration -= dt;

                if animation.action.is_some()
                {
                    let action = animation.action.as_mut().unwrap();
                    action.0 -= dt;

                    if action.0 <= 0.0
                    {
                        (animation.action.take().unwrap().1)(self);
                    }
                }
            }

            let animation = self.info.animation.as_mut().unwrap();
            if animation.duration <= 0.0
            {
                let animation = self.info.animation.take();
                debug_assert!(animation.unwrap().action.is_none());
            }
        } else
        {
            if let Some(text) = tile_info
            {
                self.show_tile_tooltip(text);
            }
        }

        self.game_state.sync_character(self.info.entity);

        self.info.interacted = false;

        true
    }

    fn show_tile_tooltip(&mut self, text: String)
    {
        let notification = NotificationInfo{
            owner: self.info.entity,
            lifetime: 0.1,
            kind: NotificationKindInfo::Text{text}
        };

        self.game_state.ui.borrow_mut().show_notification(notification);
    }

    fn colliding_info(&self, f: impl FnOnce(CollidingInfo))
    {
        let entities = self.game_state.entities();

        let transform = some_or_return!(entities.transform(self.info.entity)).clone();
        let mut collider = some_or_return!(entities.collider_mut_no_change(self.info.entity));

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
        if let Some(mut character) = entities.character_mut_no_change(self.info.entity)
        {
            let anatomy = some_or_return!(entities.anatomy(self.info.entity));
            let mut physical = some_or_return!(entities.physical_mut_no_change(self.info.entity));

            let direction = some_or_return!(Unit::try_new(direction, 0.01));
            character.walk(&anatomy, &mut physical, direction, dt);
        }
    }

    pub fn look_at_mouse(&mut self)
    {
        let entities = self.game_state.entities();
        if let Some(transform) = entities.transform(self.info.mouse_entity)
        {
            if let Some(mut character) = entities.character_mut_no_change(self.info.entity)
            {
                character.look_at(entities, self.info.entity, transform.position.xy());
            }
        }
    }

    fn is_dead(&self) -> bool
    {
        self.game_state.entities().anatomy(self.info.entity)
            .map(|anatomy| anatomy.is_dead())
            .unwrap_or(true)
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
