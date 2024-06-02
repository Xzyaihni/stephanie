use std::{
    mem,
    ops::Range
};

use nalgebra::Vector3;

use serde::{Serialize, Deserialize};

use crate::common::{
    Entity,
    EntityInfo,
    entity::AnyEntities
};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WatcherType
{
    Lifetime(f32),
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
            Self::Lifetime(left) =>
            {
                *left -= dt;

                *left <= 0.0
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplodeInfo
{
    pub amount: Range<usize>,
    pub info: EntityInfo
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WatcherAction
{
    None,
    SetVisible(bool),
    Remove,
    Explode(ExplodeInfo)
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
            Self::Remove =>
            {
                entities.remove(entity);
            },
            Self::Explode(mut info) =>
            {
                let position = entities.transform(entity).unwrap().position;
                entities.remove(entity);

                let amount = fastrand::usize(info.amount);
                (0..amount).for_each(|_|
                {
                    if let Some(target) = info.info.target()
                    {
                        target.position = position;
                    }

                    entities.push(create_info, info.info.clone());
                })
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Watcher
{
    pub kind: WatcherType,
    pub action: WatcherAction
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
                actions.push(mem::take(&mut watcher.action));
            }

            !meets
        });

        actions
    }

    pub fn push(&mut self, watcher: Watcher)
    {
        self.0.push(watcher);
    }
}
