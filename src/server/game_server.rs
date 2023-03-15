use std::{
	thread,
	sync::Arc,
	net::TcpStream
};

use parking_lot::RwLock;

use slab::Slab;

use crate::common::{
	sender_loop,
	BufferSender,
	TransformContainer,
	EntityType,
	EntityPasser,
	EntitiesContainer,
	EntitiesController,
	MessagePasser,
	player::Player,
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
	fn send_buffered(&mut self)
	{
		self.connections.iter_mut().for_each(|(_, connection)|
		{
			connection.message_buffer.get_buffered().for_each(|message|
			{
				connection.message_passer.send(&message);
			});
		});
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

	pub fn player_connect(this: Arc<RwLock<Self>>, stream: TcpStream)
	{
		let mut message_passer = MessagePasser::new(stream);

		let message = message_passer.receive();
		let name = match message.clone()
		{
			Message::PlayerConnect{name} => name,
			_ =>
			{
				eprintln!("wrong connecting message");
				return;
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

		let messager = {
			let mut writer = this.write();

			let player = Player::new(name);
			let inserted_id = writer.add_player(player);

			let mut connection_handler = writer.connection_handler.write();
			let player_id = connection_handler.connect(player_info);

			let messager = &mut connection_handler.get_mut(player_id).message_passer;

			if player_id != inserted_id
			{
				panic!("something went horribly wrong, ids of player and connection dont match");
			}

			messager.send(&Message::PlayersList{id: player_id});

			for (index, player) in writer.entities.players_ref().iter()
			{
				let entity = EntityType::Player(index);
				let transform = player.transform_ref().clone();

				messager.send(&Message::PlayerCreate{id: index, player: player.clone()});
				messager.send(&Message::EntityTransform{entity, transform});
			}

			messager.send(&Message::PlayerFullyConnected);

			messager.try_clone()
		};

		Self::listen(this, messager);
	}

	pub fn sender_loop(&self)
	{
		let handler = self.connection_handler.clone();
		thread::spawn(move ||
		{
			sender_loop(handler);
		});
	}

	pub fn listen(this: Arc<RwLock<Self>>, mut handler: MessagePasser)
	{
		thread::spawn(move ||
		{
			loop
			{
				Self::process_message(this.clone(), handler.receive());
			}
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