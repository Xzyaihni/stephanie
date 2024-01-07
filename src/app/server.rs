use std::{
	net::TcpListener,
	sync::Arc
};

use parking_lot::Mutex;

use crate::common::TileMapWithTextures;

use game_server::{GameServer, ParseError};

pub use connections_handler::ConnectionsHandler;

mod game_server;

pub mod connections_handler;

pub mod world;


pub struct Server
{
	listener: TcpListener,
	game_server: Arc<Mutex<GameServer>>
}

impl Server
{
	pub fn new(
		tilemap: TileMapWithTextures,
		address: &str,
		connections_limit: usize
	) -> Result<Self, ParseError>
	{
		let listener = TcpListener::bind(address)?;

        let game_server = GameServer::new(tilemap.tilemap, connections_limit)?;
		let game_server = Arc::new(Mutex::new(game_server));

		Ok(Self{
			listener,
			game_server
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
			if let Ok(stream) = connection
			{
				if let Err(x) = GameServer::connect(self.game_server.clone(), stream)
				{
					eprintln!("error in player connection: {x:?}");
					continue;
				}
			} else
			{
				eprintln!("connection error: {connection:?}");
				continue;
			}
		}
	}
}
