use std::{
    fs,
    f32,
    sync::Arc,
    ops::ControlFlow,
    rc::{Rc, Weak},
    cell::{RefMut, RefCell}
};

use nalgebra::{vector, Unit, Vector3};

use yanyaengine::{
    Transform,
    KeyCode,
    game_object::*
};

use crate::{
    debug_config::*,
    common::{
        with_z,
        with_error,
        some_or_value,
        some_or_return,
        some_or_unexpected_return,
        inventory_remove_item,
        angle_to_direction_3d,
        random_rotation,
        ENTITY_SCALE,
        collider::*,
        character::*,
        particle_creator::*,
        World,
        Damageable,
        SpecialTile,
        AnyEntities,
        Inventory,
        InventoryItem,
        Drug,
        Entity,
        EntityInfo,
        OnChangeInfo,
        ItemUsage,
        scripts_container::{
            parse_entity,
            push_entity,
            parse_position,
            add_info_primitives,
            ScriptsContainer,
            ScriptIndex
        },
        clothing::ClothingInfo,
        entity::ClientEntities,
        lisp::{self, *},
        systems::{collider_system, mouse_highlight_system, damaging_system::spawn_item},
        world::{CHUNK_VISUAL_SIZE, TILE_SIZE, Pos3, TilePos}
    }
};

use super::game_state::{
    DEFAULT_ZOOM,
    GameState,
    NotificationInfo,
    NotificationKindInfo,
    InventoryWhich,
    UiEvent,
    GameUiEvent,
    ControlState,
    Control,
    ui::{InventoryOpenInfo, NotificationDoor, NotificationIcon}
};


const FORCE_CRAWL_SPEED: f32 = 0.01;

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

enum OuterAction
{
    Use{script_index: ScriptIndex, entity: Entity, item: InventoryItem}
}

pub struct Game
{
    scripts: Rc<ScriptsContainer>,
    game_state: Weak<RefCell<GameState>>,
    info: Rc<RefCell<PlayerInfo>>
}

