use serde::{Serialize, Deserialize};

use nalgebra::{Vector2, Vector3};

use yanyaengine::Transform;


#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ColliderType
{
    Circle,
    Aabb
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Collider
{
    pub kind: ColliderType,
    pub is_static: bool
}

pub struct CollidingInfo<'a>
{
    pub transform: &'a mut Transform,
    pub collider: Collider
}

impl<'a> CollidingInfo<'a>
{
    fn resolve_with(self, other: CollidingInfo, offset: Vector2<f32>)
    {
        let offset = Vector3::new(offset.x, offset.y, 0.0);

        // both cant be static cuz i checked :)
        if self.collider.is_static
        {
            other.transform.position += offset;
        } else if other.collider.is_static
        {
            self.transform.position -= offset;
        } else
        {
            let half_offset = offset / 2.0;

            self.transform.position -= half_offset;
            other.transform.position += half_offset;
        }
    }

    fn circle_circle(self, other: CollidingInfo)
    {
        let this_radius = self.transform.max_scale() / 2.0;
        let other_radius = other.transform.max_scale() / 2.0;

        let offset = self.transform.position - other.transform.position;
        let distance = offset.x.hypot(offset.y);

        let max_distance = this_radius + other_radius;
        if distance < max_distance
        {
            let direction = if distance == 0.0
            {
                Vector2::new(1.0, 0.0)
            } else
            {
                offset.xy().normalize()
            };

            let shift = -(max_distance - distance);

            self.resolve_with(other, direction * shift);
        }
    }

    fn circle_aabb(self, other: CollidingInfo)
    {
        todo!()
    }

    fn aabb_aabb(self, other: CollidingInfo)
    {
        todo!()
    }

    pub fn resolve(
        self,
        other: CollidingInfo
    )
    {
        if self.collider.is_static && other.collider.is_static
        {
            return;
        }

        match (self.collider.kind, other.collider.kind)
        {
            (ColliderType::Circle, ColliderType::Circle) =>
            {
                self.circle_circle(other);
            },
            (ColliderType::Circle, ColliderType::Aabb) =>
            {
                self.circle_aabb(other);
            },
            (ColliderType::Aabb, ColliderType::Circle) =>
            {
                other.circle_aabb(self);
            },
            (ColliderType::Aabb, ColliderType::Aabb) =>
            {
                self.aabb_aabb(other);
            }
        }
    }
}
