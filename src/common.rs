use std::{
	sync::Arc,
	net::TcpStream
};

use parking_lot::RwLock;

use slab::Slab;

use nalgebra::Vector3;

use message::Message;

pub use entity::transform::{
	Transform,
	OnTransformCallback,
	TransformContainer
};

pub use entity_type::EntityType;
pub use network_entity::NetworkEntity;
pub use sender_loop::{sender_loop, BufferSender};
pub use receiver_loop::receiver_loop;

pub use tilemap::TileMap;

pub use entity::transform::ChildContainer;

use player::Player;
use entity::ChildEntity;

use physics::PhysicsEntity;

pub mod entity;
pub mod player;
pub mod character;

pub mod message;
pub mod entity_type;
pub mod network_entity;

pub mod sender_loop;
pub mod receiver_loop;

pub mod tilemap;

pub mod world;

pub mod physics;


pub trait EntityPasser
{
	fn send_single(&mut self, id: usize, message: Message);
	fn send_message(&mut self, message: Message);

	fn sync_entity(&mut self, id: EntityType, transform: Transform, velocity: Vector3<f32>)
	{
		self.send_message(Message::EntityTrasformPhysics{entity_type: id, transform, velocity});
	}
}

pub trait PlayerGet
{
	fn player(&self) -> Player;
}

pub trait EntitiesContainer
{
	type PlayerObject: TransformContainer + PlayerGet + PhysicsEntity;

	fn players_ref(&self) -> &Slab<Self::PlayerObject>;
	fn players_mut(&mut self) -> &mut Slab<Self::PlayerObject>;

	fn player_ref(&self, id: usize) -> &Self::PlayerObject
	{
		self.players_ref().get(id).unwrap()
	}

	fn player_mut(&mut self, id: usize) -> &mut Self::PlayerObject
	{
		self.players_mut().get_mut(id).unwrap()
	}

	fn empty_player(&self) -> usize
	{
		self.players_ref().vacant_key()
	}

	fn sync_entity(&mut self, id: EntityType, transform: Transform, velocity: Vector3<f32>)
	{
		match id
		{
			EntityType::Player(id) => self.player_mut(id).set_entity(transform, velocity)
		}
	}

	fn handle_message(&mut self, message: Message) -> Option<Message>
	{
		match message
		{
			Message::PlayerDestroy{id} =>
			{
				self.players_mut().remove(id);
				None
			},
			Message::EntityTrasformPhysics{entity_type, transform, velocity} =>
			{
				self.sync_entity(entity_type, transform, velocity);
				None
			},
			_ => Some(message)
		}
	}
}

pub trait EntitiesController
{
	type Container: EntitiesContainer;
	type Passer: EntityPasser;

	fn container_ref(&self) -> &Self::Container;
	fn container_mut(&mut self) -> &mut Self::Container;
	fn passer(&self) -> Arc<RwLock<Self::Passer>>;

	fn add_player(
		&mut self,
		player_associated: <Self::Container as EntitiesContainer>::PlayerObject
	) -> usize
	{
		let player = player_associated.player();
		let id = self.container_mut().players_mut().insert(player_associated);

		self.passer().write().send_message(Message::PlayerCreate{id, player});

		id
	}

	fn remove_player(&mut self, id: usize)
	{
		self.container_mut().players_mut().remove(id);
		self.passer().write().send_message(Message::PlayerDestroy{id});
	}

	fn player_mut(
		&mut self,
		id: usize
	) -> NetworkEntity<Self::Passer, <Self::Container as EntitiesContainer>::PlayerObject>
	{
		let passer = self.passer();
		let container = self.container_mut();
		NetworkEntity::new(passer, EntityType::Player(id), container.player_mut(id))
	}

	fn player_ref(&self, id: usize) -> &<Self::Container as EntitiesContainer>::PlayerObject
	{
		self.container_ref().player_ref(id)
	}
}

#[derive(Debug)]
pub struct MessagePasser
{
	stream: TcpStream
}

impl MessagePasser
{
	pub fn new(stream: TcpStream) -> Self
	{
		Self{stream}
	}

	pub fn send_one(&mut self, message: &Message) -> Result<(), bincode::Error>
	{
		self.send_many(&vec![message.clone()])
	}

	pub fn send_many(&mut self, messages: &Vec<Message>) -> Result<(), bincode::Error>
	{
		bincode::serialize_into(&mut self.stream, messages)
	}

	pub fn receive(&mut self) -> Result<Vec<Message>, bincode::Error>
	{
		bincode::deserialize_from(&mut self.stream)
	}

	pub fn receive_one(&mut self) -> Result<Option<Message>, bincode::Error>
	{
		self.receive().map(|messages| messages.into_iter().next())
	}

	pub fn try_clone(&self) -> Self
	{
		Self{stream: self.stream.try_clone().unwrap()}
	}
}