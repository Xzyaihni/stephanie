use std::{
    net::{TcpStream, TcpListener},
    sync::mpsc::Sender
};

use crate::common::{
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
        world_name: String,
        address: &str,
        connections_limit: usize
    ) -> Result<(GameServer, Self), ParseError>
    {
        let listener = TcpListener::bind(address)?;

        let (connector, game_server) = GameServer::new(
            tilemap.tilemap,
            data_infos,
            world_name,
            connections_limit
        )?;

        Ok((game_server, Self{
            listener,
            connector
        }))
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
