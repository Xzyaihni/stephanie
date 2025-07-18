use std::{
    env,
    thread::{self, JoinHandle},
    sync::{mpsc, Arc}
};

use vulkano::device::Device;

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
    debug_config::*
};

use crate::common::{
    TileMap,
    DataInfos,
    ItemsInfo,
    EnemiesInfo,
    FurnituresInfo,
    CharactersInfo,
    CharacterInfo,
    sender_loop::{waiting_loop, DELTA_TIME}
};

use crate::server::Server;

use crate::client::{
    Client,
    ClientInitInfo,
    ClientInfo
};

use config::Config;

mod config;


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
    pub above_world: ShaderId,
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
    pub ui: ShaderId,
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
                        eprintln!("slow mode running state: {}", self.running);
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

pub struct App
{
    client: Client,
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

    fn init(partial_info: InitPartialInfo<Self::SetupInfo>, app_info: Self::AppInfo) -> Self
    {
        let deferred_parse = || TileMap::parse("tiles/tiles.json", "textures/tiles/");
        let app_info = app_info.unwrap();

        let Config{name, listen_outside, address, port, debug} = Config::parse(env::args().skip(1));

        let items_info = ItemsInfo::parse(
            &partial_info.object_info.assets.lock(),
            "items",
            "items/items.json"
        );

        let mut characters_info = CharactersInfo::new();

        let player_character = characters_info.push(CharacterInfo::player(
            &partial_info.object_info.assets.lock()
        ));

        let enemies_info = EnemiesInfo::parse(
            &partial_info.object_info.assets.lock(),
            &mut characters_info,
            "enemy",
            "enemies/enemies.json"
        );

        let furnitures_info = FurnituresInfo::parse(
            &partial_info.object_info.assets.lock(),
            "furniture",
            "furnitures/furnitures.json"
        );

        let data_infos = DataInfos{
            items_info: Arc::new(items_info),
            enemies_info: Arc::new(enemies_info),
            furnitures_info: Arc::new(furnitures_info),
            characters_info: Arc::new(characters_info),
            player_character
        };

        let mut server_handle = None;
        let (host, client_address) = if let Some(address) = address
        {
            (false, address)
        } else
        {
            let (tx, rx) = mpsc::channel();

            let data_infos = data_infos.clone();
            server_handle = Some(thread::spawn(move ||
            {
                match deferred_parse()
                {
                    Ok(tilemap) =>
                    {
                        let port = port.unwrap_or(0);

                        let listen_address = format!("{}:{port}", if listen_outside { "0.0.0.0" } else { "localhost" });

                        let x = Server::new(
                            tilemap,
                            data_infos,
                            &listen_address,
                            16
                        );

                        let (mut game_server, mut server) = match x
                        {
                            Ok(x) => x,
                            Err(err) => panic!("{err}")
                        };

                        let port = server.port();
                        tx.send(port).unwrap();

                        thread::spawn(move ||
                        {
                            server.run();
                        });

                        waiting_loop(||
                        {
                            game_server.update(DELTA_TIME as f32)
                        });
                    },
                    Err(err) => panic!("error parsing tilemap: {err}")
                }
            }));

            let port = rx.recv().unwrap();

            println!("listening on port {port}");
            (true, format!("127.0.0.1:{port}"))
        };

        let client_init_info = ClientInitInfo{
            client_info: ClientInfo{
                address: client_address,
                name,
                debug
            },
            app_info,
            tilemap: deferred_parse().unwrap(),
            data_infos,
            host
        };

        DebugConfig::on_start();

        Self{
            client: Client::new(partial_info, client_init_info).unwrap(),
            server_handle,
            slow_mode: Default::default()
        }
    }

    fn update(&mut self, partial_info: UpdateBuffersPartialInfo, dt: f32)
    {
        let mut info = partial_info.to_full(&self.client.camera.read());

        if DebugConfig::is_enabled(DebugTool::SuperSpeed)
        {
            for _ in 0..10
            {
                self.client.update(&mut info, 1.0 / 60.0);
            }
        }

        let dt = dt.min(LONGEST_FRAME as f32);

        if SlowMode::as_bool()
        {
            if self.slow_mode.running()
            {
                self.client.update(&mut info, dt);
            } else if self.slow_mode.run_frame()
            {
                self.client.update(&mut info, 1.0 / 60.0);
            } else
            {
                self.client.no_update();
            }
        } else
        {
            self.client.update(&mut info, dt);
        }

        info.update_camera(&self.client.camera.read());

        self.client.update_buffers(&mut info);
    }

    fn input(&mut self, control: Control)
    {
        if self.client.input(control.clone()) { return };

        self.slow_mode.input(control);
    }

    fn mouse_move(&mut self, position: (f64, f64))
    {
        self.client.mouse_move(position);
    }

    fn draw(&mut self, info: DrawInfo)
    {
        self.client.draw(info);
    }

    fn resize(&mut self, aspect: f32)
    {
        self.client.resize(aspect);
    }

    fn render_pass_ended(&mut self, _builder: &mut CommandBuilderType)
    {
        self.client.render_pass_ended();
    }
}
