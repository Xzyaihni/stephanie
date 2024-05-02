use std::{
    fmt::Debug,
	sync::Arc,
	net::TcpStream,
    borrow::Borrow,
    ops::{Range, RangeInclusive}
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
    ChildEntity,
    Physical,
    ChildContainer,
    EntityProperties,
    PhysicalProperties,
    EntityContainer
};

pub use chunk_saver::{SaveLoad, WorldChunkSaver, ChunkSaver, EntitiesSaver};

pub use anatomy::{Anatomy, HumanAnatomy};
pub use damage::{Damageable, Damage, DamageType, DamageDirection, Side2d, DamageHeight};

pub use enemy_builder::EnemyBuilder;

pub use character::CharacterProperties;
pub use player::PlayerProperties;
pub use enemy::{EnemyProperties, Enemy};

use player::Player;

pub use physics::PhysicsEntity;

pub mod animator;

pub mod lisp;
pub mod objects_store;

pub mod damage;
pub mod anatomy;

pub mod enemy_builder;

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

pub struct WeightedPicker<I>
{
    total: f64,
    values: I
}

impl<I> WeightedPicker<I>
where
    I: IntoIterator + Clone,
    I::Item: Copy
{
    pub fn new(total: f64, values: I) -> Self
    {
        Self{total, values}
    }

    pub fn pick_from(
        random_value: f64,
        values: I,
        get_weight: impl Fn(I::Item) -> f64
    ) -> Option<I::Item>
    {
        let total = values.clone().into_iter().map(|value| get_weight(value)).sum();

        Self::new(total, values).pick_with(random_value, get_weight)
    }

    pub fn pick_with(
        &self,
        random_value: f64,
        get_weight: impl Fn(I::Item) -> f64
    ) -> Option<I::Item>
    {
        let mut random_value = random_value * self.total;

        self.values.clone().into_iter().find(|value|
        {
            let weight = get_weight(*value);
            random_value -= weight;

            random_value <= 0.0
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeededRandom(u64);

impl From<u64> for SeededRandom
{
    fn from(value: u64) -> Self
    {
        Self(value)
    }
}

impl SeededRandom
{
    pub fn new() -> Self
    {
        Self(fastrand::u64(0..u64::MAX))
    }

    pub fn set_state(&mut self, value: u64)
    {
        self.0 = value;
    }

    // splitmix64 by sebastiano vigna
    pub fn next_u64(&mut self) -> u64
    {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);

        let x = self.0;

        let x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        let x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);

        x ^ (x >> 31)
    }

    pub fn next_u64_between(&mut self, range: Range<u64>) -> u64
    {
        let difference = range.end - range.start;

        range.start + self.next_u64() % difference
    }

    pub fn next_usize_between(&mut self, range: Range<usize>) -> usize
    {
        let difference = range.end - range.start;

        range.start + (self.next_u64() as usize) % difference
    }

    pub fn next_f32(&mut self) -> f32
    {
        let x = self.next_u64();

        x as f32 / u64::MAX as f32
    }

    pub fn next_f64(&mut self) -> f64
    {
        let x = self.next_u64();

        x as f64 / u64::MAX as f64
    }

    pub fn next_f32_between(&mut self, range: RangeInclusive<f32>) -> f32
    {
        let x = self.next_f32();

        let size = range.end() - range.start();

        range.start() + x * size
    }

    pub fn next_bool(&mut self) -> bool
    {
        self.next_u64() % 2 == 0
    }
}

pub fn lerp(x: f32, y: f32, a: f32) -> f32
{
    (1.0 - a) * x + y * a
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EntityAny<PlayerType=Player, EnemyType=Enemy>
{
    Player(PlayerType),
    Enemy(EnemyType)
}

impl EntityContainer for EntityAny
{
    fn entity_ref(&self) -> &Entity
    {
        match self
        {
            Self::Player(x) => x.entity_ref(),
            Self::Enemy(x) => x.entity_ref()
        }
    }

    fn entity_mut(&mut self) -> &mut Entity
    {
        match self
        {
            Self::Player(x) => x.entity_mut(),
            Self::Enemy(x) => x.entity_mut()
        }
    }
}

impl EntityAny
{
    pub fn is_player(&self) -> bool
    {
        match self
        {
            Self::Player(_) => true,
            _ => false
        }
    }
}

pub trait EntityAnyWrappable
{
    fn wrap_any(self) -> EntityAny;
}

pub trait EntityPasser
{
	fn send_single(&mut self, id: usize, message: Message);
	fn send_message(&mut self, message: Message);

    fn sync_entity(&mut self, id: EntityType, entity: EntityAny)
    {
        self.send_message(Message::EntitySet{id, entity});
    }

	fn sync_transform(&mut self, id: EntityType, transform: Transform)
	{
        let message = Message::EntitySyncTransform{entity_type: id, transform};

		self.send_message(message);
	}
}

pub trait EntitiesContainer
{
	type PlayerObject: TransformContainer
        + Debug
        + Borrow<Player>
        + Damageable
        + PhysicsEntity;

	type EnemyObject: TransformContainer
        + Debug
        + Borrow<Enemy>
        + Damageable
        + PhysicsEntity;

	fn players_ref(&self) -> &ObjectsStore<Self::PlayerObject>;
	fn players_mut(&mut self) -> &mut ObjectsStore<Self::PlayerObject>;

	fn enemies_ref(&self) -> &ObjectsStore<Self::EnemyObject>;
	fn enemies_mut(&mut self) -> &mut ObjectsStore<Self::EnemyObject>;

    fn push(
        &mut self,
        entity: EntityAny<Self::PlayerObject, Self::EnemyObject>
    ) -> EntityType
    {
        match entity
        {
            EntityAny::Player(entity) =>
            {
                let id = self.players_mut().push(entity);

                EntityType::Player(id)
            },
            EntityAny::Enemy(entity) =>
            {
                let id = self.enemies_mut().push(entity);

                EntityType::Enemy(id)
            }
        }
    }

    fn insert(
        &mut self,
        id: EntityType,
        entity: EntityAny<Self::PlayerObject, Self::EnemyObject>
    )
    {
        match (id, entity)
        {
            (EntityType::Player(id), EntityAny::Player(entity)) =>
            {
                self.players_mut().insert(id, entity);
            },
            (EntityType::Enemy(id), EntityAny::Enemy(entity)) =>
            {
                self.enemies_mut().insert(id, entity);
            },
            x => panic!("unhandled message: {x:?}")
        }
    }

    fn remove(&mut self, id: EntityType)
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
    }

    fn damage(&mut self, id: EntityType, damage: Damage)
    {
        match id
        {
            EntityType::Player(id) =>
            {
                self.players_mut()[id].damage(damage);
            },
            EntityType::Enemy(id) =>
            {
                self.enemies_mut()[id].damage(damage);
            }
        }
    }

    fn get_entity_ref(&self, id: EntityType) -> &Entity
    {
        match id
        {
            EntityType::Player(id) =>
            {
                self.players_ref()[id].borrow().entity_ref()
            },
            EntityType::Enemy(id) =>
            {
                self.enemies_ref()[id].borrow().entity_ref()
            }
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
                self.remove(id);
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
		let entity = EntityAny::Player(player_associated.borrow().clone());

        let raw_id = self.container_mut().players_mut().push(player_associated);
		let id = EntityType::Player(raw_id);

		self.passer().write().sync_entity(id, entity);

		raw_id
	}

	fn add_enemy(
		&mut self,
		enemy_associated: <Self::Container as EntitiesContainer>::EnemyObject
	) -> usize
	{
		let entity = EntityAny::Enemy(enemy_associated.borrow().clone());

		let raw_id = self.container_mut().enemies_mut().push(enemy_associated);
        let id = EntityType::Enemy(raw_id);

		self.passer().write().sync_entity(id, entity);

		raw_id
	}

	fn remove_player(&mut self, id: usize)
	{
		self.container_mut().players_mut().remove(id);

        let id = EntityType::Player(id);
		self.passer().write().send_message(Message::EntityDestroy{id});
	}

	fn player_mut(
		&mut self,
		id: usize
	) -> NetworkEntity<'_, Self::Passer, <Self::Container as EntitiesContainer>::PlayerObject>
	{
		let passer = self.passer();
		let container = self.container_mut();

		NetworkEntity::new(passer, EntityType::Player(id), &mut container.players_mut()[id])
	}

	fn player_ref(
        &self,
        id: usize
    ) -> &<Self::Container as EntitiesContainer>::PlayerObject
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
