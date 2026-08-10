use serde::{Serialize, Deserialize};

use crate::common::Entity;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Health
{
    Normal(f32),
    Shielded{health: Option<Box<Self>>, shield: Entity},
    InheritSibling
}
