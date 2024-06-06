use serde::{Serialize, Deserialize};

use crate::common::Damage;


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
                let angle = parent_angle_between();
                angle.abs() < (*less / 2.0)
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
pub struct Damaging
{
    pub damage: Damage,
    pub predicate: DamagingPredicate,
    pub times: DamageTimes,
    pub is_player: bool
}
