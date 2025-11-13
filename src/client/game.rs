use std::{
    fs,
    f32,
    ops::ControlFlow,
    rc::{Rc, Weak},
    cell::{RefMut, RefCell}
};

use nalgebra::{Unit, Vector2, Vector3};

use yanyaengine::{
    Transform,
    KeyCode,
    game_object::*
};

use crate::{
    debug_config::*,
    common::{
        with_z,
        some_or_value,
        some_or_return,
        collider::*,
        character::*,
        watcher::*,
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
        lazy_transform::{Scaling, EaseInInfo},
        lisp::{self, *},
        systems::{collider_system, mouse_highlight_system, damaging_system::spawn_item},
        world::{CHUNK_VISUAL_SIZE, TILE_SIZE, Pos3, TilePos}
    }
};

use super::game_state::{
    GameState,
    NotificationInfo,
    NotificationKindInfo,
    InventoryWhich,
    UiEvent,
    GameUiEvent,
    ControlState,
    Control,
    ui::{NotificationDoor, NotificationIcon}
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

            let entities = game_state.entities_mut();
            let mouse_entity = entities.push_eager(true, EntityInfo{
                transform: Some(Transform{
                    scale: Vector3::new(TILE_SIZE * 0.1, TILE_SIZE * 0.1, TILE_SIZE * 5.0),
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
                entity: game_state.entities.player_entity,
                mouse_entity
            })
        };

        let mut this = Self{info: Rc::new(RefCell::new(info)), game_state};

        let primitives = this.console_primitives();

        {
            let load = |path: &str|
            {
                fs::read_to_string(path)
                    .unwrap_or_else(|err| panic!("{path} must exist ({err})"))
            };

            let mut infos = this.info.borrow_mut();
            infos.console.primitives = Some(primitives);
            infos.console.standard = load("lisp/standard.scm");
            infos.console.console_standard = load("lisp/console.scm");
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
        let info0 = self.info.clone();
        let info1 = self.info.clone();
        with_game_state(&self.game_state, move |game_state|
        {
            let ui = game_state.ui.clone();

            let entities = game_state.entities();
            entities.on_inventory(Box::new(move |OnChangeInfo{entity, ..}|
            {
                let info = info0.borrow();

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

            entities.on_remove(Box::new(move |_entities, entity|
            {
                let mut info = info1.borrow_mut();

                if Some(entity) == info.other_entity
                {
                    info.other_entity = None;
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
                [update] -> update_pre,
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
            crate::frame_time_this!{
                [update] -> ui_update,
                game_state.ui_update()
            }
        });

        self.player_container(|mut x|
        {
            controls.into_iter().for_each(|(control, state)|
            {
                x.on_control(state, control)
            });
        });

        with_game_state(&self.game_state, |game_state|
        {
            crate::frame_time_this!{
                [update] -> game_state_update,
                game_state.update(info, dt)
            };
        });

        self.player_container(|mut x|
        {
            if !x.game_state.is_loading()
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

    fn pop_entity(entities: &ClientEntities, args: &mut PrimitiveArgs) -> Result<Entity, lisp::Error>
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

        let entity = entities.with_seed(Entity::from_raw(local, id as usize).no_seed());

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

                    let entity = Self::pop_entity(entities, &mut args)?;

                    let value = args.next().unwrap();
                    let value = OutputWrapperRef::new(args.memory, value);

                    f(entities, entity, value)?;

                    Ok(().into())
                })
            }));
    }

    fn maybe_format_component(
        game_state: &Weak<RefCell<GameState>>,
        args: &mut PrimitiveArgs
    ) -> Result<LispValue, lisp::Error>
    {
        with_game_state(game_state, |game_state|
        {
            let entities = game_state.entities();

            let entity = Self::pop_entity(entities, args)?;
            let component = args.next().unwrap().as_symbol(args.memory)?;

            let maybe_info = entities.component_info(entity, &component);

            let value: LispValue = maybe_info.map(|x| args.memory.new_string(x)).unwrap_or(Ok(().into()))?;

            Ok(value)
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

                        let entity = Self::pop_entity(entities, &mut args)?;
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
                        entities.iter_entities().for_each(|entity|
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

                        let entity = Self::pop_entity(entities, &mut args)?;
                        let name = args.next().unwrap().as_symbol(args.memory)?.replace('_', " ");

                        let mut inventory = entities.inventory_mut(entity).unwrap();

                        let id = game_state.data_infos.items_info.get_id(&name).ok_or_else(||
                        {
                            lisp::Error::Custom(format!("item named {name} doesnt exist"))
                        })?;

                        let items_info = &game_state.data_infos.items_info;
                        inventory.push(items_info, Item::new(items_info, id));

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
            let camera_entity = self.info.borrow().camera;

            primitives.add(
                "camera-entity",
                PrimitiveProcedureInfo::new_simple(0, Effect::Impure, move |args|
                {
                    Self::push_entity(args.memory, camera_entity)
                }));
        }

        {
            let game_state = self.game_state.clone();
            let info = self.info.clone();

            primitives.add(
                "set-follow-target",
                PrimitiveProcedureInfo::new_simple(1, Effect::Impure, move |mut args|
                {
                    with_game_state(&game_state, |game_state|
                    {
                        let entity = Self::pop_entity(game_state.entities(), &mut args)?;

                        let mut info = info.borrow_mut();
                        PlayerContainer::new(&mut info, game_state).set_follow_target(entity);

                        Ok(().into())
                    })
                }));
        }

        {
            let camera = with_game_state(&self.game_state, |game_state|
            {
                game_state.camera.clone()
            });

            primitives.add(
                "set-camera-visual-position",
                PrimitiveProcedureInfo::new_simple(1, Effect::Impure, move |mut args|
                {
                    let value = args.next().unwrap();
                    let position = Self::parse_position(OutputWrapperRef::new(args.memory, value))?;

                    camera.write().set_position(position.into());

                    Ok(().into())
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

                        let entity = Self::pop_entity(entities, &mut args)?;

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

                        let entity = Self::pop_entity(entities, &mut args)?;

                        let position = entities.transform(entity).unwrap().position;

                        args.memory.cons_list([position.x, position.y, position.z])
                    })
                }));
        }

        {
            let game_state = self.game_state.clone();

            primitives.add(
                "rotation-entity",
                PrimitiveProcedureInfo::new_simple(1, Effect::Impure, move |mut args|
                {
                    with_game_state(&game_state, |game_state|
                    {
                        let entities = game_state.entities();

                        let entity = Self::pop_entity(entities, &mut args)?;

                        let rotation = entities.transform(entity).unwrap().rotation;

                        Ok(rotation.into())
                    })
                }));
        }

        {
            let game_state = self.game_state.clone();

            primitives.add(
                "format-component",
                PrimitiveProcedureInfo::new_simple(2, Effect::Impure, move |mut args|
                {
                    Self::maybe_format_component(&game_state, &mut args)
                }));
        }

        {
            let game_state = self.game_state.clone();

            primitives.add(
                "print-entity-info",
                PrimitiveProcedureInfo::new_simple(1..=2, Effect::Impure, move |mut args|
                {
                    with_game_state(&game_state, |game_state|
                    {
                        let entities = game_state.entities();

                        let entity = Self::pop_entity(entities, &mut args)?;
                        let is_compact = args.next().map(|x| x.as_bool()).unwrap_or(Ok(true))?;

                        let info = entities.info_ref(entity).map(|x|
                        {
                            if is_compact
                            {
                                x.compact_format()
                            } else
                            {
                                format!("{x:#?}")
                            }
                        }).unwrap_or_default();

                        eprintln!("entity info: {info}");

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
                        let message: DebugMessage = serde_json::from_str(&message).map_err(|err|
                        {
                            lisp::Error::Custom(format!("cant deserialize {message} as DebugMessage ({err})"))
                        })?;

                        game_state.send_message(Message::DebugMessage(message));

                        Ok(().into())
                    })
                }));

            use crate::debug_config::*;

            primitives.add(
                "set-debug-value",
                PrimitiveProcedureInfo::new_simple(1, Effect::Impure, move |mut args|
                {
                    DebugConfig::set_debug_value(args.next().unwrap());

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
        with_game_state(&self.game_state, |x| x.ui.borrow().get_console().cloned())
    }

    fn set_console_contents(&self, contents: Option<String>)
    {
        with_game_state(&self.game_state, |x| x.ui.borrow_mut().set_console(contents));
    }

    fn console_command(&mut self, command: String, output: ConsoleOutput)
    {
        fn code<'a>(info: &'a PlayerInfo, command: &'a str) -> [&'a str; 4]
        {
            let console = &info.console;
            [&console.standard, &console.console_standard, &console.past_commands, command]
        }

        let config = LispConfig{
            type_checks: true,
            memory: LispMemory::new(self.info.borrow().console.primitives.as_ref().unwrap().clone(), 2048, 1 << 16)
        };

        let lisp = {
            let info = self.info.borrow();
            let code = code(&info, &command);

            match Lisp::new_with_config(config, &code)
            {
                Ok(x) => x,
                Err(err) =>
                {
                    eprintln!("error parsing {command}: {err}");
                    Lisp::print_highlighted(&code, err.position);
                    return;
                }
            }
        };

        let result = match lisp.run()
        {
            Ok(x) => x,
            Err(err) =>
            {
                let info = self.info.borrow();
                let code = code(&info, &command);

                eprintln!("error running {command}: {err}");
                Lisp::print_highlighted(&code, err.position);
                return;
            }
        };

        if !output.is_quiet()
        {
            eprintln!("ran command {command}, result: {result}");
        }

        let defined_this = result.into_memory().defined_values().unwrap().len();
        let changed_environment = defined_this > self.info.borrow().console.standard_definitions;
        if changed_environment
        {
            self.info.borrow_mut().remember_command(defined_this, &command);
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
    pub entity: Entity,
    pub mouse_entity: Entity
}

struct ConsoleInfo
{
    primitives: Option<Rc<Primitives>>,
    standard_definitions: usize,
    standard: String,
    console_standard: String,
    past_commands: String
}

impl ConsoleInfo
{
    pub fn new() -> Self
    {
        Self{
            primitives: None,
            standard_definitions: 0,
            standard: String::new(),
            console_standard: String::new(),
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
    entity: Entity,
    mouse_entity: Entity,
    other_entity: Option<Entity>,
    console: ConsoleInfo,
    animation: Option<PlayerAnimation>,
    previous_oxygen: Option<f32>,
    previous_cooldown: (f32, f32),
    mouse_highlighted: Option<Entity>,
    interacted: bool
}

impl PlayerInfo
{
    pub fn new(info: PlayerCreateInfo) -> Self
    {
        let console = ConsoleInfo::new();

        Self{
            camera: info.camera,
            entity: info.entity,
            mouse_entity: info.mouse_entity,
            other_entity: None,
            console,
            animation: None,
            previous_oxygen: None,
            previous_cooldown: (0.0, 0.0),
            mouse_highlighted: None,
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
        let is_dead = self.game_state.entities().anatomy(self.info.entity).map(|x| x.is_dead()).unwrap_or(false);
        if is_dead
        {
            self.game_state.ui.borrow_mut().player_dead();
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
        if !self.update_camera_follow() { return; }

        self.game_state.entities().end_sync_full(self.info.camera);

        self.camera_sync();
    }

    fn update_camera_follow(&self) -> bool
    {
        let mouse_position = self.game_state.world_mouse_position();
        let mouse_position = Vector3::new(mouse_position.x, mouse_position.y, 0.0);

        let entities = self.game_state.entities();

        let entity_position = some_or_value!(entities.transform(self.game_state.entities.follow_target()), false).position;

        let follow_position = if mouse_position.magnitude() > CHUNK_VISUAL_SIZE * 2.0
        {
            entity_position
        } else
        {
            entity_position + mouse_position / 5.0
        };

        some_or_value!(entities.target(self.info.camera), false).position = follow_position;

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

        if (state.is_down() && is_animating) || self.game_state.is_paused()
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
                if let Some(mouse_touched) = self.info.mouse_highlighted
                {
                    if let Some(item) = self.game_state.entities().item(mouse_touched).map(|x| x.clone())
                    {
                        let entities = self.game_state.entities();

                        if let Some(mut inventory) = entities.inventory_mut(self.info.entity)
                        {
                            inventory.push(&self.game_state.data_infos.items_info, item);

                            if let Some(mut lazy) = entities.lazy_transform_mut(mouse_touched)
                            {
                                lazy.scaling = Scaling::EaseIn(EaseInInfo::new(0.015));

                                if let Some(transform) = entities.transform(self.info.entity)
                                {
                                    lazy.target_local.position = transform.position;
                                    lazy.target_local.scale = with_z(Vector2::zeros(), lazy.target_local.scale.z);

                                    entities.add_watcher(mouse_touched, Watcher{
                                        kind: WatcherType::Lifetime(1.0.into()),
                                        action: Box::new(|entities: &mut ClientEntities, entity|
                                        {
                                            entities.remove_deferred(entity);
                                        }),
                                        ..Default::default()
                                    });
                                }
                            }
                        }
                    } else
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
                    }

                    return;
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
                            },
                            Drug::BoneHeal{amount} =>
                            {
                                anatomy.bone_heal(*amount)
                            }
                        };

                        if consumed
                        {
                            if let Some(mut inventory) = self.get_inventory(which)
                            {
                                inventory.remove(&self.game_state.data_infos.items_info, item);
                                if let Some(mut character) = self.game_state.entities().character_mut(self.info.entity)
                                {
                                    character.on_removed_item(item);
                                }
                            }
                        }
                    }
                } else
                {
                    eprintln!("tried to use an item that doesnt exist");
                }
            },
            GameUiEvent::Drop{which, item} =>
            {
                if which != InventoryWhich::Player
                {
                    return;
                }

                if let Some(dropped_item) = self.get_inventory(which)
                    .and_then(|mut inventory| inventory.remove(&self.game_state.data_infos.items_info, item))
                {
                    if let Some(mut character) = self.game_state.entities().character_mut(self.info.entity)
                    {
                        character.on_removed_item(item);
                    }

                    if let Some(player_transform) = self.game_state.entities().transform(self.info.entity).as_ref()
                    {
                        spawn_item(
                            self.game_state.entities(),
                            &self.game_state.common_textures,
                            player_transform,
                            &dropped_item
                        );
                    }
                } else
                {
                    eprintln!("tried to drop item that doesnt exist");
                }
            },
            GameUiEvent::Wield(item) =>
            {
                if let Some(mut character) = self.game_state.entities().character_mut(player)
                {
                    character.set_holding(Some(item));
                }
            },
            GameUiEvent::Take(item) =>
            {
                if let Some(taken) = self.get_inventory(InventoryWhich::Other)
                    .and_then(|mut inventory| inventory.remove(&self.game_state.data_infos.items_info, item))
                {
                    if let Some(mut inventory) = self.game_state.entities().inventory_mut(self.info.entity)
                    {
                        inventory.push(&self.game_state.data_infos.items_info, taken);
                    }
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

    fn get_inventory(&self, which: InventoryWhich) -> Option<RefMut<'_, Inventory>>
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
                    GameUiEvent::Info{which: InventoryWhich::Player, item: item_id},
                    GameUiEvent::Drop{which: InventoryWhich::Player, item: item_id}
                ];

                if let Some(usage) = info.usage()
                {
                    actions.insert(1, GameUiEvent::Use{usage, which: InventoryWhich::Player, item: item_id});
                }

                actions
            }));
        }
    }

    fn set_follow_target(&mut self, entity: Entity)
    {
        self.game_state.entities.set_follow_target(entity);
    }

    pub fn this_update(&mut self, dt: f32) -> bool
    {
        if !self.exists() || self.game_state.is_paused()
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

            if let Some(mut transform) = entities.transform_mut(self.info.mouse_entity)
            {
                transform.position = camera_position + mouse_position;
            }

            let new_mouse_highlighted = mouse_highlight_system::update(
                entities,
                self.info.entity,
                self.info.mouse_entity
            );

            if let Some(previous_mouse_highlighted) = self.info.mouse_highlighted
            {
                if Some(previous_mouse_highlighted) != new_mouse_highlighted
                {
                    if let Some(mut render) = entities.render_mut_no_change(previous_mouse_highlighted)
                    {
                        render.outlined = false;
                    }
                }
            }

            self.info.mouse_highlighted = new_mouse_highlighted;
        }

        self.update_camera_follow();

        let entities = &mut self.game_state.entities.entities;
        if let Some((current_oxygen, current_cooldown)) = entities.character(self.info.entity).map(|x|
        {
            (x.oxygen_fraction(entities), x.attack_cooldown())
        })
        {
            if self.info.previous_oxygen != current_oxygen
            {
                let was_none = self.info.previous_oxygen.is_none();
                self.info.previous_oxygen = current_oxygen;

                if !was_none
                {
                    let oxygen = current_oxygen.unwrap_or(0.0);

                    self.game_state.ui.borrow_mut().set_oxygen(oxygen);
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

                self.game_state.cooldown_fraction = fraction;
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

        if able_to_move
        {
            self.update_tile_actions();
        }

        if self.info.animation.is_some()
        {
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
        }

        self.game_state.sync_character(self.info.entity);

        self.info.interacted = false;

        true
    }

    fn update_tile_actions(&mut self)
    {
        let interact_button = ||
        {
            self.game_state.controls.key_name(&Control::Interact)
        };

        let animation_duration = 0.7;

        let mut tile_info = None;
        let mut new_animation = None;

        (|| {
            let entities = self.game_state.entities();

            if let Some(collider) = entities.collider(self.info.entity)
            {
                if let Some(door_entity) = collider.collided().iter().find(|x| entities.door_exists(**x)).copied()
                {
                    let door = entities.door(door_entity).unwrap();
                    let new_state = !door.is_open();

                    if !new_state
                    {
                        let collider = some_or_return!(entities.collider(door_entity));

                        let door_blocked = collider.collided().iter().any(|x|
                        {
                            let check_collider = ColliderInfo{
                                kind: ColliderType::Rectangle,
                                layer: ColliderLayer::Door,
                                ghost: true,
                                ..Default::default()
                            }.into();

                            let check_collider = CollidingInfoRef{
                                entity: None,
                                transform: door.door_transform(),
                                collider: &check_collider
                            };

                            if DebugConfig::is_enabled(DebugTool::CollisionBounds)
                            {
                                collider_system::debug_collision_bounds(entities, &check_collider);
                            }

                            let other_collider = some_or_value!(entities.collider(*x), false);
                            let other = CollidingInfoRef::new(
                                some_or_value!(entities.transform(*x), false).clone(),
                                &other_collider
                            );

                            check_collider.collide_immutable(&other, |_| {})
                        });

                        if door_blocked
                        {
                            tile_info = Some((NotificationIcon::Door(NotificationDoor::Close(true)), interact_button()));
                            return;
                        }
                    }

                    drop(door);

                    if self.info.interacted
                    {
                        let mut door = entities.door_mut(door_entity).unwrap();

                        door.set_open(entities, door_entity, self.info.entity, new_state);
                    } else
                    {
                        let icon = if new_state { NotificationDoor::Open } else { NotificationDoor::Close(false) };
                        tile_info = Some((NotificationIcon::Door(icon), interact_button()));
                    }
                }
            }
        })();

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
                        tile_info = Some((NotificationIcon::GoUp, interact_button()));
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
                tile_info = Some((NotificationIcon::GoDown, interact_button()));

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
        }

        if self.info.animation.is_none()
        {
            if let Some((icon, text)) = tile_info
            {
                self.show_tile_tooltip(icon, text);
            }
        }
    }

    fn show_tile_tooltip(&mut self, icon: NotificationIcon, text: String)
    {
        let notification = NotificationInfo{
            owner: self.info.entity,
            lifetime: 0.1,
            kind: NotificationKindInfo::Text{icon, text},
            is_closed: false
        };

        self.game_state.ui.borrow_mut().show_notification(notification);
    }

    fn colliding_info(&self, f: impl FnOnce(CollidingInfoRef))
    {
        let entities = self.game_state.entities();

        let transform = some_or_return!(entities.transform(self.info.entity)).clone();
        let collider = some_or_return!(entities.collider(self.info.entity));

        f(CollidingInfoRef{
            entity: Some(self.info.entity),
            transform,
            collider: &collider
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

    #[allow(dead_code)]
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
