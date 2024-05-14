use std::{
    thread,
    sync::{mpsc, Arc}
};

use argparse::{ArgumentParser, StoreOption, StoreTrue, Store};

use yanyaengine::{
    YanyaApp,
    Control,
    game_object::*
};

use common::{TileMap, EnemiesInfo};

use server::Server;

use client::{
    Client,
    ClientInitInfo,
    ClientInfo
};

pub mod common;

pub mod server;
pub mod client;


pub struct App(Client);

impl YanyaApp for App
{
    fn init(partial_info: InitPartialInfo) -> Self
    {
        let deferred_parse = || TileMap::parse("tiles/tiles.json", "textures/tiles/");

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
        
        let enemies_info = EnemiesInfo::parse(
            &partial_info.assets.lock(),
            "enemy",
            "enemies/enemies.json"
        );

        let enemies_info = Arc::new(enemies_info);

        let client_address = if let Some(address) = address
        {
            address
        } else
        {
            let (tx, rx) = mpsc::channel();

            let enemies_info = enemies_info.clone();
            thread::spawn(move ||
            {
                match deferred_parse()
                {
                    Ok(tilemap) =>
                    {
                        let port = port.unwrap_or(0);

                        let server = Server::new(
                            tilemap,
                            enemies_info,
                            &format!("0.0.0.0:{port}"),
                            16
                        );

                        let mut server = match server
                        {
                            Ok(x) => x,
                            Err(err) => panic!("{err}")
                        };

                        let port = server.port();
                        tx.send(port).unwrap();

                        server.run()
                    },
                    Err(err) => panic!("error parsing tilemap: {err}")
                }
            });

            let port = rx.recv().unwrap();

            println!("listening on port {port}");
            format!("127.0.0.1:{port}")
        };

        let client_init_info = ClientInitInfo{
            client_info: ClientInfo{
                address: client_address,
                name,
                debug_mode
            },
            tilemap: deferred_parse().unwrap(),
            enemies_info
        };

        Self(Client::new(partial_info, client_init_info).unwrap())
    }

    fn update(&mut self, dt: f32)
    {
        self.0.update(dt);
    }

    fn update_buffers(&mut self, partial_info: UpdateBuffersPartialInfo)
    {
        self.0.update_buffers(partial_info);
    }

    fn input(&mut self, control: Control)
    {
        self.0.input(control);
    }

    fn mouse_move(&mut self, position: (f64, f64))
    {
        self.0.mouse_move(position);
    }

    fn draw(&mut self, info: DrawInfo)
    {
        self.0.draw(info);
    }

    fn resize(&mut self, aspect: f32)
    {
        self.0.resize(aspect);
    }
}
