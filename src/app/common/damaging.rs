use std::f32;

use serde::{Serialize, Deserialize};

use nalgebra::Vector3;

use yanyaengine::Transform;

use crate::common::{short_rotation, angle_between, damage::*, Physical, Side2d, Entity};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DamagingPredicate
{
    None,
    ParentAngleLess(f32)
}

impl DamagingPredicate
{
    pub fn meets(
        &self,
        parent_angle_between: impl FnOnce() -> f32
    ) -> bool
    {
        match self
        {
            Self::None => true,
            Self::ParentAngleLess(less) =>
            {
                let angle = parent_angle_between().abs();
                angle < (*less / 2.0)
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DamageTimes
{
    Once
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DamagingType
{
    None,
    Mass(f32),
    Damage{angle: f32, damage: Damage}
}

pub struct CollisionInfo
{
    pub relative_velocity: Option<Vector3<f32>>,
    pub global_rotation: f32,
    pub relative_rotation: f32,
    pub relative_height: f32
}

impl CollisionInfo
{
    pub fn new(
        this: &Transform,
        other: &Transform,
        this_physical: Option<&Physical>,
        other_physical: Option<&Physical>
    ) -> Self
    {
        let global_rotation = short_rotation(angle_between(this.position, other.position));

        let relative_velocity = other_physical.zip(this_physical).map(|(other, this)|
        {
            other.velocity - this.velocity
        });

        Self{
            relative_velocity,
            global_rotation,
            relative_rotation: short_rotation(global_rotation - other.rotation),
            relative_height: other.position.z - this.position.z
        }
    }
}

impl DamagingType
{
    pub fn as_damage(
        &self,
        collision: impl FnOnce() -> Option<CollisionInfo>
    ) -> Option<(f32, Damage)>
    {
        match self
        {
            Self::None => None,
            Self::Mass(mass) =>
            {
                let info = collision()?;

                let force = info.relative_velocity? * *mass;

                let side = Side2d::from_angle(info.relative_rotation);
                let height = DamageHeight::from_z(info.relative_height);

                let direction = DamageDirection{
                    side,
                    height
                };

                let kind = DamageType::Blunt(force.magnitude() * 100.0);
                let damage = Damage::new(direction, kind);

                Some((info.global_rotation, damage))
            },
            Self::Damage{angle, damage} =>
            {
                let info = collision()?;

                Some((info.global_rotation + *angle, damage.clone()))
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DamagingInfo
{
    pub damage: DamagingType,
    pub predicate: DamagingPredicate,
    pub times: DamageTimes,
    pub is_player: bool
}

impl Default for DamagingInfo
{
    fn default() -> Self
    {
        Self{
            damage: DamagingType::None,
            predicate: DamagingPredicate::None,
            times: DamageTimes::Once,
            is_player: false
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Damaging
{
    pub damage: DamagingType,
    pub predicate: DamagingPredicate,
    pub is_player: bool,
    times: DamageTimes,
    already_damaged: Vec<Entity>
}

impl From<DamagingInfo> for Damaging
{
    fn from(info: DamagingInfo) -> Self
    {
        Self{
            damage: info.damage,
            predicate: info.predicate,
            times: info.times,
            is_player: info.is_player,
            already_damaged: Vec::new()
        }
    }
}

impl Damaging
{
    pub fn can_damage(&self, entity: Entity) -> bool
    {
        match self.times
        {
            DamageTimes::Once =>
            {
                !self.already_damaged.contains(&entity)
            }
        }
    }

    pub fn damaged(&mut self, entity: Entity)
    {
        match self.times
        {
            DamageTimes::Once =>
            {
                self.already_damaged.push(entity);
            }
        }
    }
}
