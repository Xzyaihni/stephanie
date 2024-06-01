use serde::{Serialize, Deserialize};

use nalgebra::{Vector2, Vector3};

use yanyaengine::Transform;

use crate::common::Physical;


#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ColliderType
{
    Circle,
    Aabb
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ColliderLayer
{
    Normal,
    Ui
}

#[derive(Debug, Clone)]
pub struct ColliderInfo
{
    pub kind: ColliderType,
    pub layer: ColliderLayer,
    pub is_static: bool
}

impl Default for ColliderInfo
{
    fn default() -> Self
    {
        Self{
            kind: ColliderType::Circle,
            layer: ColliderLayer::Normal,
            is_static: false
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collider
{
    pub kind: ColliderType,
    pub layer: ColliderLayer,
    pub is_static: bool
}

impl From<ColliderInfo> for Collider
{
    fn from(info: ColliderInfo) -> Self
    {
        Self{
            kind: info.kind,
            layer: info.layer,
            is_static: info.is_static
        }
    }
}

pub struct CollidingInfo<'a>
{
    pub physical: Option<&'a mut Physical>,
    pub transform: &'a mut Transform,
    pub collider: Collider
}

impl<'a> CollidingInfo<'a>
{
    fn resolve_with(&mut self, other: &mut CollidingInfo, offset: Vector2<f32>)
    {
        let offset = Vector3::new(offset.x, offset.y, 0.0);

        debug_assert!(!(self.collider.is_static && other.collider.is_static));

        if self.collider.is_static
        {
            other.transform.position += offset;
            if let Some(physical) = &mut other.physical
            {
                physical.invert_velocity();
            }
        } else if other.collider.is_static
        {
            self.transform.position -= offset;
            if let Some(physical) = &mut self.physical
            {
                physical.invert_velocity();
            }
        } else
        {
            match (&mut self.physical, &mut other.physical)
            {
                (Some(this_physical), Some(other_physical)) =>
                {
                    let total_mass = this_physical.mass + other_physical.mass;

                    let left = {
                        let top = this_physical.mass - other_physical.mass;

                        top / total_mass * this_physical.velocity
                    };

                    let right = {
                        let top = other_physical.mass * 2.0;

                        top / total_mass * other_physical.velocity
                    };
                    
                    let previous_velocity = this_physical.velocity;

                    let elasticity = 0.9;

                    this_physical.velocity = (left + right) * elasticity;

                    let top = {
                        let left = this_physical.mass * (previous_velocity - this_physical.velocity);
                        
                        left + other_physical.mass * other_physical.velocity
                    };

                    other_physical.velocity = (top / other_physical.mass) * elasticity;

                    let mass_ratio = this_physical.mass / other_physical.mass;

                    let (this_scale, other_scale) = if mass_ratio >= 1.0
                    {
                        let mass_ratio = other_physical.mass / this_physical.mass;

                        (1.0 - mass_ratio, mass_ratio)
                    } else
                    {
                        (mass_ratio, 1.0 - mass_ratio)
                    };

                    self.transform.position -= offset * this_scale;
                    other.transform.position += offset * other_scale;
                },
                (Some(this_physical), None) =>
                {
                    self.transform.position -= offset;
                    this_physical.invert_velocity();
                },
                (None, Some(other_physical)) =>
                {
                    other.transform.position += offset;
                    other_physical.invert_velocity();
                },
                (None, None) =>
                {
                    let half_offset = offset / 2.0;
                    self.transform.position -= half_offset;
                    other.transform.position += half_offset;
                }
            }
        }
    }

    fn resolve_with_offset(
        &mut self,
        other: &mut CollidingInfo,
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

    fn circle_circle(&mut self, other: &mut CollidingInfo) -> bool
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

    fn circle_aabb(&mut self, other: &mut CollidingInfo) -> bool
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

    fn aabb_aabb(&mut self, other: &mut CollidingInfo) -> bool
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
        mut self,
        mut other: CollidingInfo
    ) -> bool
    {
        if self.collider.layer != other.collider.layer
        {
            return false
        }

        if self.collider.is_static && other.collider.is_static
        {
            return false;
        }

        match (self.collider.kind, other.collider.kind)
        {
            (ColliderType::Circle, ColliderType::Circle) =>
            {
                self.circle_circle(&mut other)
            },
            (ColliderType::Circle, ColliderType::Aabb) =>
            {
                self.circle_aabb(&mut other)
            },
            (ColliderType::Aabb, ColliderType::Circle) =>
            {
                other.circle_aabb(&mut self)
            },
            (ColliderType::Aabb, ColliderType::Aabb) =>
            {
                self.aabb_aabb(&mut other)
            }
        }
    }
}
