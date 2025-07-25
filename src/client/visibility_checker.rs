use nalgebra::{Unit, Vector2, Vector3};

use yanyaengine::Transform;

use crate::common::{
    project_onto,
    raycast::*,
    world::TilePos
};


#[derive(Debug, Clone)]
pub struct VisibilityChecker
{
    pub world_position: TilePos,
    pub size: Vector3<f32>,
    pub position: Vector3<f32>
}

impl VisibilityChecker
{
    fn visible_sphere_radius(&self, position: Vector3<f32>, radius: f32) -> bool
    {
        let offset = position - self.position;

        let half_size = self.size / 2.0;

        let limit = half_size + Vector3::new(radius, radius, 0.0);

        (0..3).all(|i|
        {
            offset.index(i).abs() <= *limit.index(i)
        })
    }

    pub fn visible_point(&self, point: Vector3<f32>) -> bool
    {
        self.visible_sphere_radius(point, 0.0)
    }

    pub fn visible_point_2d(&self, point: Vector2<f32>) -> bool
    {
        let offset = point - self.position.xy();

        let half_size = self.size / 2.0;

        (0..2).all(|i|
        {
            offset.index(i).abs() <= *half_size.index(i)
        })
    }

    pub fn visible_sphere(&self, transform: &Transform) -> bool
    {
        let radius = transform.max_scale() / 2.0;

        self.visible_sphere_radius(transform.position, radius)
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
            scale: self.size,
            ..Default::default()
        };

        if let Some(result) = raycast_rectangle(start, direction, &rectangle)
        {
            result.within_limits(magnitude)
        } else
        {
            false
        }
    }
}
