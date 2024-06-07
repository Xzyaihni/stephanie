use std::{
    f32,
    fmt::Debug,
    sync::Arc,
    net::TcpStream,
    ops::{Range, RangeInclusive}
};

use serde::{Serialize, Deserialize};

use parking_lot::RwLock;

use message::Message;

pub use yanyaengine::{Transform, TransformContainer};

pub use objects_store::ObjectsStore;

pub use sender_loop::{sender_loop, BufferSender};
pub use receiver_loop::receiver_loop;

pub use tilemap::{TileMap, TileMapWithTextures};

pub use lazy_transform::{
    LazyTransform,
    LazyTransformInfo,
    LazyTargettable
};

pub use entity::{
    AnyEntities,
    ServerToClient,
    Component,
    Parent,
    Entity,
    EntityInfo,
    ClientEntityInfo,
    Entities
};

pub use sides::{Side1d, Side2d, Side3d};

pub use loot::Loot;
pub use item::Item;
pub use items_info::{ItemInfo, ItemsInfo, Ranged};

pub use inventory::{InventorySorter, InventoryItem, Inventory};

pub use player::{Player, PlayerEntities};

pub use enemy::{EnemyBehavior, Enemy};
pub use enemy_builder::EnemyBuilder;
pub use furniture_builder::FurnitureBuilder;
pub use enemies_info::{EnemyId, EnemyInfo, EnemiesInfo};

pub use chunk_saver::{SaveLoad, WorldChunkSaver, ChunkSaver, EntitiesSaver};

pub use render_info::RenderInfo;

pub use anatomy::{Anatomy, HumanAnatomy};
pub use damage::{Damageable, Damage, DamageType, DamageDirection, DamageHeight};

pub use collider::{ColliderType, Collider, CollidingInfo};
pub use physics::{Physical, PhysicalProperties};

pub mod sides;
pub mod lisp;
pub mod objects_store;

pub mod render_info;

pub mod damaging;
pub mod damage;
pub mod anatomy;

pub mod watcher;
pub mod lazy_transform;
pub mod entity;

pub mod loot;
pub mod item;
pub mod items_info;

pub mod inventory;

pub mod player;

pub mod particle_creator;
pub mod furniture_builder;

pub mod enemy;
pub mod enemy_builder;
pub mod enemies_info;

pub mod message;

pub mod sender_loop;
pub mod receiver_loop;

pub mod tilemap;

pub mod chunk_saver;
pub mod world;

pub mod collider;
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

pub const ENTITY_SCALE: f32 = 0.1;

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
        let total = values.clone().into_iter().map(&get_weight).sum();

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

pub fn random_rotation() -> f32
{
    fastrand::f32() * (f32::consts::PI * 2.0)
}

pub fn short_rotation(rotation: f32) -> f32
{
    let rotation = rotation % (f32::consts::PI * 2.0);

    if rotation > f32::consts::PI
    {
        rotation - 2.0 * f32::consts::PI
    } else if rotation < -f32::consts::PI
    {
        rotation + 2.0 * f32::consts::PI
    } else
    {
        rotation
    }
}

pub fn angle_between(a: &Transform, b: &Transform) -> f32
{
    let offset = b.position - a.position;

    let a_angle = -a.rotation;
    let angle_between = offset.y.atan2(-offset.x);

    let relative_angle = angle_between + (f32::consts::PI - a_angle);

    short_rotation(relative_angle)
}

// thanks freya holmer
pub fn ease_out(current: f32, target: f32, decay: f32, dt: f32) -> f32
{
    target + (current - target) * (-decay * dt).exp()
}

pub fn lerp(x: f32, y: f32, a: f32) -> f32
{
    (1.0 - a) * x + y * a
}

pub fn get_two_mut<T>(s: &mut [T], one: usize, two: usize) -> (&mut T, &mut T)
{
    if one > two
    {
        let (left, right) = s.split_at_mut(one);

        (&mut right[0], &mut left[two])
    } else
    {
        let (left, right) = s.split_at_mut(two);

        (&mut left[one], &mut right[0])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionId(pub usize);

pub trait EntityPasser
{
    fn send_single(&mut self, id: ConnectionId, message: Message);
    fn send_message(&mut self, message: Message);
}

pub trait EntitiesController
{
    type Container;
    type Passer: EntityPasser;

    fn container_ref(&self) -> &Self::Container;
    fn container_mut(&mut self) -> &mut Self::Container;
    fn passer(&self) -> Arc<RwLock<Self::Passer>>;
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
        if messages.is_empty()
        {
            return Ok(());
        }

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
