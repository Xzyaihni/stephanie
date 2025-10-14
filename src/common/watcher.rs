use nalgebra::Vector3;

use serde::{Serialize, Deserialize};

use crate::common::{
    short_rotation,
    render_info::*,
    particle_creator::*,
    lazy_transform::*,
    Entity,
    EntityInfo,
    Collider,
    Occluder,
    Item,
    entity::AnyEntities
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
    pub fn meets<E: AnyEntities>(
        &mut self,
        entities: &E,
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

#[derive(Debug, Clone)]
pub enum WatcherAction
{
    None,
    AddWatcher(Box<Watcher>),
    OutlineableDisable,
    SetVisible(bool),
    SetMixColor(Option<MixColor>),
    SetItem(Option<Box<Item>>),
    SetCollider(Option<Box<Collider>>),
    SetOccluder(Option<Occluder>),
    SetTargetPosition(Vector3<f32>),
    SetTargetScale(Vector3<f32>),
    SetTargetRotation(f32),
    SetLazyRotation(Rotation),
    SetLazyConnection(Connection),
    Remove,
    Create(Box<EntityInfo>),
    Explode(Box<ExplodeInfo>)
}

impl Default for WatcherAction
{
    fn default() -> Self
    {
        Self::None
    }
}

impl WatcherAction
{
    pub fn execute<E: AnyEntities>(
        self,
        entities: &mut E,
        entity: Entity
    )
    {
        match self
        {
            Self::None => (),
            Self::AddWatcher(watcher) =>
            {
                entities.add_watcher(entity, *watcher);
            },
            Self::OutlineableDisable =>
            {
                entities.set_outlined(entity, false);
            },
            Self::SetVisible(value) =>
            {
                if let Some(mut target) = entities.visible_target(entity)
                {
                    *target = value;
                }
            },
            Self::SetMixColor(value) =>
            {
                if let Some(mut target) = entities.mix_color_target(entity)
                {
                    *target = value;
                }
            },
            Self::SetItem(value) =>
            {
                entities.set_item(entity, value.map(|x| *x));
            },
            Self::SetCollider(value) =>
            {
                entities.set_collider(entity, value.map(|x| *x));
            },
            Self::SetOccluder(value) =>
            {
                entities.set_occluder(entity, value);
            },
            Self::SetTargetPosition(position) =>
            {
                if let Some(mut target) = entities.target(entity)
                {
                    target.position = position;
                }
            },
            Self::SetTargetScale(scale) =>
            {
                if let Some(mut target) = entities.target(entity)
                {
                    target.scale = scale;
                }
            },
            Self::SetTargetRotation(rotation) =>
            {
                if let Some(mut target) = entities.target(entity)
                {
                    target.rotation = rotation;
                }
            },
            Self::SetLazyRotation(rotation) =>
            {
                if let Some(mut lazy) = entities.lazy_transform_mut(entity)
                {
                    lazy.rotation = rotation;
                }
            },
            Self::SetLazyConnection(connection) =>
            {
                if let Some(mut lazy) = entities.lazy_transform_mut(entity)
                {
                    lazy.connection = connection;
                }
            },
            Self::Remove =>
            {
                entities.remove(entity);
            },
            Self::Create(info) =>
            {
                entities.push_eager(true, *info);
            },
            Self::Explode(info) =>
            {
                ParticleCreator::create_particles(
                    entities,
                    entity,
                    info.info,
                    info.prototype
                );

                if !info.keep
                {
                    entities.remove(entity);
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Watcher
{
    pub kind: WatcherType,
    pub action: WatcherAction,
    pub id: Option<WatcherId>,
    pub persistent: bool
}

impl Default for Watcher
{
    fn default() -> Self
    {
        Self{
            kind: WatcherType::Instant,
            action: WatcherAction::None,
            id: None,
            persistent: false
        }
    }
}

impl Watcher
{
    pub fn simple_disappearing(lifetime: f32) -> Self
    {
        Self{
            kind: WatcherType::Lifetime(lifetime.into()),
            action: WatcherAction::Remove,
            ..Default::default()
        }
    }

    pub fn simple_one_frame() -> Self
    {
        Self{
            kind: WatcherType::Frames(1.into()),
            action: WatcherAction::Remove,
            ..Default::default()
        }
    }
}
