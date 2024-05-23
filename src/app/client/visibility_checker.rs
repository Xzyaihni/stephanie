use nalgebra::{Vector2, Vector3};

use yanyaengine::Transform;

use crate::common::BoundingShape;


pub struct VisibilityChecker
{
    pub size: Vector2<f32>,
    pub position: Vector3<f32>
}

impl VisibilityChecker
{
    pub fn visible(
        &self,
        shape: BoundingShape,
        transform: &Transform
    ) -> bool
    {
        let offset = (transform.position - self.position).xy();

        match shape
        {
            BoundingShape::Circle =>
            {
                let radius = transform.scale / 2.0;

                let half_size = self.size / 2.0;

                let lower = -half_size - radius.xy();
                let upper = half_size + radius.xy();

                let inbounds = |low, high, pos|
                {
                    (low..=high).contains(&pos)
                };

                inbounds(lower.x, upper.x, offset.x)
                    && inbounds(lower.y, upper.y, offset.y)
            }
        }
    }
}
