use std::{
    mem,
    ops::Range
};

use nalgebra::Vector3;

use serde::{Serialize, Deserialize};

use crate::common::{
    random_rotation,
    Entity,
    EntityInfo,
    entity::AnyEntities
};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lifetime
{
    current: f32,
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
                entities.collider(entity).map(|x| x.collided().is_some()).unwrap_or(false)
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
    pub amount: Range<usize>,
    pub speed: f32,
    pub info: EntityInfo
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WatcherAction
{
    None,
    SetVisible(bool),
    SetTargetScale(Vector3<f32>),
    Remove,
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
        create_info: &mut E::CreateInfo<'_>,
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
            Self::SetTargetScale(scale) =>
            {
                if let Some(mut target) = entities.target(entity)
                {
                    target.scale = scale;
                }
            },
            Self::Remove =>
            {
                entities.remove(entity);
            },
            Self::Explode(mut info) =>
            {
                let position;
                let scale;
                {
                    let transform = entities.transform(entity).unwrap();

                    position = transform.position;
                    scale = transform.scale;
                }

                let parent_velocity = entities.physical(entity).map(|x| x.velocity);

                if !info.keep
                {
                    entities.remove(entity);
                }

                let amount = fastrand::usize(info.amount);
                (0..amount).for_each(|_|
                {
                    if let Some(target) = info.info.target()
                    {
                        let r = ||
                        {
                            2.0 * fastrand::f32()
                        };

                        let offset = scale - Vector3::new(scale.x * r(), scale.y * r(), 0.0);
                        target.position = position + offset / 2.0;
                        target.position.z = 0.0;

                        target.rotation = random_rotation();
                    }

                    if let Some(physical) = info.info.physical.as_mut()
                    {
                        let r = random_rotation();
                        let velocity = Vector3::new(r.cos(), r.sin(), 0.0) * info.speed;
                        physical.velocity = parent_velocity.unwrap_or_default() + velocity;
                        physical.velocity.z = 0.0;
                    }

                    // for now all watcher created entities r local (i might change that?)
                    entities.push(create_info, true, info.info.clone());
                })
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Watchers(Vec<Watcher>);

impl Default for Watchers
{
    fn default() -> Self
    {
        Self(Vec::new())
    }
}

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
