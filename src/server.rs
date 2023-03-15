use std::{
	sync::Arc,
	io,
	net::TcpListener
};

use parking_lot::RwLock;

use game_server::GameServer;

mod game_server;


pub struct Server
{
	listener: TcpListener,
	connections_limit: usize,
	game_server: Arc<RwLock<GameServer>>
}

impl Server
{
	pub fn new(address: &str, connections_limit: usize) -> io::Result<Self>
	{
		let listener = TcpListener::bind(format!("{address}:0"))?;

		let game_server = Arc::new(RwLock::new(GameServer::new(connections_limit)));
		game_server.read().sender_loop();

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

				GameServer::player_connect(
					self.game_server.clone(),
					stream
				);
			}
		}
	}
}