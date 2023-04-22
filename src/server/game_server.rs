use std::{
	sync::Arc,
	net::TcpStream
};

use parking_lot::RwLock;

use slab::Slab;

use super::{
	ConnectionsHandler,
	connections_handler::PlayerInfo,
	world_generator::WorldGenerator
};

use crate::common::{
	sender_loop,
	receiver_loop,
	TileMap,
	EntityPasser,
	EntitiesContainer,
	EntitiesController,
	MessagePasser,
	player::{Player, PlayerProperties},
	message::{
		Message,
		MessageBuffer
	}
};


#[derive(Debug)]
pub struct ServerEntitiesContainer
{
	players: Slab<Player>
}

impl ServerEntitiesContainer
{
	pub fn new(limit: usize) -> Self
	{
		let players = Slab::with_capacity(limit);

		Self{players}
	}
}

impl EntitiesContainer for ServerEntitiesContainer
{
	type PlayerObject = Player;

	fn players_ref(&self) -> &Slab<Self::PlayerObject>
	{
		&self.players
	}

	fn players_mut(&mut self) -> &mut Slab<Self::PlayerObject>
	{
		&mut self.players
	}
}

#[derive(Debug)]
pub enum ConnectionError
{
	BincodeError(bincode::Error),
	WrongConnectionMessage,
	IdMismatch
}

impl From<bincode::Error> for ConnectionError
{
	fn from(value: bincode::Error) -> Self
	{
		ConnectionError::BincodeError(value)
	}
}

#[derive(Debug)]
pub struct GameServer
{
	entities: ServerEntitiesContainer,
	connection_handler: Arc<RwLock<ConnectionsHandler>>,
	world_generator: WorldGenerator
}

impl GameServer
{
	pub fn new(tilemap: TileMap, limit: usize) -> Self
	{
		let entities = ServerEntitiesContainer::new(limit);
		let connection_handler = Arc::new(RwLock::new(ConnectionsHandler::new(limit)));

		let world_generator = WorldGenerator::new(connection_handler.clone(), tilemap);

		Self{entities, connection_handler, world_generator}
	}

	pub fn player_connect(
		this: Arc<RwLock<Self>>,
		stream: TcpStream
	) -> Result<(), ConnectionError>
	{
		let mut message_passer = MessagePasser::new(stream);

		let name = match message_passer.receive_one()?
		{
			Some(Message::PlayerConnect{name}) => name,
			_ =>
			{
				return Err(ConnectionError::WrongConnectionMessage);
			}
		};

		println!("player \"{name}\" connected");

		let player_info = this.read().player_by_name(&name);
		let player_info = match player_info
		{
			Some(player) => player,
			None => PlayerInfo::new(MessageBuffer::new(), message_passer)
		};

		let (id, messager) = {
			let mut writer = this.write();

			let player_properties = PlayerProperties{
				..Default::default()
			};

			let player = Player::new(player_properties);

			let inserted_id = writer.add_player(player);

			let mut connection_handler = writer.connection_handler.write();
			let player_id = connection_handler.connect(player_info);

			let messager = connection_handler.get_mut(player_id);

			if player_id != inserted_id
			{
				return Err(ConnectionError::IdMismatch);
			}

			messager.send_blocking(Message::PlayerOnConnect{id: player_id})?;

			writer.entities.players_ref().iter().try_for_each(|(index, player)|
			{
				messager.send_blocking(Message::PlayerCreate{id: index, player: player.clone()})
			})?;

			messager.send_blocking(Message::PlayerFullyConnected)?;

			(player_id, messager.clone_messager())
		};

		Self::listen(this, messager, id);

		Ok(())
	}

	pub fn sender_loop(&self)
	{
		sender_loop(self.connection_handler.clone());
	}

	pub fn listen(this: Arc<RwLock<Self>>, handler: MessagePasser, id: usize)
	{
		receiver_loop(this, handler, move |this, message|
		{
			Self::process_message(this, id, message);
		}, move |this|
		{
			Self::on_connection_closed(this, id);
		});
	}

	fn on_connection_closed(this: Arc<RwLock<Self>>, id: usize)
	{
		let mut writer = this.write();

		println!("player \"{}\" disconnected", writer.player_ref(id).name());

		writer.connection_handler.write().remove_connection(id);
		writer.remove_player(id);
	}

	pub fn process_message(this: Arc<RwLock<Self>>, id: usize, message: Message)
	{
		let id_mismatch = || panic!("id mismatch in serverside process message");

		let mut writer = this.write();

		if message.forward()
		{
			writer.connection_handler.write().send_message(message.clone());
		}

		let message = match writer.entities.handle_message(message)
		{
			Some(x) => x,
			None => return
		};

		let message = match writer.world_generator.handle_message(id, message)
		{
			Some(x) => x,
			None => return
		};

		match message
		{
			Message::PlayerCreate{id, player} =>
			{
				if id != writer.entities.players_mut().insert(player)
				{
					id_mismatch();
				}
			},
			x => panic!("unhandled message: {:?}", x)
		}
	}

	pub fn player_by_name(&self, _name: &str) -> Option<PlayerInfo>
	{
		eprintln!("nyo player loading for now,,");
		None
	}

	pub fn connections_amount(&self) -> usize
	{
		self.connection_handler.read().connections_amount()
	}
}

impl EntitiesController for GameServer
{
	type Container = ServerEntitiesContainer;
	type Passer = ConnectionsHandler;

	fn container_ref(&self) -> &Self::Container
	{
		&self.entities
	}

	fn container_mut(&mut self) -> &mut Self::Container
	{
		&mut self.entities
	}

	fn passer(&self) -> Arc<RwLock<Self::Passer>>
	{
		self.connection_handler.clone()
	}
}