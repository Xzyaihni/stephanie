use std::{
    rc::Rc,
    net::{TcpStream, TcpListener},
    sync::mpsc::Sender
};

use crate::common::{
    DataInfos,
    TileMap,
    loot::ServerScripts
};

use game_server::{GameServer, ParseError};

pub use connections_handler::ConnectionsHandler;

mod game_server;

pub mod connections_handler;

pub mod world;


pub struct ServerInfo
{
    pub server_scripts: ServerScripts,
    pub world_name: String,
    pub connections_limit: usize
}

pub struct Server
{
    listener: TcpListener,
    connector: Sender<TcpStream>
}

impl Server
{
    pub fn new(
        tilemap: Rc<TileMap>,
        data_infos: DataInfos,
        server_info: ServerInfo,
        address: &str
    ) -> Result<(GameServer, Self), ParseError>
    {
        let listener = TcpListener::bind(address)?;

        let (connector, game_server) = GameServer::new(
            tilemap,
            data_infos,
            server_info
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
