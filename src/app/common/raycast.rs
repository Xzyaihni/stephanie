use nalgebra::{Unit, Vector3};

use yanyaengine::Transform;

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

pub fn raycast_circle(
    start: &Vector3<f32>,
    direction: &Unit<Vector3<f32>>,
    transform: &Transform
) -> Option<RaycastResult>
{
    let radius = transform.max_scale() / 2.0;

    let position = transform.position;

    let offset = start - position;

    let left = direction.dot(&offset).powi(2);
    let right = offset.magnitude_squared() - radius.powi(2);

    // math ppl keep making fake letters
    let nabla = left - right;

    if nabla < 0.0
    {
        None
    } else
    {
        let sqrt_nabla = nabla.sqrt();
        let left = -(direction.dot(&offset));

        let first = left - sqrt_nabla;
        let second = left + sqrt_nabla;

        let close = first.min(second);
        let far = first.max(second);

        let pierce = far - close;

        Some(RaycastResult{distance: close, pierce})
    }
}
