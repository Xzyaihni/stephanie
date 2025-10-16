use nalgebra::Vector3;

use serde::{Serialize, Deserialize};

use crate::common::{
    short_rotation,
    some_or_return,
    particle_creator::*,
    Entity,
    EntityInfo,
    entity::ClientEntities
};


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatcherId
{
    Outline
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lifetime
{
    pub current: f32,
    start: f32
}

impl From<f32> for Lifetime
{
    fn from(start: f32) -> Self
    {
        Self{
            current: start,
            start
        }
    }
}

impl Lifetime
{
    pub fn fraction(&self) -> f32
    {
        self.current / self.start
    }

    pub fn reset(&mut self)
    {
        self.current = self.start;
    }
}

#[derive(Debug, Clone)]
pub struct Frames
{
    current: u32,
    max: u32
}

impl From<u32> for Frames
{
    fn from(value: u32) -> Self
    {
        Self{current: value, max: value}
    }
}

#[derive(Debug, Clone)]
pub enum WatcherType
{
    Instant,
    Collision,
    Lifetime(Lifetime),
    Frames(Frames),
    RotationDistance{from: f32, near: f32},
    ScaleDistance{from: Vector3<f32>, near: f32}
}

impl WatcherType
{
    pub fn meets(
        &mut self,
        entities: &ClientEntities,
        entity: Entity,
        dt: f32
    ) -> bool
    {
        match self
        {
            Self::Instant => true,
            Self::Collision =>
            {
                entities.collider(entity)
                    .map(|x| !x.collided().is_empty() || !x.collided_tiles().is_empty())
                    .unwrap_or(false)
            },
            Self::RotationDistance{from, near} =>
            {
                if let Some(transform) = entities.transform(entity)
                {
                    short_rotation(transform.rotation - *from).abs() < *near
                } else
                {
                    false
                }
            },
            Self::ScaleDistance{from, near} =>
            {
                if let Some(transform) = entities.transform(entity)
                {
                    transform.scale.metric_distance(from) < *near
                } else
                {
                    false
                }
            },
            Self::Lifetime(lifetime) =>
            {
                lifetime.current -= dt;

                let meets = lifetime.current <= 0.0;

                if meets
                {
                    lifetime.reset();
                }

                meets
            },
            Self::Frames(left) =>
            {
                left.current -= 1;

                let meets = left.current == 0;

                if meets
                {
                    left.current = left.max;
                }

                meets
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExplodeInfo
{
    pub keep: bool,
    pub info: ParticlesInfo,
    pub prototype: EntityInfo
}

pub type ActionType = Box<dyn FnOnce(&mut ClientEntities, Entity)>;

pub struct Watcher
{
    pub kind: WatcherType,
    pub action: ActionType,
    pub id: Option<WatcherId>
}

impl Default for Watcher
{
    fn default() -> Self
    {
        Self{
            kind: WatcherType::Instant,
            action: Box::new(|_, _| {}),
            id: None
        }
    }
}

impl Watcher
{
    pub fn simple_disappearing(lifetime: f32) -> Self
    {
        Self{
            kind: WatcherType::Lifetime(lifetime.into()),
            action: Box::new(|entities, entity| entities.remove(entity)),
            ..Default::default()
        }
    }

    pub fn simple_one_frame() -> Self
    {
        Self{
            kind: WatcherType::Frames(1.into()),
            action: Box::new(|entities, entity| entities.remove(entity)),
            ..Default::default()
        }
    }

    pub fn explode_action(info: ExplodeInfo) -> ActionType
    {
        Box::new(move |entities, entity|
        {
            let transform = some_or_return!(entities.transform(entity)).clone();

            let parent_velocity = entities.physical(entity).map(|x| *x.velocity()).unwrap_or_default();

            let physical = info.prototype.physical.map(|mut physical|
            {
                physical.add_velocity_raw(parent_velocity);

                physical
            });

            let parent_scale = transform.scale;

            let entity_info = EntityInfo{
                transform: Some(transform),
                physical,
                ..info.prototype
            };

            create_particles(
                &mut *entities,
                info.info,
                entity_info,
                parent_scale
            );

            if !info.keep
            {
                entities.remove(entity);
            }
        })
    }
}
