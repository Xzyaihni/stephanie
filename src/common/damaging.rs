use std::f32;

use serde::{Serialize, Deserialize};

use nalgebra::Vector3;

use yanyaengine::Transform;

use crate::common::{
    angle_between,
    damage::*,
    TILE_SIZE,
    Faction,
    Physical,
    Entity,
    player::StatId,
    raycast::RaycastInfo,
    world::TilePos
};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DamagingPredicate
{
    None,
    ParentAngleLess{angle: f32, minimum_distance: f32}
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
    Mass(DamageType),
    Collision{angle: f32, damage: DamagePartial},
    Raycast{info: RaycastInfo, damage: DamagePartial, start: Vector3<f32>, target: Vector3<f32>, scale_pierce: Option<f32>}
}

pub struct CollisionInfo
{
    pub relative_velocity: Option<Vector3<f32>>,
    pub global_rotation: f32,
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
        let global_rotation = angle_between(other.position, this.position);

        let other_velocity = other_physical.map(|x| *x.velocity()).unwrap_or_else(Vector3::zeros);
        let relative_velocity = this_physical.map(|this|
        {
            other_velocity - this.velocity()
        });

        Self{
            relative_velocity,
            global_rotation,
            relative_height: other.position.z - this.position.z
        }
    }
}

impl DamagingType
{
    pub fn as_damage(
        &self,
        collision: impl FnOnce() -> Option<CollisionInfo>
    ) -> Option<(f32, DamagePartial)>
    {
        match self
        {
            Self::None => None,
            Self::Raycast{..} => None,
            Self::Mass(damage) =>
            {
                let info = collision()?;

                let height = DamageHeight::from_z(info.relative_height);

                let kind = *damage * (info.relative_velocity?.magnitude() / TILE_SIZE);
                let damage = DamagePartial{
                    data: kind,
                    height
                };

                Some((info.global_rotation, damage))
            },
            Self::Collision{angle, damage} =>
            {
                let info = collision()?;

                Some((*angle + info.global_rotation, damage.clone()))
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
    pub same_tile_z: bool,
    pub source: Option<Entity>,
    pub knockback: f32,
    pub faction: Option<Faction>,
    pub on_hit_gain: Option<(StatId, f64)>,
    pub ranged: bool
}

impl Default for DamagingInfo
{
    fn default() -> Self
    {
        Self{
            damage: DamagingType::None,
            predicate: DamagingPredicate::None,
            times: DamageTimes::Once,
            same_tile_z: true,
            source: None,
            knockback: 1.0,
            faction: None,
            on_hit_gain: None,
            ranged: false
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DamagedId
{
    Entity(Entity),
    Tile(TilePos)
}

impl DamagedId
{
    pub fn is_tile(&self) -> bool
    {
        if let Self::Tile(_) = self
        {
            true
        } else
        {
            false
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Damaging
{
    pub damage: DamagingType,
    pub predicate: DamagingPredicate,
    pub same_tile_z: bool,
    pub faction: Faction,
    pub knockback: f32,
    pub source: Option<Entity>,
    pub ranged: bool,
    pub on_hit_gain: Option<(StatId, f64)>,
    times: DamageTimes,
    already_damaged: Vec<DamagedId>
}

impl From<DamagingInfo> for Damaging
{
    fn from(info: DamagingInfo) -> Self
    {
        Self{
            damage: info.damage,
            predicate: info.predicate,
            same_tile_z: info.same_tile_z,
            times: info.times,
            faction: info.faction.expect("faction must be specified"),
            knockback: info.knockback,
            source: info.source,
            ranged: info.ranged,
            on_hit_gain: info.on_hit_gain,
            already_damaged: Vec::new()
        }
    }
}

impl Damaging
{
    pub fn can_damage(&self, damaged: &DamagedId) -> bool
    {
        match self.times
        {
            DamageTimes::Once =>
            {
                !self.already_damaged.contains(damaged)
            }
        }
    }

    pub fn damaged(&mut self, damaged: DamagedId)
    {
        match self.times
        {
            DamageTimes::Once =>
            {
                self.already_damaged.push(damaged);
            }
        }
    }
}
