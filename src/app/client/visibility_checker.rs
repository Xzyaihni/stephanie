use std::ops::Range;

use nalgebra::{Unit, Vector2, Vector3};

use yanyaengine::Transform;

use crate::common::{
    project_onto,
    raycast::*,
    render_info::BoundingShape
};


pub struct VisibilityChecker
{
    pub size: Vector2<f32>,
    pub position: Vector3<f32>
}

impl VisibilityChecker
{
    fn visible_sphere_radius(&self, position: Vector3<f32>, radius: f32) -> bool
    {
        let offset = position - self.position;

        let half_size = Vector3::new(self.size.x, self.size.y, Self::z_size()) / 2.0;

        let lower = -half_size - Vector3::repeat(radius);
        let upper = half_size + Vector3::repeat(radius);

        (0..3).all(|i|
        {
            (*lower.index(i)..=*upper.index(i)).contains(offset.index(i))
        })
    }

    #[allow(dead_code)]
    fn visible_point(&self, point: Vector3<f32>) -> bool
    {
        self.visible_sphere_radius(point, 0.0)
    }

    fn visible_sphere(&self, transform: &Transform) -> bool
    {
        let radius = transform.max_scale() / 2.0;

        self.visible_sphere_radius(transform.position, radius)
    }

    fn z_height() -> Range<f32>
    {
        -1.0..1.0
    }

    fn z_size() -> f32
    {
        let z_height = Self::z_height();

        z_height.end - z_height.start
    }

    pub fn visible_occluding_plane(&self, transform: &Transform) -> bool
    {
        let start = project_onto(transform, &Vector3::new(-0.5, 0.0, 0.0));
        let end = project_onto(transform, &Vector3::new(0.5, 0.0, 0.0));

        let diff = end - start;
        let magnitude = diff.magnitude();

        let direction = Unit::new_unchecked(diff / magnitude);

        let rectangle = Transform{
            position: self.position,
            scale: Vector3::new(self.size.x, self.size.y, Self::z_size()),
            ..Default::default()
        };

        if let Some(result) = raycast_rectangle(&start, &direction, &rectangle)
        {
            result.within_limits(magnitude)
        } else
        {
            false
        }
    }

    pub fn visible(
        &self,
        shape: BoundingShape,
        transform: &Transform
    ) -> bool
    {
        match shape
        {
            BoundingShape::Circle =>
            {
                self.visible_sphere(transform)
            }
        }
    }
}
