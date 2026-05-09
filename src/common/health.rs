use serde::{Serialize, Deserialize};


#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Health
{
    Normal(f32),
    InheritSibling
}
