use nalgebra::{Unit, Vector3};

use crate::common::{collider::*, Entity};


pub struct RaycastResult
{
    pub distance: f32,
    pub pierce: f32
}

pub struct RaycastInfo
{
    pub pierce: Option<f32>,
    pub layer: ColliderLayer,
    pub ignore_entity: Option<Entity>,
    pub ignore_end: bool
}

#[derive(Debug)]
pub enum RaycastHitId
{
    Entity(Entity),
    // later
    Tile
}

#[derive(Debug)]
pub struct RaycastHit
{
    pub id: RaycastHitId,
    pub distance: f32,
    pub width: f32
}

#[derive(Debug)]
pub struct RaycastHits
{
    pub start: Vector3<f32>,
    pub direction: Unit<Vector3<f32>>,
    pub hits: Vec<RaycastHit>
}

impl RaycastHits
{
    pub fn hit_position(&self, hit: &RaycastHit) -> Vector3<f32>
    {
        self.start + self.direction.into_inner() * hit.distance
    }
}
