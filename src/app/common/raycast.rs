use std::{
    ops::Range,
    cmp::Ordering
};

use nalgebra::{Unit, Vector3, VectorView3, Rotation3};

use serde::{Serialize, Deserialize};

use yanyaengine::Transform;

use crate::common::{
    collider::*,
    damaging::DamagedId,
    world::{TILE_SIZE, TilePos},
    TileInfo,
    World,
    Entity,
    Pos3
};


#[derive(Debug, Clone)]
pub struct RaycastResult
{
    pub distance: f32,
    pub pierce: f32
}

impl RaycastResult
{
    pub fn is_behind(&self) -> bool
    {
        (self.distance + self.pierce) < 0.0
    }

    pub fn within_limits(&self, magnitude: f32) -> bool
    {
        self.distance <= magnitude && !self.is_behind()
    }

    pub fn hit_points(
        &self,
        start: Vector3<f32>,
        direction: Unit<Vector3<f32>>
    ) -> (Vector3<f32>, Option<Vector3<f32>>)
    {
        let first = start + (*direction * self.distance);
        let second = (self.pierce != 0.0).then(||
        {
            start + (*direction * (self.distance + self.pierce))
        });

        (first, second)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RaycastPierce
{
    None,
    Density{ignore_anatomy: bool}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaycastInfo
{
    pub pierce: Option<f32>,
    pub pierce_scale: RaycastPierce,
    pub layer: ColliderLayer,
    pub ignore_entity: Option<Entity>,
    pub ignore_end: bool
}

pub type RaycastHitId = DamagedId;

#[derive(Debug, Clone)]
pub struct RaycastHit
{
    pub id: RaycastHitId,
    pub result: RaycastResult
}

#[derive(Debug, Clone)]
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
        hit.result.hit_points(self.start, self.direction).0
    }
}

pub fn raycast_world<'a, Exit: FnMut(&TileInfo, &RaycastHit) -> bool>(
    world: &'a World,
    start: &'a Vector3<f32>,
    direction: &'a Unit<Vector3<f32>>,
    mut early_exit: Exit
) -> impl Iterator<Item=RaycastHit> + use<'a, Exit>
{
    fn inside_tile_pos(position: Vector3<f32>) -> Vector3<f32>
    {
        position.map(|x|
        {
            let m = x % TILE_SIZE;

            if m < 0.0
            {
                TILE_SIZE + m
            } else
            {
                m
            }
        })
    }

    (0..).scan((TilePos::from(Pos3::from(*start)), inside_tile_pos(*start)), move |(current_pos, current), _| -> Option<Option<RaycastHit>>
    {
        let tile = *world.tile(*current_pos)?;
        let tile_info = world.tile_info(tile);

        let is_colliding = tile_info.colliding;

        let axis_distances = current.zip_map(direction, |x, d|
        {
            if x < 0.0
            {
                if d < 0.0 { TILE_SIZE + x } else { -x }
            } else
            {
                if d < 0.0 { x } else { TILE_SIZE - x }
            }
        });

        let axis_amounts = axis_distances.component_div(direction);

        let change_index = axis_amounts.iamin();
        let change: Pos3<i32> = {
            let mut value = Vector3::repeat(0);
            value[change_index] = if direction[change_index] < 0.0 { -1 } else { 1 };

            value.into()
        };

        let step_size = axis_amounts[change_index].abs();
        let direction_change = **direction * step_size;

        let next_start = {
            let mut offset = *current + direction_change;
            offset[change_index] = if direction[change_index] < 0.0 { TILE_SIZE } else { 0.0 };

            offset
        };

        let hit = is_colliding.then(||
        {
            let id = RaycastHitId::Tile(*current_pos);

            let position = Vector3::from(current_pos.position()) + *current;

            let distance = position.metric_distance(start);
            let pierce = direction_change.magnitude();
            let result = RaycastResult{
                distance,
                pierce
            };

            RaycastHit{id, result}
        });

        if let Some(hit) = hit.as_ref()
        {
            if early_exit(tile_info, hit)
            {
                return None;
            }
        }

        *current_pos = current_pos.offset(change);
        *current = next_start;

        Some(hit)
    }).flatten()
}

pub fn raycast_this(
    start: &Vector3<f32>,
    direction: &Unit<Vector3<f32>>,
    kind: ColliderType,
    transform: &Transform
) -> Option<RaycastResult>
{
    match kind
    {
        ColliderType::RayZ => None,
        ColliderType::Tile(_) => None,
        ColliderType::Circle => raycast_circle(start, direction, transform),
        ColliderType::Aabb
        | ColliderType::Rectangle => raycast_rectangle(start, direction, transform)
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

fn ray_plane_distance(
    point: Vector3<f32>,
    direction: Unit<Vector3<f32>>,
    normal: &Unit<Vector3<f32>>,
    plane_distance: f32
) -> f32
{
    (plane_distance - point.dot(normal)) / (direction.dot(normal))
}

fn ray_slab_interval(
    point: Vector3<f32>,
    direction: Unit<Vector3<f32>>,
    normal: &Unit<Vector3<f32>>,
    plane_distance: f32
) -> Range<f32>
{
    let half_distance = plane_distance / 2.0;

    let a = ray_plane_distance(point, direction, normal, -half_distance);
    let b = ray_plane_distance(point, direction, normal, half_distance);

    Range{start: a.min(b), end: a.max(b)}
}

pub fn raycast_rectangle(
    start: &Vector3<f32>,
    direction: &Unit<Vector3<f32>>,
    transform: &Transform
) -> Option<RaycastResult>
{
    let point = start - transform.position;

    let check_axis = |axis: VectorView3<f32>, d: f32|
    {
        let axis: Vector3<f32> = axis.into();
        let axis = Unit::new_unchecked(axis);

        ray_slab_interval(point, *direction, &axis, d)
    };

    let rotation_matrix = Rotation3::from_axis_angle(
        &Vector3::z_axis(),
        transform.rotation
    );

    let rotation_matrix = rotation_matrix.matrix();

    let x = check_axis(rotation_matrix.column(0), transform.scale.x);
    let y = check_axis(rotation_matrix.column(1), transform.scale.y);
    let z = check_axis(rotation_matrix.column(2), transform.scale.z);

    let furthest_start = x.start.max(y.start).max(z.start);
    let earliest_end = x.end.min(y.end).min(z.end);

    match furthest_start.partial_cmp(&earliest_end)?
    {
        Ordering::Equal => Some(RaycastResult{distance: furthest_start, pierce: 0.0}),
        Ordering::Less =>
        {
            Some(RaycastResult{distance: furthest_start, pierce: earliest_end - furthest_start})
        },
        Ordering::Greater => None
    }
}
