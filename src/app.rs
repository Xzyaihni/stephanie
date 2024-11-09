use std::{
    env,
    thread::{self, JoinHandle},
    sync::{mpsc, Arc}
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

use common::{
    TileMap,
    DataInfos,
    ItemsInfo,
    EnemiesInfo,
    CharactersInfo,
    CharacterInfo,
    sender_loop::{waiting_loop, DELTA_TIME}
};

use server::Server;

use client::{
    Client,
    ClientInitInfo,
    ClientInfo
};

use config::Config;

mod config;

pub mod common;

pub mod server;
pub mod client;


pub struct ProgramShaders
{
    pub default: ShaderId,
    pub default_shaded: ShaderId,
    pub world: ShaderId,
    pub world_shaded: ShaderId,
    pub shadow: ShaderId,
    pub ui: ShaderId
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
    type AppInfo = Option<AppInfo>;

    fn init(partial_info: InitPartialInfo, app_info: Self::AppInfo) -> Self
    {
        let deferred_parse = || TileMap::parse("tiles/tiles.json", "textures/tiles/");
        let app_info = app_info.unwrap();

        let Config{name, address, port, debug} = Config::parse(env::args().skip(1));

        let items_info = ItemsInfo::parse(
            &partial_info.assets.lock(),
            "items",
            "items/items.json"
        );

        let mut characters_info = CharactersInfo::new();

        let player_character = characters_info.push(CharacterInfo::player(
            &partial_info.assets.lock()
        ));

        let enemies_info = EnemiesInfo::parse(
            &partial_info.assets.lock(),
            &mut characters_info,
            "enemy",
            "enemies/enemies.json"
        );

        let data_infos = DataInfos{
            items_info: Arc::new(items_info),
            enemies_info: Arc::new(enemies_info),
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

                        let x = Server::new(
                            tilemap,
                            data_infos,
                            &format!("0.0.0.0:{port}"),
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
}
