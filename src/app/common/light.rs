use serde::{Serialize, Deserialize};


#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Light
{
    pub strength: f32
}
