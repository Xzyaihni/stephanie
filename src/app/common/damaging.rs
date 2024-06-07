use std::f32;

use serde::{Serialize, Deserialize};

use crate::common::{Entity, Damage};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DamagingPredicate
{
    None,
    ParentAngleLess(f32)
}

impl DamagingPredicate
{
    pub fn meets(
        &self,
        parent_angle_between: impl FnOnce() -> f32
    ) -> bool
    {
        match self
        {
            Self::None => true,
            Self::ParentAngleLess(less) =>
            {
                let angle = parent_angle_between().abs();
                angle < (*less / 2.0)
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DamageTimes
{
    Once
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DamagingInfo
{
    pub damage: Damage,
    pub predicate: DamagingPredicate,
    pub times: DamageTimes,
    pub is_player: bool
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Damaging
{
    pub damage: Damage,
    pub predicate: DamagingPredicate,
    pub is_player: bool,
    times: DamageTimes,
    already_damaged: Vec<Entity>
}

impl From<DamagingInfo> for Damaging
{
    fn from(info: DamagingInfo) -> Self
    {
        Self{
            damage: info.damage,
            predicate: info.predicate,
            times: info.times,
            is_player: info.is_player,
            already_damaged: Vec::new()
        }
    }
}

impl Damaging
{
    pub fn can_damage(&self, entity: Entity) -> bool
    {
        match self.times
        {
            DamageTimes::Once =>
            {
                !self.already_damaged.contains(&entity)
            }
        }
    }

    pub fn damaged(&mut self, entity: Entity)
    {
        match self.times
        {
            DamageTimes::Once =>
            {
                self.already_damaged.push(entity);
            }
        }
    }
}
