use std::{
    fs,
    rc::{Weak, Rc},
    cell::RefCell,
    thread::{self, JoinHandle},
    sync::{mpsc, Arc},
    collections::{VecDeque, HashMap}
};

use nalgebra::{vector, Vector2};

use strum::IntoEnumIterator;

use vulkano::{
    device::Device,
    image::view::ImageView,
    command_buffer::BlitImageInfo
};

#[cfg(debug_assertions)]
use vulkano::{
    DeviceSize,
    sync::PipelineStage,
    query::{
        QueryResultFlags,
        QueryPool,
        QueryPoolCreateInfo,
        QueryType
    }
};

use yanyaengine::{
    EngineEvent,
    YanyaApp,
    Control,
    ShaderId,
    PhysicalKey,
    KeyCode,
    ElementState,
    game_object::*
};

use crate::{
    LONGEST_FRAME,
    debug_config::*,
    main_menu::{OnlineMode, MainMenu, MenuAction},
    server::Server,
    client::{
        Client,
        ClientInfo,
        ClientInitInfo,
        SlicedTexture,
        PartCreator
    },
    common::{
        ENTITY_SCALE,
        ENTITY_PIXEL_SCALE,
        TileMap,
        TileMapWithTextures,
        DataInfos,
        ItemsInfo,
        EnemiesInfo,
        FurnituresInfo,
        CharactersInfo,
        Crafts,
        loot::*,
        lisp::*,
        scripts_container::server_info_primitives,
        tilemap::TileLoot,
        furnitures_info::FurnitureLoot,
        enemies_info::EnemyLoot,
        items_info::{ScriptsContainer, TextureCreator},
        door::{door_scale, door_texture, DoorMaterial},
        sender_loop::{waiting_loop, DELTA_TIME}
    }
};


#[allow(dead_code)]
const TIMESTAMPS_COUNT: u32 = 9;

#[derive(Clone)]
pub struct TimestampQuery
{
    #[cfg(debug_assertions)]
    pub period: f32,
    #[cfg(debug_assertions)]
    pub query_pool: Arc<QueryPool>
}

impl From<&Arc<Device>> for TimestampQuery
{
    #[allow(unused_variables)]
    fn from(device: &Arc<Device>) -> Self
    {
        #[cfg(debug_assertions)]
        {
            let period = device.physical_device().properties().timestamp_period;

            let query_pool = QueryPool::new(
                device.clone(),
                QueryPoolCreateInfo{
                    query_count: TIMESTAMPS_COUNT,
                    ..QueryPoolCreateInfo::query_type(QueryType::Timestamp)
                }
            ).unwrap();

            Self{period, query_pool}
        }

        #[cfg(not(debug_assertions))]
        {
            Self{}
        }
    }
}

impl TimestampQuery
{
    #[allow(unused_variables)]
    pub fn setup(&self, info: &mut ObjectCreateInfo)
    {
        #[cfg(debug_assertions)]
        {
            let builder = info.partial.builder_wrapper.builder();

            unsafe{
                builder.reset_query_pool(
                    self.query_pool.clone(),
                    0..TIMESTAMPS_COUNT
                ).unwrap();
            }
        }
    }

    #[allow(unused_variables)]
    pub fn start(&self, info: &mut DrawInfo, index: u32)
    {
        #[cfg(debug_assertions)]
        {
            if index >= TIMESTAMPS_COUNT
            {
                panic!("tried to start a timestamp with an index above the length")
            }

            let builder = info.object_info.builder_wrapper.builder();

            unsafe{
                builder.write_timestamp(
                    self.query_pool.clone(),
                    index,
                    PipelineStage::TopOfPipe
                ).unwrap();
            }
        }
    }

    #[allow(unused_variables)]
    pub fn end(&self, info: &mut DrawInfo, index: u32)
    {
        #[cfg(debug_assertions)]
        {
            let builder = info.object_info.builder_wrapper.builder();

            unsafe{
                builder.write_timestamp(
                    self.query_pool.clone(),
                    index,
                    PipelineStage::BottomOfPipe
                ).unwrap();
            }
        }
    }