impl Game
{
    pub fn new(game_state: Weak<RefCell<GameState>>) -> Self
    {
        let (scripts, info) = {
            let game_state = game_state.upgrade().unwrap();
            let mut game_state = game_state.borrow_mut();

            let entities = game_state.entities_mut();
            let mouse_entity = entities.push_eager(true, EntityInfo{
                transform: Some(Transform{
                    scale: vector![TILE_SIZE * 0.1, TILE_SIZE * 0.1, TILE_SIZE * 5.0],
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

            let scripts = game_state.scripts.clone();

            let player_info = PlayerInfo::new(PlayerCreateInfo{
                camera: game_state.entities.camera_entity,
                entity: game_state.entities.player_entity,
                mouse_entity
            });

            (scripts, player_info)
        };

        let mut this = Self{scripts, info: Rc::new(RefCell::new(info)), game_state};

        let primitives = this.console_primitives();

        {
            let load = |path: &str|
            {
                fs::read_to_string(path)
                    .unwrap_or_else(|err| panic!("{path} must exist ({err})"))
            };

            let mut infos = this.info.borrow_mut();
            infos.console.primitives = Some(primitives);
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

    pub fn on_player_connected(&mut self, screen_size: [f32; 2])
    {
        let info0 = self.info.clone();
        with_game_state(&self.game_state, move |game_state|
        {
            let ui0 = game_state.ui.clone();
            let ui1 = game_state.ui.clone();

            let entities = game_state.entities();
            entities.on_inventory(Box::new(move |OnChangeInfo{entity, ..}|
            {
                ui0.borrow_mut().inventory_changed(entity);
            }));

            entities.on_character(Box::new(move |OnChangeInfo{entity, ..}|
            {
                ui1.borrow_mut().inventory_changed(entity);
            }));

            entities.on_remove(Box::new(move |_entities, entity|
            {
                let mut info = info0.borrow_mut();

                if Some(entity) == info.other_entity
                {
                    info.other_entity = None;
                }
            }));
        });

        self.player_container(|mut x|
        {
            x.on_player_connected(screen_size)
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
            x.this_update(info.partial.size, dt)
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

        {
            let mut outer_actions = Vec::new();

            self.player_container(|mut x|
            {
                controls.into_iter().for_each(|(control, state)|
                {
                    x.on_control(&mut outer_actions, state, control)
                });
            });

            outer_actions.into_iter().for_each(|action|
            {
                match action
                {
                    OuterAction::Use{script_index, entity, item} =>
                    {
                        let script = self.scripts.get(script_index);
                        if let Err(err) = script.run_with(|memory|
                        {
                            if let Some(entity) = with_error(push_entity(memory, entity))
                            {
                                with_error(memory.define("caller-entity", entity));
                            }

                            with_error(memory.define("caller-item-inventory-id", (item.as_raw() as i32).into()));
                        })
                        {
                            eprintln!("error running on_use (in {}): {err}", script.get_source(err.position.source));
                        }
                    }
                }
            });
        }

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

    fn maybe_format_component(
        game_state: &Weak<RefCell<GameState>>,
        args: &mut PrimitiveArgs
    ) -> Result<LispValue, lisp::Error>
    {
        with_game_state(game_state, |game_state|
        {
            let entities = game_state.entities();

            let entity = parse_entity(entities, args.next_value().unwrap())?;
            let component = args.next_value().unwrap().as_symbol()?;

            let maybe_info = entities.component_info(entity, &component);

            let value: LispValue = maybe_info.map(|x| args.memory.new_string(x)).unwrap_or(Ok(().into()))?;

            Ok(value)
        })
    }

    fn console_primitives(&mut self) -> Rc<Primitives>
    {
        let mut primitives = Primitives::default();

        add_info_primitives(&mut primitives, Rc::new(RefCell::new(self.game_state.clone())));

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

                push_entity(args.memory, entity)
            }));

        {
            let game_state = self.game_state.clone();

            primitives.add(
                "print-chunk-at",
                PrimitiveProcedureInfo::new_simple(1..=2, Effect::Impure, move |mut args|
                {
                    with_game_state(&game_state, |game_state|
                    {
                        let position = parse_position(args.next_value().unwrap())?;

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
            let player_entity = self.info.borrow().entity;

            primitives.add(
                "player-entity",
                PrimitiveProcedureInfo::new_simple(0, Effect::Impure, move |args|
                {
                    push_entity(args.memory, player_entity)
                }));
        }

        {
            let mouse_entity = self.info.borrow().mouse_entity;

            primitives.add(
                "mouse-entity",
                PrimitiveProcedureInfo::new_simple(0, Effect::Impure, move |args|
                {
                    push_entity(args.memory, mouse_entity)
                }));
        }

        {
            let camera_entity = self.info.borrow().camera;

            primitives.add(
                "camera-entity",
                PrimitiveProcedureInfo::new_simple(0, Effect::Impure, move |args|
                {
                    push_entity(args.memory, camera_entity)
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
                        let entity = parse_entity(game_state.entities(), args.next_value().unwrap())?;

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
                    let position = parse_position(OutputWrapperRef::new(args.memory, value))?;

                    camera.write().set_position(position.into());

                    Ok(().into())
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

                        let entity = parse_entity(entities, args.next_value().unwrap())?;
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
                        let message = args.next_value().unwrap().as_string()?;
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
        fn code<'a>(info: &'a PlayerInfo, command: &'a str) -> [&'a str; 3]
        {
            let console = &info.console;
            [&console.console_standard, &console.past_commands, command]
        }

        let config = LispConfig{
            compile_config: CompileConfig{type_checks: true, apply_known: true},
            memory: LispMemory::new(self.info.borrow().console.primitives.as_ref().unwrap().clone(), 2048, 1 << 16),
            ..Default::default()
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

                eprintln!("error running {command} (in {}): {err}", lisp.get_source(err.position.source));
                Lisp::print_highlighted(&code, err.position);
                return;
            }
        };

        if !output.is_quiet()
        {
            eprintln!("ran command {command}, result: {result}");
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
    console_standard: String,
    past_commands: String
}

impl ConsoleInfo
{
    pub fn new() -> Self
    {
        Self{
            primitives: None,
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
    queued_action: bool,
    crawling: bool,
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
            queued_action: false,
            crawling: false,
            interacted: false
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

    pub fn on_player_connected(&mut self, screen_size: [f32; 2])
    {
        if let Some(mut character) = self.game_state.entities().character_mut(self.info.entity)
        {
            character.initialized_buffered(BufferedActions{
                target: {
                    let f = self.ranged_target_function();
                    Arc::new(move |entities| f(entities).unwrap_or_else(|| { eprintln!("couldnt get buffered target"); Vector3::zeros() }))
                }
            });
        } else
        {
            eprintln!("player has no character, huh??");
        }

        let is_dead = self.game_state.entities().anatomy(self.info.entity).map(|x| x.is_dead()).unwrap_or(false);
        if is_dead
        {
            self.game_state.ui.borrow_mut().player_dead();
        }

        self.camera_sync_instant(screen_size);
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

    pub fn camera_sync_instant(&mut self, screen_size: [f32; 2])
    {
        if !self.update_camera_follow(screen_size) { return; }

        self.game_state.entities().end_sync_full(self.info.camera);

        self.camera_sync();
    }

    fn update_camera_follow(&self, screen_size: [f32; 2]) -> bool
    {
        let mouse_position = with_z(self.game_state.world_mouse_position(), 0.0);

        let entities = self.game_state.entities();

        let entity_position = some_or_value!(entities.transform(self.game_state.entities.follow_target()), false).position;

        let follow_position = if mouse_position.magnitude() > (CHUNK_VISUAL_SIZE * 2.0)
        {
            entity_position
        } else
        {
            entity_position + mouse_position * 0.13
        };

        let player = entities.player(self.info.entity);
        let screenshake = player.as_ref().map(|x| &x.screenshake);

        let screen_ratio = screen_size[0].max(screen_size[1]) / 1920.0;
        let camera_zoom = self.game_state.camera_scale() / DEFAULT_ZOOM / screen_ratio;

        let shake_strength = screenshake.map(|screenshake| screenshake.effective_shake()).unwrap_or(0.0) * camera_zoom;

        let shake_offset = with_z(angle_to_direction_3d(random_rotation()).xy() * shake_strength, 0.0);

        let kick_offset = with_z(screenshake.map(|screenshake| screenshake.offset() * camera_zoom).unwrap_or_default(), 0.0);

        let follow_position = follow_position + shake_offset + kick_offset;

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

    pub fn on_control(&mut self, outer_actions: &mut Vec<OuterAction>, state: ControlState, control: Control)
    {
        enum MainAction
        {
            Use,
            Bash,
            Poke,
            Shoot
        }

        let select_main_action = |this: &Self| -> MainAction
        {
            let entities = this.game_state.entities();

            let character = some_or_value!(entities.character(this.info.entity), MainAction::Bash);

            let item = some_or_value!(character.held_item_info(entities), MainAction::Bash);

            if item.on_use.is_some()
            {
                return MainAction::Use;
            }

            if item.ranged.is_some()
            {
                return MainAction::Shoot;
            }

            if item.prefer_poke
            {
                return MainAction::Poke;
            }

            MainAction::Bash
        };

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

        if state.is_up()
        {
            match control
            {
                Control::MainAction =>
                {
                    if self.info.queued_action
                    {
                        self.info.queued_action = false;

                        match select_main_action(self)
                        {
                            MainAction::Use => (),
                            MainAction::Bash => self.character_action(CharacterAction::Bash{state: true}),
                            MainAction::Poke => self.character_action(CharacterAction::Poke{state: true}),
                            MainAction::Shoot =>
                            {
                                let target = some_or_return!(self.ranged_target());

                                self.character_action(CharacterAction::Ranged{state: true, target})
                            }
                        }
                    }
                },
                Control::Bash =>
                {
                    if self.info.queued_action
                    {
                        self.info.queued_action = false;

                        self.character_action(CharacterAction::Bash{state: true});
                    }
                },
                Control::Poke =>
                {
                    if self.info.queued_action
                    {
                        self.info.queued_action = false;

                        self.character_action(CharacterAction::Poke{state: true});
                    }
                },
                Control::Shoot =>
                {
                    if self.info.queued_action
                    {
                        self.info.queued_action = false;

                        let target = some_or_return!(self.ranged_target());

                        self.character_action(CharacterAction::Ranged{state: true, target});
                    }
                },
                Control::Throw =>
                {
                    if self.info.queued_action
                    {
                        self.info.queued_action = false;

                        let target = some_or_return!(self.ranged_target());

                        self.character_action(CharacterAction::Throw{state: true, target});
                    }
                },
                _ => ()
            }
        }

        match control
        {
            Control::Crawl if !is_floating =>
            {
                self.info.crawling = state.to_bool();
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
                    if self.game_state.entities().item_exists(mouse_touched)
                    {
                        self.character_action(CharacterAction::PickupItem{item: mouse_touched});
                    } else
                    {
                        if let Some(other) = self.info.other_entity
                        {
                            self.game_state.ui.borrow_mut().close_inventory(other);
                        }

                        self.info.other_entity = Some(mouse_touched);

                        self.game_state.ui.borrow_mut().open_inventory(mouse_touched, Box::new(move |InventoryOpenInfo{
                            item_info: info,
                            ids,
                            ..
                        }|
                        {
                            let mut actions = Vec::new();

                            let id = ids[0];
                            let items_amount = ids.len();

                            actions.push(GameUiEvent::Take(ids));

                            if items_amount > 1
                            {
                                actions.push(GameUiEvent::TakeOne(id));
                            }

                            actions.push(GameUiEvent::Info{which: InventoryWhich::Other, item: id});

                            if let Some(usage) = info.usage().cloned()
                            {
                                actions.insert(1, GameUiEvent::Use{usage, which: InventoryWhich::Other, item: id});
                            }

                            actions
                        }));
                    }

                    return;
                }

                if is_animating
                {
                    return;
                }

                match select_main_action(self)
                {
                    MainAction::Use =>
                    {
                        let entities = self.game_state.entities();

                        let character = some_or_unexpected_return!(entities.character(self.info.entity));
                        let item = some_or_unexpected_return!(character.held_item_info(entities));

                        let script_index = some_or_unexpected_return!(item.on_use);

                        outer_actions.push(OuterAction::Use{
                            script_index,
                            entity: self.info.entity,
                            item: character.holding().expect("must be returned with the redundant check above")
                        });
                    },
                    MainAction::Bash => self.stance_action(AttackStance::Side, |state| CharacterAction::Bash{state}),
                    MainAction::Poke => self.stance_action(AttackStance::Forward, |state| CharacterAction::Poke{state}),
                    MainAction::Shoot =>
                    {
                        let target = some_or_return!(self.ranged_target());

                        self.stance_action(AttackStance::Forward, |state| CharacterAction::Ranged{state, target})
                    }
                }
            },
            Control::Bash =>
            {
                if is_animating
                {
                    return;
                }

                self.stance_action(AttackStance::Side, |state| CharacterAction::Bash{state});
            },
            Control::Poke =>
            {
                if is_animating
                {
                    return;
                }

                self.stance_action(AttackStance::Forward, |state| CharacterAction::Poke{state});
            },
            Control::Shoot =>
            {
                if is_animating
                {
                    return;
                }

                let target = some_or_return!(self.ranged_target());

                self.stance_action(AttackStance::Forward, |state| CharacterAction::Ranged{state, target});
            },
            Control::Throw =>
            {
                if is_animating
                {
                    return;
                }

                let target = some_or_return!(self.ranged_target());

                self.stance_action(AttackStance::Forward, |state| CharacterAction::Throw{state, target});
            },
            Control::Reload =>
            {
                if is_animating
                {
                    return;
                }

                self.character_action(CharacterAction::Reload{item: None});
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

    fn stance_action(&mut self, stance: AttackStance, action: impl Fn(bool) -> CharacterAction)
    {
        let mut character = some_or_return!(self.game_state.entities().character_mut(self.info.entity));

        if character.stance() == stance
        {
            drop(character);

            self.character_action(action(false));
            self.character_action(action(true));
        } else
        {
            character.set_stance(self.game_state.entities(), stance);

            drop(character);

            self.info.queued_action = true;

            self.character_action(action(false));
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

                    if let Some(usage) = info.usage.as_ref()
                    {
                        let mut anatomy = some_or_return!(self.game_state.entities().anatomy_mut(self.info.entity));

                        let heal_particles = ||
                        {
                            let info = ParticlesKind::Heal.create(&self.game_state.common_textures);
                            create_particles(
                                self.game_state.entities(),
                                info.info,
                                EntityInfo{
                                    transform: Some(Transform{
                                        position: some_or_return!(self.game_state.entities().transform(self.info.entity)).position,
                                        ..Default::default()
                                    }),
                                    ..info.prototype
                                },
                                Vector3::repeat(ENTITY_SCALE)
                            );
                        };

                        let consumed = match usage
                        {
                            ItemUsage::Drug(drug) =>
                            {
                                match drug
                                {
                                    Drug::Heal{amount} =>
                                    {
                                        let is_full = anatomy.is_full();
                                        if !is_full
                                        {
                                            heal_particles();
                                            anatomy.heal(*amount);
                                        }

                                        !is_full
                                    },
                                    Drug::BoneHeal{amount} =>
                                    {
                                        let consumed = anatomy.bone_heal(*amount);

                                        if consumed
                                        {
                                            heal_particles();
                                        }

                                        consumed
                                    }
                                }
                            },
                            ItemUsage::BoneHeal(amount) =>
                            {
                                let consumed = anatomy.bone_heal(*amount);

                                if consumed
                                {
                                    heal_particles();
                                }

                                consumed
                            }
                        };

                        if consumed
                        {
                            if let Some(entity) = self.get_inventory_entity(which)
                            {
                                inventory_remove_item(self.game_state.entities(), entity, item);
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

                if let Some(dropped_item) = self.get_inventory_entity(which)
                    .and_then(|entity| inventory_remove_item(self.game_state.entities(), entity, item))
                {
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
                    character.try_set_holding(self.game_state.entities(), Some(item));
                }
            },
            GameUiEvent::Unwield =>
            {
                if let Some(mut character) = self.game_state.entities().character_mut(player)
                {
                    character.unhold();
                }
            },
            GameUiEvent::Equip(item) =>
            {
                if let Some(mut character) = self.game_state.entities().character_mut(player)
                {
                    if let Some(inventory) = self.get_inventory(InventoryWhich::Player)
                    {
                        let item_info = self.game_state.data_infos.items_info.get(inventory[item].id);

                        if let Some(ClothingInfo{slot, ..}) = item_info.clothing
                        {
                            character.set_equip(slot, Some(item));
                        } else
                        {
                            eprintln!("tried to equip an item that cant be equipped");
                        }
                    }
                }
            },
            GameUiEvent::Unequip(slot) =>
            {
                if let Some(mut character) = self.game_state.entities().character_mut(player)
                {
                    character.set_equip(slot, None);
                }
            },
            GameUiEvent::Reload{item} =>
            {
                self.character_action(CharacterAction::Reload{item: Some(item)});
            },
            GameUiEvent::Take(_) | GameUiEvent::TakeOne(_) =>
            {
                let inventory_entity = some_or_return!(self.get_inventory_entity(InventoryWhich::Other));

                let take_item = |item|
                {
                    if let Some(taken) = inventory_remove_item(self.game_state.entities(), inventory_entity, item)
                    {
                        if let Some(mut inventory) = self.game_state.entities().inventory_mut(self.info.entity)
                        {
                            inventory.push(&self.game_state.data_infos.items_info, taken);
                        }
                    } else
                    {
                        eprintln!("tried to take item that doesnt exist");
                    }
                };

                match event
                {
                    GameUiEvent::Take(items) => items.into_iter().for_each(take_item),
                    GameUiEvent::TakeOne(item) => take_item(item),
                    _ => ()
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
            ui.open_inventory(this, Box::new(move |InventoryOpenInfo{item_info: info, ids, equip, ..}|
            {
                let id = ids[0];

                let mut actions = Vec::new();

                if equip == Some(EquipState::Held)
                {
                    actions.push(GameUiEvent::Unwield);
                } else if equip.is_none()
                {
                    actions.push(GameUiEvent::Wield(id));
                }

                if let Some(ClothingInfo{slot, ..}) = info.clothing
                {
                    if let Some(equip_state) = equip
                    {
                        if let EquipState::Equipped = equip_state
                        {
                            actions.push(GameUiEvent::Unequip(slot));
                        }
                    } else
                    {
                        actions.push(GameUiEvent::Equip(id));
                    }
                }

                if info.ammo.is_some()
                {
                    actions.push(GameUiEvent::Reload{item: id});
                }

                actions.push(GameUiEvent::Info{which: InventoryWhich::Player, item: id});
                actions.push(GameUiEvent::Drop{which: InventoryWhich::Player, item: id});

                if let Some(usage) = info.usage().cloned()
                {
                    actions.insert(1, GameUiEvent::Use{usage, which: InventoryWhich::Player, item: id});
                }

                actions
            }));
        }
    }

    fn set_follow_target(&mut self, entity: Entity)
    {
        self.game_state.entities.set_follow_target(entity);
    }

    pub fn this_update(&mut self, screen_size: [f32; 2], dt: f32) -> bool
    {
        if !self.exists() || self.game_state.is_paused()
        {
            return true;
        }

        if !self.update_user_events()
        {
            return false;
        }

        let mouse_position = with_z(self.game_state.world_mouse_position(), 0.0);

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

        self.update_camera_follow(screen_size);

        {
            let falloff_speed = 4.0;
            self.game_state.mouse_fraction.alpha = (self.game_state.mouse_fraction.alpha - falloff_speed * dt).max(0.0);
        }

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

                self.game_state.mouse_fraction.amount = fraction;

                if fraction != 0.0
                {
                    self.game_state.mouse_fraction.alpha = 1.0;
                }
            }
        }

        {
            let blood = self.game_state.entities().anatomy(self.info.entity).and_then(|anatomy|
            {
                anatomy.blood().fraction()
            }).unwrap_or(0.0);

            self.game_state.ui.borrow_mut().set_blood(blood);
        }

        let is_grounded = self.game_state.entities().physical(self.info.entity).as_ref()
            .map(|physical| physical.is_grounded() || physical.floating())
            .unwrap_or(false);

        if self.info.animation.is_none() && is_grounded
        {
            let movement_direction = self.movement_direction();

            if let Some(movement) = movement_direction
            {
                self.walk(movement, dt);
            }
        }


        let able_to_move;

        {
            let entities = self.game_state.entities();

            let this_entity = self.info.entity;
            let anatomy = entities.anatomy(this_entity);

            able_to_move = anatomy.as_ref().map(|anatomy| anatomy.speed() != 0.0).unwrap_or(false)
                && self.info.animation.is_none();

            if let Some(anatomy) = anatomy
            {
                let crawl_state = anatomy.speeds().legs <= FORCE_CRAWL_SPEED || self.info.crawling;

                let changed = anatomy.is_crawling() != crawl_state;
                if changed
                {
                    drop(anatomy);

                    entities.anatomy_mut(this_entity).unwrap().set_crawling(crawl_state);
                }
            }
        };

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

            let stairs: Option<TilePos> = World::tiles_inside(&colliding, |tile_pos|
            {
                world.tile(tile_pos).map(|tile|
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

            let stairs = World::tiles_inside(&colliding, |tile_pos|
            {
                world.tile(tile_pos).map(|tile|
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

    fn ranged_target_function(&self) -> impl Fn(&ClientEntities) -> Option<Vector3<f32>>
    {
        let mouse_entity = self.info.mouse_entity;
        let player_entity = self.info.entity;

        move |entities|
        {
            let mut target = entities.transform(mouse_entity)?.position;
            target.z = entities.transform(player_entity)?.position.z;

            Some(target)
        }
    }

    fn ranged_target(&self) -> Option<Vector3<f32>>
    {
        (self.ranged_target_function())(self.game_state.entities())
    }
}
