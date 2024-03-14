use std::{
    fmt,
	net::TcpStream,
	sync::Arc
};

use nalgebra::Vector3;

use parking_lot::{RwLock, Mutex};

use slab::Slab;

use yanyaengine::{TransformContainer, Transform};

use super::{
	ConnectionsHandler,
	connections_handler::PlayerInfo,
	world::World
};

pub use super::world::ParseError;

use crate::common::{
	sender_loop,
	receiver_loop,
	TileMap,
	EntityPasser,
	EntitiesContainer,
	EntitiesController,
	MessagePasser,
    PlayerProperties,
    CharacterProperties,
    EntityProperties,
    PhysicalProperties,
    world::chunk::TILE_SIZE,
	player::Player,
    enemy::Enemy,
	message::{
		Message,
		MessageBuffer
	}
};


#[derive(Debug)]
pub struct ServerEntitiesContainer
{
	players: Slab<Player>,
    enemies: Slab<Enemy>
}

impl ServerEntitiesContainer
{
	pub fn new(limit: usize) -> Self
	{
		let players = Slab::with_capacity(limit);
        let enemies = Slab::with_capacity(limit);

		Self{players, enemies}
	}
}

impl EntitiesContainer for ServerEntitiesContainer
{
	type PlayerObject = Player;
    type EnemyObject = Enemy;

	fn players_ref(&self) -> &Slab<Self::PlayerObject>
	{
		&self.players
	}

	fn players_mut(&mut self) -> &mut Slab<Self::PlayerObject>
	{
		&mut self.players
	}

	fn enemies_ref(&self) -> &Slab<Self::EnemyObject>
	{
		&self.enemies
	}

	fn enemies_mut(&mut self) -> &mut Slab<Self::EnemyObject>
	{
		&mut self.enemies
	}
}

#[derive(Debug)]
pub enum ConnectionError
{
	BincodeError(bincode::Error),
	WrongConnectionMessage,
	IdMismatch
}

impl fmt::Display for ConnectionError
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        let s = match self
        {
            Self::BincodeError(x) => x.to_string(),
            Self::WrongConnectionMessage => "wrong connection message".to_owned(),
            Self::IdMismatch => "id mismatch".to_owned()
        };

        write!(f, "{s}")
    }
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
	world: World,
	connection_handler: Arc<RwLock<ConnectionsHandler>>
}

impl GameServer
{
	pub fn new(tilemap: TileMap, limit: usize) -> Result<Self, ParseError>
	{
		let entities = ServerEntitiesContainer::new(limit);
		let connection_handler = Arc::new(RwLock::new(ConnectionsHandler::new(limit)));

		let world = World::new(connection_handler.clone(), tilemap)?;

		sender_loop(connection_handler.clone());

		Ok(Self{entities, world, connection_handler})
	}

	pub fn connect(this: Arc<Mutex<Self>>, stream: TcpStream) -> Result<(), ConnectionError>
	{
		if this.lock().connection_handler.read().under_limit()
		{
			Self::player_connect(this, stream)
		} else
		{
			Ok(())
		}
	}

	pub fn player_connect(
		this: Arc<Mutex<Self>>,
		stream: TcpStream
	) -> Result<(), ConnectionError>
	{
		let (id, messager) = this.lock().player_connect_inner(stream)?;

		let other_this = this.clone();
		receiver_loop(
			messager,
			move |message| this.lock().process_message_inner(message, id),
			move || other_this.lock().connection_close(id)
		);

		Ok(())
	}

	fn player_connect_inner(
		&mut self,
		stream: TcpStream
	) -> Result<(usize, MessagePasser), ConnectionError>
	{
		let player_info = self.player_info(stream)?;

        let player_index = self.entities.players_ref().len() + 1;

		let player_properties = PlayerProperties{
            name: format!("stephanie #{player_index}"),
            character_properties: CharacterProperties{
                speed: 0.5,
                entity_properties: EntityProperties{
                    texture: "player/hair.png".to_owned(),
                    physical: PhysicalProperties{
                        mass: 50.0,
                        friction: 0.5,
                        transform: Transform{
                            scale: Vector3::repeat(0.1),
                            ..Default::default()
                        }
                    }
                }
            }
		};

		let player = {
			let mut player = Player::new(player_properties);
			player.translate(Vector3::new(0.0, 0.0, TILE_SIZE));

			player
		};

		let world_id = self.world.add_player((*player.position()).into());
		let inserted_id = self.add_player(player);

		if world_id != inserted_id
		{
			return Err(ConnectionError::IdMismatch);
		}

		self.player_create(player_info, inserted_id)
	}

	fn player_info(&self, stream: TcpStream) -> Result<PlayerInfo, ConnectionError>
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

		let player_info = self.player_by_name(&name);

		Ok(player_info.unwrap_or_else(||
		{
			PlayerInfo::new(MessageBuffer::new(), message_passer)
		}))
	}

	fn player_create(
		&mut self,
		player_info: PlayerInfo,
		check_id: usize
	) -> Result<(usize, MessagePasser), ConnectionError>
	{
		let mut connection_handler = self.connection_handler.write();
		let player_id = connection_handler.connect(player_info);

		let messager = connection_handler.get_mut(player_id);

		if player_id != check_id
		{
			return Err(ConnectionError::IdMismatch);
		}

		messager.send_blocking(Message::PlayerOnConnect{id: player_id})?;

		self.entities.players_ref().iter().try_for_each(|(index, player)|
		{
			messager.send_blocking(Message::PlayerCreate{id: index, player: player.clone()})
		})?;

		messager.send_blocking(Message::PlayerFullyConnected)?;

		Ok((player_id, messager.clone_messager()))
	}

	fn connection_close(&mut self, id: usize)
	{
		let player = self.player_ref(id);

		println!("player \"{}\" disconnected", player.name());

		self.world.remove_player(id);

		self.connection_handler.write().remove_connection(id);
		self.remove_player(id);
	}

	fn process_message_inner(&mut self, message: Message, id: usize)
	{
		let id_mismatch = || panic!("id mismatch in serverside process message");

        let message = match message
        {
            Message::RepeatMessage{message} =>
            {
                self.connection_handler.write().send_message(*message);

                return;
            },
            x => x
        };

		if message.forward()
		{
			self.connection_handler.write().send_message(message.clone());
		}

		let message = match self.world.handle_message(id, message)
		{
			Some(x) => x,
			None => return
		};

		let message = match self.entities.handle_message(message)
		{
			Some(x) => x,
			None => return
		};

		match message
		{
			Message::PlayerCreate{id, player} =>
			{
				if id != self.entities.players_mut().insert(player)
				{
					id_mismatch();
				}
			},
			x => panic!("unhandled message: {x:?}")
		}
	}

	pub fn player_by_name(&self, _name: &str) -> Option<PlayerInfo>
	{
		eprintln!("nyo player loading for now,,");
		None
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
