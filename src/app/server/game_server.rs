use std::{
    fmt,
	net::TcpStream,
	sync::Arc
};

use parking_lot::{RwLock, Mutex};

use nalgebra::Vector3;

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
    ObjectsStore,
    TileMap,
    Entity,
    EntityInfo,
    Component,
    RenderInfo,
    Player,
    Entities,
    Anatomy,
    HumanAnatomy,
    EntityPasser,
    EntitiesController,
    MessagePasser,
    PhysicalProperties,
    world::chunk::TILE_SIZE,
    message::{
        Message,
        MessageBuffer
    }
};


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

pub struct GameServer
{
	entities: Entities,
	world: World,
	connection_handler: Arc<RwLock<ConnectionsHandler>>
}

impl GameServer
{
	pub fn new(tilemap: TileMap, limit: usize) -> Result<Self, ParseError>
	{
		let entities = Entities::new();
		let connection_handler = Arc::new(RwLock::new(ConnectionsHandler::new(limit)));

		let world = World::new(connection_handler.clone(), tilemap)?;

		sender_loop(connection_handler.clone());

		Ok(Self{entities, world, connection_handler})
	}

    pub fn update(&mut self, dt: f32)
    {
        const STEPS: u32 = 2;

        for _ in 0..STEPS
        {
            let dt = dt / STEPS as f32;

            self.entities.update_physical(dt);
        }
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
		let (player, id, messager) = this.lock().player_connect_inner(stream)?;

		let other_this = this.clone();
		receiver_loop(
			messager,
			move |message| this.lock().process_message_inner(message, id, player),
			move || other_this.lock().connection_close(id, player)
		);

		Ok(())
	}

	fn player_connect_inner(
		&mut self,
		stream: TcpStream
	) -> Result<(Entity, usize, MessagePasser), ConnectionError>
	{
        let player_index = self.entities.player.len() + 1;

        let transform = Transform{
            scale: Vector3::repeat(0.1),
            position: Vector3::new(0.0, 0.0, TILE_SIZE),
            ..Default::default()
        };

        let physical = PhysicalProperties{
            mass: 50.0,
            friction: 0.5,
            floating: false
        };

        let anatomy = Anatomy::Human(HumanAnatomy::default());

        let position = transform.position;

		let info = EntityInfo{
            player: Some(Player{name: format!("stephanie #{player_index}")}),
            transform: Some(transform),
            render: Some(RenderInfo{texture: "player/hair.png".to_owned()}),
            physical: Some(physical.into()),
            anatomy: Some(anatomy)
		};

		let inserted = self.entities.push(info);
		self.world.add_player(inserted, position.into());

		let player_info = self.player_info(stream, inserted)?;

		let (connection, messager) = self.player_create(inserted, player_info)?;

        Ok((inserted, connection, messager))
	}

	fn player_info(&self, stream: TcpStream, entity: Entity) -> Result<PlayerInfo, ConnectionError>
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

		Ok(PlayerInfo::new(MessageBuffer::new(), message_passer, entity))
	}

	fn player_create(
		&mut self,
        entity: Entity,
		player_info: PlayerInfo
	) -> Result<(usize, MessagePasser), ConnectionError>
	{
		let mut connection_handler = self.connection_handler.write();
		let connection_id = connection_handler.connect(player_info);

		let messager = connection_handler.get_mut(connection_id);

		messager.send_blocking(Message::PlayerOnConnect{entity})?;

		self.entities.entities_iter().try_for_each(|entity|
	    {
            let info = self.entities.info(entity);
            let message = Message::EntitySet{entity, info};

            messager.send_blocking(message)
		})?;

		messager.send_blocking(Message::PlayerFullyConnected)?;

		Ok((connection_id, messager.clone_messager()))
	}

	fn connection_close(&mut self, id: usize, entity: Entity)
	{
        let mut writer = self.connection_handler.write();

		self.world.remove_player(entity);

		writer.remove_connection(id);

		if let Some(player) = self.entities.player(entity)
        {
            println!("player \"{}\" disconnected", player.name);

            writer.send_message(self.entities.remove_message(entity));
        }
	}

	fn process_message_inner(&mut self, message: Message, id: usize, player: Entity)
	{
        let message = match message
        {
            Message::RepeatMessage{message} =>
            {
                self.send_message(*message);

                return;
            },
            x => x
        };

		if message.forward()
		{
			self.connection_handler.write().send_message_without(id, message.clone());
		}

		let message = match self.world.handle_message(&mut self.entities, id, player, message)
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
			x => panic!("unhandled message: {x:?}")
		}
	}

    fn send_message(&mut self, message: Message)
    {
        self.connection_handler.write().send_message(message);
    }
}

impl EntitiesController for GameServer
{
	type Container = Entities;
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
