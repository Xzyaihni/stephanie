use std::{
	sync::Arc,
	net::TcpListener
};

use parking_lot::RwLock;

use crate::common::TileMap;

use game_server::{GameServer, ParseError};

pub use connections_handler::ConnectionsHandler;

mod game_server;

pub mod connections_handler;

pub mod world;


pub struct Server
{
	listener: TcpListener,
	connections_limit: usize,
	game_server: Arc<RwLock<GameServer>>
}

impl Server
{
	pub fn new(
		tilemap: TileMap,
		address: &str,
		connections_limit: usize
	) -> Result<Self, ParseError>
	{
		let listener = TcpListener::bind(format!("{address}"))?;

		let game_server = Arc::new(RwLock::new(GameServer::new(tilemap, connections_limit)?));
		game_server.write().sender_loop();

		Ok(Self{
			listener,
			connections_limit,
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
				if self.game_server.read().connections_amount() >= self.connections_limit
				{
					return;
				}

				if let Err(x) = GameServer::player_connect(self.game_server.clone(), stream)
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