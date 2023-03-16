use std::{
	sync::Arc,
	net::TcpStream
};

use parking_lot::RwLock;

use slab::Slab;

use crate::common::{
	sender_loop,
	receiver_loop,
	BufferSender,
	EntityType,
	EntityPasser,
	EntitiesContainer,
	EntitiesController,
	MessagePasser,
	player::{Player, PlayerProperties},
	physics::PhysicsEntity,
	message::{
		Message,
		MessageBuffer
	}
};


#[derive(Debug)]
pub struct PlayerInfo
{
	message_buffer: MessageBuffer,
	message_passer: MessagePasser
}

impl PlayerInfo
{
	pub fn set_message(&mut self, message: Message)
	{
		self.message_buffer.set_message(message);
	}
}

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
pub struct ConnectionsHandler
{
	connections: Slab<PlayerInfo>
}

impl ConnectionsHandler
{
	pub fn new(limit: usize) -> Self
	{
		let connections = Slab::with_capacity(limit);

		Self{connections}
	}

	pub fn remove_connection(&mut self, id: usize)
	{
		self.connections.remove(id);
	}

	pub fn connections_amount(&self) -> usize
	{
		self.connections.len()
	}

	pub fn connect(&mut self, player_info: PlayerInfo) -> usize
	{
		self.connections.insert(player_info)
	}

	pub fn get_mut(&mut self, id: usize) -> &mut PlayerInfo
	{
		self.connections.get_mut(id).unwrap()
	}
}

impl EntityPasser for ConnectionsHandler
{
	fn send_message(&mut self, message: Message)
	{
		let entity_type = message.entity_type();

		self.connections.iter_mut().filter(|(index, _)|
		{
			Some(EntityType::Player(*index)) != entity_type
		}).for_each(|(_, player_info)|
		{
			player_info.set_message(message.clone());
		});
	}
}

impl BufferSender for ConnectionsHandler
{
	fn send_buffered(&mut self) -> Result<(), bincode::Error>
	{
		self.connections.iter_mut().try_for_each(|(_, connection)|
		{
			connection.message_buffer.get_buffered().try_for_each(|message|
			{
				connection.message_passer.send(&message)
			})
		})
	}
}

#[derive(Debug)]
pub enum ConnectionError
{
	BincodeError(bincode::Error),
	WrongConnectionMessage
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
	connection_handler: Arc<RwLock<ConnectionsHandler>>
}

impl GameServer
{
	pub fn new(limit: usize) -> Self
	{
		let entities = ServerEntitiesContainer::new(limit);
		let connection_handler = Arc::new(RwLock::new(ConnectionsHandler::new(limit)));

		Self{entities, connection_handler}
	}

	pub fn player_connect(
		this: Arc<RwLock<Self>>,
		stream: TcpStream
	) -> Result<(), ConnectionError>
	{
		let mut message_passer = MessagePasser::new(stream);

		let name = match message_passer.receive()?
		{
			Message::PlayerConnect{name} => name,
			_ =>
			{
				return Err(ConnectionError::WrongConnectionMessage);
			}
		};

		eprintln!("player \"{name}\" connected");

		let player_info = this.read().player_by_name(&name);
		let player_info = match player_info
		{
			Some(player) => player,
			None => PlayerInfo{
				message_buffer: MessageBuffer::new(),
				message_passer
			}
		};

		let (id, messager) = {
			let mut writer = this.write();

			let player = Player::new(PlayerProperties{name, ..Default::default()});
			let inserted_id = writer.add_player(player);

			let mut connection_handler = writer.connection_handler.write();
			let player_id = connection_handler.connect(player_info);

			let messager = &mut connection_handler.get_mut(player_id).message_passer;

			if player_id != inserted_id
			{
				panic!("something went horribly wrong, ids of player and connection dont match");
			}

			messager.send(&Message::PlayerOnConnect{id: player_id})?;

			writer.entities.players_ref().iter().try_for_each(|(index, player)|
			{
				let entity_type = EntityType::Player(index);
				let entity = player.entity_clone();

				messager.send(&Message::PlayerCreate{id: index, player: player.clone()})?;
				messager.send(&Message::EntitySync{entity_type, entity})
			})?;

			messager.send(&Message::PlayerFullyConnected)?;

			(player_id, messager.try_clone())
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
		receiver_loop(this, handler, Self::process_message, move |this|
		{
			let mut writer = this.write();

			writer.connection_handler.write().remove_connection(id);
			writer.remove_player(id);
		});
	}

	pub fn process_message(this: Arc<RwLock<Self>>, message: Message)
	{
		let id_mismatch = || panic!("id mismatch in serverside process message");

		let mut writer = this.write();

		writer.connection_handler.write().send_message(message.clone());

		let message = writer.entities.handle_message(message);

		if let Some(message) = message
		{
			match message
			{
				Message::PlayerCreate{id, player} =>
				{
					if id != writer.entities.players_mut().insert(player)
					{
						id_mismatch();
					}
				},
				_ => ()
			}
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