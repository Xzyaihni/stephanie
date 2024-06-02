use nalgebra::Vector3;

use serde::{Serialize, Deserialize};

use crate::common::{
    Entity,
    entity::AnyEntities
};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WatcherType
{
    ScaleDistance{from: Vector3<f32>, near: f32}
}

impl WatcherType
{
    fn meets<E: AnyEntities>(
        &mut self,
        entities: &E,
        entity: Entity
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
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WatcherAction
{
    SetVisible(bool)
}

impl WatcherAction
{
    fn execute<E: AnyEntities>(
        &mut self,
        entities: &E,
        entity: Entity
    )
    {
        match self
        {
            Self::SetVisible(value) =>
            {
                if let Some(mut target) = entities.visible_target(entity)
                {
                    *target = *value;
                }
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

impl Watcher
{
    fn meets<E: AnyEntities>(
        &mut self,
        entities: &E,
        entity: Entity
    ) -> bool
    {
        let meets = self.kind.meets(entities, entity);

        if meets
        {
            self.action.execute(entities, entity);
        }

        meets
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
    pub fn execute<E: AnyEntities>(
        &mut self,
        entities: &E,
        entity: Entity
    )
    {
        self.0.retain_mut(|watcher| !watcher.meets(entities, entity));
    }

    pub fn push(&mut self, watcher: Watcher)
    {
        self.0.push(watcher);
    }
}