    pub fn get_results(&self) -> Vec<Option<u64>>
    {
        #[cfg(debug_assertions)]
        {
            let flags = QueryResultFlags::WITH_AVAILABILITY;

            let count = self.query_pool.result_len(flags) * TIMESTAMPS_COUNT as DeviceSize;
            let mut buffer = vec![0; count as usize];

            self.query_pool.get_results(0..TIMESTAMPS_COUNT, &mut buffer, flags).unwrap();

            (0..TIMESTAMPS_COUNT as usize).map(|index|
            {
                (buffer[index * 2 + 1] != 0).then(||
                {
                    buffer[index * 2]
                })
            }).collect()
        }

        #[cfg(not(debug_assertions))]
        {
            unreachable!()
        }
    }
}

#[derive(Clone)]
pub struct ProgramShaders
{
    pub default: ShaderId,
    pub character: ShaderId,
    pub above_world: ShaderId,
    pub default_full_lit: ShaderId,
    pub default_shaded: ShaderId,
    pub world: ShaderId,
    pub world_shaded: ShaderId,
    pub shadow: ShaderId,
    pub sky_shadow: ShaderId,
    pub sky_lighting: ShaderId,
    pub occluder: ShaderId,
    pub light_shadow: ShaderId,
    pub lighting: ShaderId,
    pub clear_alpha: ShaderId,
    pub menu_background: ShaderId,
    pub ui: ShaderId,
    pub ui_fill: ShaderId,
    pub mouse: ShaderId,
    pub final_mix: ShaderId
}

pub struct AppInfo
{
    pub shaders: ProgramShaders
}

type SlowMode = <DebugConfig as DebugConfigTrait>::SlowMode;

pub trait SlowModeStateTrait
{
    fn input(&mut self, control: Control);

    fn running(&self) -> bool;
    fn run_frame(&mut self) -> bool;
}

pub trait SlowModeTrait
{
    type State: SlowModeStateTrait + Default;

    fn as_bool() -> bool;
}

pub struct SlowModeTrue;
pub struct SlowModeFalse;

pub struct SlowModeState
{
    running: bool,
    step_now: bool
}

impl SlowModeStateTrait for SlowModeState
{
    fn input(&mut self, control: Control)
    {
        match control
        {
            Control::Keyboard{keycode: PhysicalKey::Code(code), state: ElementState::Pressed, ..} =>
            {
                match code
                {
                    KeyCode::KeyM =>
                    {
                        self.running = !self.running;
                        eprintln!("slow mode is {}", if self.running { "off" } else { "on" });
                    },
                    KeyCode::KeyN => self.step_now = true,
                    _ => ()
                }
            },
            _ => ()
        }
    }

    fn running(&self) -> bool
    {
        self.running
    }

    fn run_frame(&mut self) -> bool
    {
        let run_this = self.running || self.step_now;

        self.step_now = false;

        run_this
    }
}

impl SlowModeStateTrait for ()
{
    fn input(&mut self, _control: Control) {}
    fn running(&self) -> bool { false }
    fn run_frame(&mut self) -> bool { false }
}

impl Default for SlowModeState
{
    fn default() -> Self
    {
        Self{
            running: true,
            step_now: false
        }
    }
}

impl SlowModeTrait for SlowModeTrue
{
    type State = SlowModeState;

    fn as_bool() -> bool { true }
}

impl SlowModeTrait for SlowModeFalse
{
    type State = ();

    fn as_bool() -> bool { false }
}

enum Scene
{
    Game,
    Menu(Box<MainMenu>)
}

pub struct App
{
    client: Client,
    scene: Scene,
    data_infos: DataInfos,
    tilemap: TileMapWithTextures,
    server_scripts_info: ServerScriptsInfo,
    engine_events: VecDeque<EngineEvent>,
    server_handle: Option<JoinHandle<()>>,
    slow_mode: <SlowMode as SlowModeTrait>::State
}

impl Drop for App
{
    fn drop(&mut self)
    {
        self.client.exit();

        if let Some(handle) = self.server_handle.take()
        {
            handle.join().unwrap();
        }

        eprintln!("application closed properly");
    }
}

impl YanyaApp for App
{
    type SetupInfo = TimestampQuery;
    type AppInfo = Option<AppInfo>;

