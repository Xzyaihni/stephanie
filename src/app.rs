use std::{
    thread::{self, JoinHandle},
    sync::{mpsc, Arc}
};

use argparse::{ArgumentParser, StoreOption, StoreTrue, Store};

use yanyaengine::{
    YanyaApp,
    Control,
    ShaderId,
    game_object::*
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

pub mod common;

pub mod server;
pub mod client;


pub struct ProgramShaders
{
    pub default: ShaderId,
    pub world: ShaderId,
    pub shadow: ShaderId,
    pub ui: ShaderId
}

pub struct AppInfo
{
    pub shaders: ProgramShaders
}

pub struct App
{
    client: Client,
    server_handle: Option<JoinHandle<()>>
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

        let mut name = "player_name".to_owned();

        let mut address = None;
        let mut port = None;

        let mut debug_mode = false;

        {
            let mut parser = ArgumentParser::new();

            parser.refer(&mut name)
                .add_option(&["-n", "--name"], Store, "player name");

            parser.refer(&mut address)
                .add_option(&["-a", "--address"], StoreOption, "connection address");

            parser.refer(&mut port)
                .add_option(&["-p", "--port"], StoreOption, "hosting port");

            parser.refer(&mut debug_mode)
                .add_option(&["-d", "--debug"], StoreTrue, "enable debug mode");

            parser.parse_args_or_exit();
        }
        
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
                debug_mode
            },
            app_info,
            tilemap: deferred_parse().unwrap(),
            data_infos,
            host
        };

        Self{
            client: Client::new(partial_info, client_init_info).unwrap(),
            server_handle
        }
    }

    fn update(&mut self, dt: f32)
    {
        self.client.update(dt.min(1.0 / 20.0));
    }

    fn update_buffers(&mut self, partial_info: UpdateBuffersPartialInfo)
    {
        self.client.update_buffers(partial_info);
    }

    fn input(&mut self, control: Control)
    {
        self.client.input(control);
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
