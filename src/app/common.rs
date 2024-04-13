use std::{
	sync::Arc,
	net::TcpStream
};

use serde::{Serialize, Deserialize};

use parking_lot::RwLock;

use message::Message;

pub use yanyaengine::{Transform, TransformContainer};

pub use objects_store::ObjectsStore;

pub use entity_type::EntityType;
pub use network_entity::NetworkEntity;
pub use sender_loop::{sender_loop, BufferSender};
pub use receiver_loop::receiver_loop;

pub use tilemap::{TileMap, TileMapWithTextures};

pub use entity::{
    Entity,
    Physical,
    ChildContainer,
    EntityProperties,
    PhysicalProperties
};

pub use chunk_saver::{SaveLoad, WorldChunkSaver, ChunkSaver};

pub use character::CharacterProperties;
pub use player::PlayerProperties;
pub use enemy::{EnemyProperties, Enemy};

use player::Player;
use entity::ChildEntity;

use physics::PhysicsEntity;

pub mod lisp;
pub mod objects_store;

pub mod entity;
pub mod player;
pub mod enemy;
pub mod character;

pub mod message;
pub mod entity_type;
pub mod network_entity;

pub mod sender_loop;
pub mod receiver_loop;

pub mod tilemap;

pub mod chunk_saver;
pub mod world;

pub mod physics;


#[macro_export]
macro_rules! time_this
{
    ($name:expr, $($tt:tt),*) =>
    {
        {
            use std::time::Instant;

            let start_time = Instant::now();

            $($tt)*

            eprintln!("{} took {} ms", $name, start_time.elapsed().as_millis());
        }
    }
}

pub fn lerp(x: f32, y: f32, a: f32) -> f32
{
    (1.0 - a) * x + y * a
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EntityAny
{
    Player(Player),
    Enemy(Enemy)
}

pub trait EntityPasser
{
	fn send_single(&mut self, id: usize, message: Message);
	fn send_message(&mut self, message: Message);

	fn sync_transform(&mut self, id: EntityType, transform: Transform)
	{
        let message = Message::EntitySyncTransform{entity_type: id, transform};

		self.send_message(message);
	}
}

pub trait GettableInner<I, T>
{
    fn wrap(info: I, value: T) -> Self;
	fn get_inner(&self) -> T;
}

pub trait EntitiesContainer<I>
{
	type PlayerObject: TransformContainer + GettableInner<I, Player> + PhysicsEntity;
	type EnemyObject: TransformContainer + GettableInner<I, Enemy> + PhysicsEntity;

	fn players_ref(&self) -> &ObjectsStore<Self::PlayerObject>;
	fn players_mut(&mut self) -> &mut ObjectsStore<Self::PlayerObject>;

	fn enemies_ref(&self) -> &ObjectsStore<Self::EnemyObject>;
	fn enemies_mut(&mut self) -> &mut ObjectsStore<Self::EnemyObject>;

    fn insert(&mut self, id: EntityType, info: I, entity: EntityAny)
    {
        match (id, entity)
        {
            (EntityType::Player(id), EntityAny::Player(entity)) =>
            {
                self.players_mut().insert(id, Self::PlayerObject::wrap(info, entity));
            },
            (EntityType::Enemy(id), EntityAny::Enemy(entity)) =>
            {
                self.enemies_mut().insert(id, Self::EnemyObject::wrap(info, entity));
            },
            x => panic!("unhandled message: {x:?}")
        }
    }

	fn empty_player(&self) -> usize
	{
		self.players_ref().vacant_key()
	}

	fn empty_enemy(&self) -> usize
	{
		self.enemies_ref().vacant_key()
	}

	fn sync_transform(&mut self, id: EntityType, other: Transform)
	{
		match id
		{
			EntityType::Player(id) => self.players_mut()[id].sync_transform(other),
			EntityType::Enemy(id) => self.enemies_mut()[id].sync_transform(other)
		}
	}

	fn handle_message(&mut self, message: Message) -> Option<Message>
	{
		match message
		{
			Message::EntityDestroy{id} =>
			{
                match id
                {
				    EntityType::Player(id) =>
                    {
                        self.players_mut().remove(id);
                    },
				    EntityType::Enemy(id) =>
                    {
                        self.enemies_mut().remove(id);
                    }
                }

				None
			},
			Message::EntitySyncTransform{entity_type, transform} =>
			{
				self.sync_transform(entity_type, transform);
				None
			},
			_ => Some(message)
		}
	}
}

pub trait EntitiesController<I>
{
	type Container: EntitiesContainer<I>;
	type Passer: EntityPasser;

	fn container_ref(&self) -> &Self::Container;
	fn container_mut(&mut self) -> &mut Self::Container;
	fn passer(&self) -> Arc<RwLock<Self::Passer>>;

	fn add_player(
		&mut self,
		player_associated: <Self::Container as EntitiesContainer<I>>::PlayerObject
	) -> usize
	{
		let entity = EntityAny::Player(player_associated.get_inner());

        let raw_id = self.container_mut().players_mut().push(player_associated);
		let id = EntityType::Player(raw_id);

		self.passer().write().send_message(Message::EntityCreate{id, entity});

		raw_id
	}

	fn add_enemy(
		&mut self,
		enemy_associated: <Self::Container as EntitiesContainer<I>>::EnemyObject
	) -> usize
	{
		let entity = EntityAny::Enemy(enemy_associated.get_inner());

		let raw_id = self.container_mut().enemies_mut().push(enemy_associated);
        let id = EntityType::Enemy(raw_id);

		self.passer().write().send_message(Message::EntityCreate{id, entity});

		raw_id
	}

	fn remove_player(&mut self, id: usize)
	{
		self.container_mut().players_mut().remove(id);

        let id = EntityType::Player(id);
		self.passer().write().send_message(Message::EntityDestroy{id});
	}

	fn player_mut<'a>(
		&'a mut self,
		id: usize
	) -> NetworkEntity<'a, Self::Passer, <Self::Container as EntitiesContainer<I>>::PlayerObject>
    where
        Self::Container: 'a
	{
		let passer = self.passer();
		let container = self.container_mut();

		NetworkEntity::new(passer, EntityType::Player(id), &mut container.players_mut()[id])
	}

	fn player_ref<'a>(&'a self, id: usize) -> &<Self::Container as EntitiesContainer<I>>::PlayerObject
    where
        Self::Container: 'a
	{
		&self.container_ref().players_ref()[id]
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
