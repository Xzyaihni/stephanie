use std::{
    thread,
    net::{TcpStream, TcpListener},
    sync::{Arc, mpsc::Sender}
};

use parking_lot::Mutex;

use crate::common::{
    sender_loop::{waiting_loop, DELTA_TIME},
    DataInfos,
    TileMapWithTextures
};

use game_server::{GameServer, ParseError};

pub use connections_handler::ConnectionsHandler;

mod game_server;

pub mod connections_handler;

pub mod world;


pub struct Server
{
    listener: TcpListener,
    connector: Sender<TcpStream>
}

impl Server
{
    pub fn new(
        tilemap: TileMapWithTextures,
        data_infos: DataInfos,
        address: &str,
        connections_limit: usize
    ) -> Result<Self, ParseError>
    {
        let listener = TcpListener::bind(address)?;

        let (connector, game_server) = GameServer::new(
            tilemap.tilemap,
            data_infos,
            connections_limit
        )?;

        let game_server = Arc::new(Mutex::new(game_server));

        {
            let game_server = game_server.clone();
            thread::spawn(move ||
            {
                waiting_loop(||
                {
                    game_server.lock().update(DELTA_TIME as f32);

                    false
                });
            });
        }

        Ok(Self{
            listener,
            connector
        })
    }

    pub fn port(&self) -> u16
    {
        self.listener.local_addr().unwrap().port()
    }

    pub fn run(&mut self)
    {
        for connection in self.listener.incoming()
        {
            match connection
            {
                Ok(stream) =>
                {
                    if let Err(x) = self.connector.send(stream)
                    {
                        eprintln!("error in player connection: {x}");
                        continue;
                    }
                },
                Err(err) =>
                {
                    eprintln!("connection error: {err}");
                    continue;
                }
            }
        }
    }
}
