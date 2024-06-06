use serde::{Serialize, Deserialize};

use yanyaengine::Transform;

use crate::common::Damage;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DamagingPredicate
{
    None,
    AngleLess(f32)
}

impl DamagingPredicate
{
    pub fn meets(&self, transform: &Transform) -> bool
    {
        match self
        {
            Self::None => true,
            Self::AngleLess(less) =>
            {
                dbg!(transform.rotation);

                false
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