    fn init(mut partial_info: InitPartialInfo<Self::SetupInfo>, app_info: Self::AppInfo) -> Self
    {
        let app_info = app_info.unwrap();

        let mut scripts = ScriptsContainer::new();

        let items_info = {
            let mut assets = partial_info.object_info.assets.lock();
            let builder_wrapper = &mut partial_info.object_info.builder_wrapper;

            Arc::new(ItemsInfo::parse(
                TextureCreator{
                    builder_wrapper,
                    assets: &mut assets
                },
                &mut scripts,
                "items".into(),
                "items/items.json".into()
            ))
        };

        let mut characters_info = CharactersInfo::new();

        let mut server_enemy_scripts_info: Vec<EnemyScriptsInfo<Option<ServerScriptSingleInfo>>> = Vec::new();

        let enemies_info = EnemiesInfo::parse(
            EnemyLoot{
                server: &mut server_enemy_scripts_info
            },
            &partial_info.object_info.assets.lock(),
            &mut characters_info,
            &items_info,
            "enemy".into(),
            "info/enemies.json".into()
        );

        let player_character = enemies_info.get(enemies_info.get_id("me").unwrap_or_else(||
        {
            panic!("enemy named `me` is required, cant get player character id")
        })).character;

        let mut server_furniture_loot_info: Vec<ServerFurnitureLootInfo<Option<ServerScriptSingleInfo>>> = Vec::new();
        let mut client_furniture_loot_info = Vec::new();

        let furnitures_info = FurnituresInfo::parse(
            FurnitureLoot{
                server: &mut server_furniture_loot_info,
                client: &mut client_furniture_loot_info
            },
            &partial_info.object_info.assets.lock(),
            "furniture".into(),
            "info/furnitures.json".into()
        );

        let server_scripts_info = ServerScriptsInfo{
            furniture: server_furniture_loot_info,
            enemy: server_enemy_scripts_info
        };

        let crafts = Crafts::parse(&items_info, "info/crafts.json".into());

        let data_infos = DataInfos{
            items_info,
            enemies_info: Arc::new(enemies_info),
            furnitures_info: Arc::new(furnitures_info),
            characters_info: Arc::new(characters_info),
            crafts: Arc::new(crafts),
            player_character
        };

        let mut tile_loot_info = Vec::new();

        let tilemap = TileMap::parse(
            TileLoot{
                client: &mut tile_loot_info
            },
            "info/tiles.json",
            "textures/tiles/"
        ).unwrap();

        let client_loot = ClientScripts{
            furniture: client_furniture_loot_info,
            tile: tile_loot_info,
            empty: TileLootInfo::default(),
            door: DoorGenerator::new("lisp/door_loot.scm")
        };

        let sliced_textures = {
            let mut assets = partial_info.object_info.assets.lock();

            let mut part_creator = PartCreator{
                assets: &mut assets,
                resource_uploader: partial_info.object_info.builder_wrapper.resource_uploader_mut()
            };

            let textures: HashMap<String, SlicedTexture> = fs::read_dir("textures/special/sliced/").map(|dir_iter|
            {
                dir_iter.filter_map(|path|
                {
                    match path
                    {
                        Ok(path) =>
                        {
                            let path = path.path();
                            SlicedTexture::new(&mut part_creator, &path)
                        },
                        Err(err) =>
                        {
                            eprintln!("error opening sliced texture file: {err}");
                            None
                        }
                    }
                }).collect()
            }).unwrap_or_default();

            Rc::new(textures)
        };

        let init_info = ClientInitInfo{
            app_info,
            sliced_textures,
            tilemap: tilemap.clone(),
            data_infos: data_infos.clone(),
            scripts,
            client_scripts: client_loot
        };

        DebugConfig::on_start();

        let scene = Scene::Menu(Box::new(MainMenu::new(
            &partial_info.object_info,
            init_info.app_info.shaders.clone(),
            init_info.sliced_textures.clone()
        )));

        let client = Client::new(partial_info, init_info).unwrap();

        Self{
            client,
            scene,
            data_infos,
            tilemap,
            server_scripts_info,
            engine_events: VecDeque::new(),
            server_handle: None,
            slow_mode: Default::default()
        }
    }

    fn take_engine_event(&mut self) -> Option<EngineEvent> { self.engine_events.pop_front() }

