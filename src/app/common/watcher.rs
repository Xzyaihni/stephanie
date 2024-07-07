use std::mem;

use nalgebra::Vector3;

use serde::{Serialize, Deserialize};

use crate::common::{
    render_info::*,
    particle_creator::*,
    lazy_transform::*,
    Entity,
    EntityInfo,
    entity::AnyEntities
};


#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WatcherType
{
    Instant,
    Collision,
    Lifetime(Lifetime),
    ScaleDistance{from: Vector3<f32>, near: f32}
}

impl WatcherType
{
    fn meets<E: AnyEntities>(
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
                entities.collider(entity).map(|x| !x.collided().is_empty()).unwrap_or(false)
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
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplodeInfo
{
    pub keep: bool,
    pub info: ParticlesInfo,
    pub prototype: EntityInfo
}

// i can just add a closure variant that doesnt get serialized but meh
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WatcherAction
{
    None,
    SetVisible(bool),
    SetMixColor(Option<MixColor>),
    SetTargetPosition(Vector3<f32>),
    SetTargetScale(Vector3<f32>),
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Watcher
{
    pub kind: WatcherType,
    pub action: WatcherAction,
    pub persistent: bool
}

impl Default for Watcher
{
    fn default() -> Self
    {
        Self{
            kind: WatcherType::Instant,
            action: WatcherAction::None,
            persistent: false
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Watchers(Vec<Watcher>);

impl Watchers
{
    pub fn new(watchers: Vec<Watcher>) -> Self
    {
        Self(watchers)
    }

    pub fn execute<E: AnyEntities>(
        &mut self,
        entities: &E,
        entity: Entity,
        dt: f32
    ) -> Vec<WatcherAction>
    {
        let mut actions = Vec::new();
        self.0.retain_mut(|watcher|
        {
            let meets = watcher.kind.meets(entities, entity, dt);

            if meets
            {
                let replacement = if watcher.persistent
                {
                    watcher.action.clone()
                } else
                {
                    Default::default()
                };

                actions.push(mem::replace(&mut watcher.action, replacement));
            }

            if watcher.persistent
            {
                true
            } else
            {
                !meets
            }
        });

        actions
    }

    pub fn push(&mut self, watcher: Watcher)
    {
        self.0.push(watcher);
    }
}
