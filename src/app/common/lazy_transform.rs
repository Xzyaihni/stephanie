use serde::{Serialize, Deserialize};

use yanyaengine::Transform;


pub struct LazyTransformInfo
{
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LazyTransform
{
    pub target: Transform
}

impl From<LazyTransformInfo> for LazyTransform
{
    fn from(info: LazyTransformInfo) -> Self
    {
        Self{
            target: Default::default()
        }
    }
}

impl LazyTransform
{
    pub fn next(&mut self, dt: f32) -> Transform
    {
        self.target.clone()
    }
}
