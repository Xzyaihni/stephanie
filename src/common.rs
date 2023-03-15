use std::{
	sync::Arc,
	net::TcpStream
};

use parking_lot::RwLock;

use slab::Slab;

use message::Message;

pub use entity::transform::{Transform, TransformContainer};
pub use entity_type::EntityType;
pub use network_entity::NetworkEntity;
pub use sender_loop::{sender_loop, BufferSender};

pub mod entity;

pub mod player;
pub mod character;

pub mod message;
pub mod entity_type;
pub mod network_entity;

pub mod sender_loop;


pub trait EntityPasser
{
	fn send_message(&mut self, message: Message);

	fn sync_transform(&mut self, id: EntityType, transform: Transform)
	{
		self.send_message(Message::EntityTransform{entity: id, transform});
	}
}

pub trait EntitiesContainer
{
	type PlayerObject: TransformContainer;

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

	fn sync_transform(&mut self, id: EntityType, transform: Transform)
	{
		match id
		{
			EntityType::Player(id) => self.player_mut(id).set_transform(transform)
		}
	}

	fn handle_message(&mut self, message: Message) -> Option<Message>
	{
		match message
		{
			Message::EntityTransform{entity, transform} =>
			{
				self.sync_transform(entity, transform.clone());
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

	pub fn send(&mut self, message: &Message)
	{
		bincode::serialize_into(&mut self.stream, message).unwrap();
	}

	pub fn receive(&mut self) -> Message
	{
		bincode::deserialize_from(&mut self.stream).unwrap()
	}

	pub fn try_clone(&self) -> Self
	{
		Self{stream: self.stream.try_clone().unwrap()}
	}
}