    fn update(&mut self, partial_info: UpdateBuffersPartialInfo, dt: f32)
    {
        let dt = dt.min(LONGEST_FRAME as f32);

        match &mut self.scene
        {
            Scene::Game =>
            {
                let mut info = partial_info.to_full(&self.client.camera.read());

                self.update_game(&mut info, dt);
            },
            Scene::Menu(x) =>
            {
                let (partial_info, action) = if DebugConfig::is_enabled(DebugTool::SkipMenu)
                {
                    (partial_info, MenuAction::Start)
                } else
                {
                    let values = if SlowMode::as_bool()
                    {
                        if self.slow_mode.running()
                        {
                            Some((x.update(dt), dt))
                        } else if self.slow_mode.run_frame()
                        {
                            let dt = 1.0 / 60.0;
                            Some((x.update(dt), dt))
                        } else
                        {
                            None
                        }
                    } else
                    {
                        Some((x.update(dt), dt))
                    };

                    let partial_info = x.update_buffers(partial_info, values.as_ref().map(|x| x.1));

                    if let Some((action, _)) = values
                    {
                        (partial_info, action)
                    } else
                    {
                        return;
                    }
                };

                match action
                {
                    MenuAction::None => (),
                    MenuAction::Rebind(control, key) =>
                    {
                        x.rebind(control, key);
                    },
                    MenuAction::SetFrameLimit(limit) =>
                    {
                        self.engine_events.push_back(EngineEvent::SetPresentMode(limit.as_present_mode()));
                    },
                    MenuAction::Quit => self.exit(),
                    MenuAction::Start =>
                    {
                        let client_info = x.info.clone();

                        let (tx, rx) = mpsc::channel();

                        let listen_outside = client_info.online_mode == OnlineMode::Host;

                        let tilemap = self.tilemap.clone();
                        let server_loot = self.server_scripts_info.clone();
                        let data_infos = self.data_infos.clone();

                        let world_name = client_info.name.world_name();

                        let address = if client_info.online_mode != OnlineMode::Client
                        {
                            self.server_handle = thread::Builder::new().name("stephy_server".to_owned()).spawn(move ||
                            {
                                let port = 0;

                                let listen_address = format!("{}:{port}", if listen_outside { "0.0.0.0" } else { "127.0.0.1" });

                                let tilemap = Rc::new(tilemap.tilemap);

                                let server_entities = Rc::new(RefCell::new(Weak::new()));
                                let server_world = Rc::new(RefCell::new(Weak::new()));
                                let server_message_sender = Rc::new(RefCell::new(None));

                                let server_loot = {
                                    let enemy_primitives = Rc::new(server_info_primitives(
                                        tilemap.clone(),
                                        server_world.clone(),
                                        server_entities.clone(),
                                        data_infos.clone(),
                                        server_message_sender.clone()
                                    ));

                                    let c = |s: Option<ServerScriptSingleInfo>| -> Generator
                                    {
                                        s.map(|s| loot_compile(s.name, &s.code)).unwrap_or_default()
                                    };

                                    ServerScripts{
                                        furniture: server_loot.furniture.into_iter().map(|x| x.map(c)).collect(),
                                        enemy: server_loot.enemy.into_iter().map(|x| -> EnemyScriptsInfo<Generator>
                                        {
                                            let on_create = x.on_create.map(|s|
                                            {
                                                let memory = LispMemory::new(enemy_primitives.clone(), 128, 1 << 10);

                                                let config = LispConfig{
                                                    memory,
                                                    env_variables: vec!["caller-transform".to_owned()],
                                                    ..Default::default()
                                                };

                                                let lisp = match Lisp::new_with_config(config, &[&s.code])
                                                {
                                                    Ok(mut lisp) =>
                                                    {
                                                        lisp.set_source_name(0, s.name.clone());

                                                        Some(lisp)
                                                    },
                                                    Err(err) =>
                                                    {
                                                        eprintln!("error parsing on_use for enemy `{}`: {err}", &s.name);

                                                        None
                                                    }
                                                };

                                                Generator::new_raw(lisp)
                                            }).unwrap_or_default();

                                            EnemyScriptsInfo{
                                                on_create,
                                                on_contents: c(x.on_contents),
                                                on_equip: c(x.on_equip)
                                            }
                                        }).collect()
                                    }
                                };

                                let x = Server::new(
                                    tilemap,
                                    data_infos,
                                    server_loot,
                                    world_name,
                                    &listen_address,
                                    16
                                );

                                let (mut game_server, mut server) = match x
                                {
                                    Ok(x) => x,
                                    Err(err) => panic!("{err}")
                                };

                                *server_entities.borrow_mut() = game_server.entities();
                                *server_world.borrow_mut() = game_server.world();
                                *server_message_sender.borrow_mut() = Some(game_server.sender());

                                let port = server.port();
                                tx.send(port).unwrap();

                                thread::spawn(move ||
                                {
                                    server.run();
                                });

                                waiting_loop(||
                                {
                                    crate::frame_time_this!{
                                        [] -> server_update,
                                        game_server.update(DELTA_TIME as f32)
                                    }
                                });
                            }).map_or_else(|err|
                            {
                                eprintln!("failed to start the server thread: {err}");

                                None
                            }, Some);

                            if self.server_handle.is_none()
                            {
                                self.exit();
                                return;
                            }

                            let port = rx.recv().unwrap();

                            if client_info.online_mode == OnlineMode::Host
                            {
                                eprintln!("listening on port {port}");
                            }

                            format!("127.0.0.1:{port}")
                        } else
                        {
                            client_info.address.text
                        };

                        let mut info = partial_info.to_full(&self.client.camera.read());

                        let client_info = ClientInfo{
                            address,
                            name: client_info.name.display_name(),
                            host: client_info.online_mode != OnlineMode::Client,
                            debug: client_info.debug,
                            mouse_position: x.mouse_position(),
                            controls: x.bindings()
                        };

                        if client_info.debug
                        {
                            fn compare_size(name: &str, this: Vector2<u32>, other: Vector2<u32>)
                            {
                                fn s(size: Vector2<u32>) -> String { format!("{}x{}", size.x, size.y) }

                                if this != other
                                {
                                    eprintln!("{name} sprite size ({}) is incorrect, has to be: {}", s(this), s(other));
                                }
                            }

                            let assets = info.partial.assets.lock();

                            DoorMaterial::iter().for_each(|material|
                            {
                                (1..=2).for_each(|width|
                                {
                                    let name = door_texture(material, width);
                                    if let Some(texture) = assets.try_texture_by_name(&name)
                                    {
                                        let [x, y, _z] = texture.lock().image().extent();
                                        let expected_size = door_scale(width);

                                        compare_size(
                                            &name,
                                            vector![x, y],
                                            (expected_size / ENTITY_SCALE * ENTITY_PIXEL_SCALE as f32).map(|x| x.round() as u32)
                                        );
                                    }
                                });
                            });
                        }

                        self.client.initialize(&mut info, client_info);
                        self.scene = Scene::Game;

                        self.update_game(&mut info, dt);
                    }
                }
            }
        }
    }

