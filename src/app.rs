use std::{
    thread,
    sync::mpsc
};

use argparse::{ArgumentParser, StoreOption, StoreTrue, Store};

use yanyaengine::{
    YanyaApp,
    Control,
    game_object::*
};

use common::TileMap;

use server::Server;

use client::{
    Client,
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

        let name = "stephanie #1".to_owned();
        let mut client_info = ClientInfo{address: String::new(), name, debug_mode: false};

        let mut address = None;
        let mut port = None;

        {
            let mut parser = ArgumentParser::new();

            parser.refer(&mut client_info.name)
                .add_option(&["-n", "--name"], Store, "player name");

            parser.refer(&mut address)
                .add_option(&["-a", "--address"], StoreOption, "connection address");

            parser.refer(&mut port)
                .add_option(&["-p", "--port"], StoreOption, "hosting port");

            parser.refer(&mut client_info.debug_mode)
                .add_option(&["-d", "--debug"], StoreTrue, "enable debug mode");

            parser.parse_args_or_exit();
        }

        if let Some(address) = address
        {
            client_info.address = address;
        } else
        {
            let (tx, rx) = mpsc::channel();

            thread::spawn(move ||
            {
                match deferred_parse()
                {
                    Ok(tilemap) =>
                    {
                        let port = port.unwrap_or(0);
                        let mut server = match Server::new(tilemap, &format!("0.0.0.0:{port}"), 16)
                        {
                            Ok(x) => x,
                            Err(err) if err.printable().is_some() =>
                            {
                                panic!("{}", err.printable().unwrap())
                            },
                            Err(err) => panic!("{:?}", err)
                        };

                        let port = server.port();
                        tx.send(port).unwrap();

                        server.run()
                    },
                    Err(err) => panic!("error parsing tilemap: {:?}", err)
                }
            });

            let port = rx.recv().unwrap();

            client_info.address = "127.0.0.1".to_owned() + &format!(":{port}");
            println!("listening on port {port}");
        }

        Self(Client::new(partial_info, client_info).unwrap())
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
