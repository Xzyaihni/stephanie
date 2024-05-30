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

    fn resolve_with_offset(
        self,
        other: CollidingInfo,
        max_distance: Vector3<f32>,
        offset: Vector3<f32>
    )
    {
        let offset = max_distance.xy().zip_map(&offset.xy(), |max_distance, offset|
        {
            if offset < 0.0
            {
                -max_distance - offset
            } else
            {
                max_distance - offset
            }
        });

        let offset = if offset.x.abs() < offset.y.abs()
        {
            Vector2::new(offset.x, 0.0)
        } else
        {
            Vector2::new(0.0, offset.y)
        };

        self.resolve_with(other, offset);
    }

    fn circle_circle(self, other: CollidingInfo) -> bool
    {
        let this_radius = self.transform.max_scale() / 2.0;
        let other_radius = other.transform.max_scale() / 2.0;

        let offset = other.transform.position - self.transform.position;
        let distance = offset.x.hypot(offset.y);

        let max_distance = this_radius + other_radius;
        let collided = distance < max_distance;
        if collided
        {
            let direction = if distance == 0.0
            {
                Vector2::new(1.0, 0.0)
            } else
            {
                offset.xy().normalize()
            };

            let shift = max_distance - distance;

            self.resolve_with(other, direction * shift);
        }

        collided
    }

    fn circle_aabb(self, other: CollidingInfo) -> bool
    {
        let this_radius = self.transform.max_scale() / 2.0;
        let other_scale = other.transform.scale / 2.0;

        let offset = other.transform.position - self.transform.position;

        let max_distance = other_scale + Vector3::repeat(this_radius);
        let collided = (-max_distance.x..max_distance.x).contains(&offset.x)
            && (-max_distance.y..max_distance.y).contains(&offset.y);

        if collided
        {
            self.resolve_with_offset(other, max_distance, offset);
        }

        collided
    }

    fn aabb_aabb(self, other: CollidingInfo) -> bool
    {
        let this_scale = self.transform.scale / 2.0;
        let other_scale = other.transform.scale / 2.0;

        let offset = other.transform.position - self.transform.position;

        let max_distance = this_scale + other_scale;
        let collided = (-max_distance.x..max_distance.x).contains(&offset.x)
            && (-max_distance.y..max_distance.y).contains(&offset.y);

        if collided
        {
            self.resolve_with_offset(other, max_distance, offset);
        }

        collided
    }

    pub fn resolve(
        self,
        other: CollidingInfo
    ) -> bool
    {
        if self.collider.is_static && other.collider.is_static
        {
            return false;
        }

        match (self.collider.kind, other.collider.kind)
        {
            (ColliderType::Circle, ColliderType::Circle) =>
            {
                self.circle_circle(other)
            },
            (ColliderType::Circle, ColliderType::Aabb) =>
            {
                self.circle_aabb(other)
            },
            (ColliderType::Aabb, ColliderType::Circle) =>
            {
                other.circle_aabb(self)
            },
            (ColliderType::Aabb, ColliderType::Aabb) =>
            {
                self.aabb_aabb(other)
            }
        }
    }
}