    fn input(&mut self, control: Control)
    {
        let captured = match &mut self.scene
        {
            Scene::Game =>
            {
                self.client.input(control.clone())
            },
            Scene::Menu(x) =>
            {
                x.input(control.clone())
            }
        };

        if captured
        {
            return;
        }

        self.slow_mode.input(control);
    }

    fn mouse_move(&mut self, position: (f64, f64))
    {
        self.client.mouse_move(position);

        match &mut self.scene
        {
            Scene::Game => (),
            Scene::Menu(x) => x.mouse_move(position)
        }
    }

    fn draw(&mut self, info: DrawInfo)
    {
        match &mut self.scene
        {
            Scene::Game => self.client.draw(info),
            Scene::Menu(x) => x.draw(info)
        }
    }

    fn resize(&mut self, aspect: f32)
    {
        self.client.resize(aspect);

        match &mut self.scene
        {
            Scene::Game => (),
            Scene::Menu(x) => x.resize(aspect)
        }
    }

    fn render_pass_ended(&mut self, attachments: &[Arc<ImageView>], builder: &mut CommandBuilderType)
    {
        builder.blit_image(BlitImageInfo{
            ..BlitImageInfo::images(attachments[5].image().clone(), attachments[6].image().clone())
        }).unwrap();

        match &mut self.scene
        {
            Scene::Game => self.client.render_pass_ended(),
            Scene::Menu(_) => ()
        }
    }
}

impl App
{
    pub fn client(&self) -> &Client
    {
        &self.client
    }

    fn exit(&mut self)
    {
        self.engine_events.push_back(EngineEvent::Exit);
    }

    fn update_game(&mut self, info: &mut UpdateBuffersInfo, dt: f32)
    {
        let status = if SlowMode::as_bool()
        {
            if self.slow_mode.running()
            {
                self.client.update(info, dt)
            } else if self.slow_mode.run_frame()
            {
                self.client.update(info, 1.0 / 60.0)
            } else
            {
                self.client.no_update();

                true
            }
        } else
        {
            self.client.update(info, dt)
        };

        if !status
        {
            self.exit();
            return;
        }

        info.update_camera(&self.client.camera.read());

        self.client.update_buffers(info);
    }
}
