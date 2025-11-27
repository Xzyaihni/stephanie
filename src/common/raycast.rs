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
    Ignore,
    Density{ignore_anatomy: bool}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaycastInfo
{
    pub pierce: Option<f32>,
    pub pierce_scale: RaycastPierce,
    pub scale: f32,
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

pub fn swept_aabb_world_with_before<'a>(
    world: &'a World,
    this: &'a Transform,
    direction: Vector3<f32>
) -> impl Iterator<Item=(TilePos, f32)> + use<'a>
{
    swept_aabb_world_inner::<false>(world, this, direction)
}

pub fn swept_aabb_world(
    world: &World,
    this: &Transform,
    direction: Vector3<f32>
) -> Option<(TilePos, f32)>
{
    swept_aabb_world_inner::<true>(world, this, direction).min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
}

pub fn swept_aabb_world_collides(
    world: &World,
    this: &Transform,
    direction: Vector3<f32>
) -> bool
{
    swept_aabb_world_inner::<true>(world, this, direction).next().is_some()
}

fn swept_aabb_world_inner<'a, const EXCLUDE_BEFORE: bool>(
    world: &'a World,
    this: &'a Transform,
    direction: Vector3<f32>
) -> impl Iterator<Item=(TilePos, f32)> + use<'a, EXCLUDE_BEFORE>
{
    let tilemap = world.tilemap();

    let start = this.position;
    let end = start + direction;

    let limit = direction.magnitude();

    let direction = Unit::new_normalize(direction);

    let half_size = this.scale * 0.5;

    let top_left = TilePos::from(start.zip_map(&end, |a, b| a.min(b)) - half_size);
    let bottom_right = TilePos::from(start.zip_map(&end, |a, b| a.max(b)) + half_size);

    let size = this.scale + Vector3::repeat(TILE_SIZE);

    top_left.tiles_between(bottom_right).filter_map(move |pos|
    {
        let tile = world.tile(pos);

        let is_colliding = tile.map(|tile| tilemap[*tile].colliding).unwrap_or(false);

        if !is_colliding
        {
            return None;
        }

        let other = Transform{
            scale: size,
            position: pos.entity_position(),
            ..Default::default()
        };

        raycast_rectangle(start, direction, &other).map(|x|
        {
            (pos, x.distance)
        }).filter(|(_, x)|
        {
            if EXCLUDE_BEFORE && *x < (TILE_SIZE * -0.1)
            {
                return false;
            }

            *x <= limit
        })
    })
}

pub fn raycast_world<'a, Exit: FnMut(&TileInfo, &TilePos, &RaycastResult) -> bool>(
    world: &'a World,
    start: Vector3<f32>,
    direction: Unit<Vector3<f32>>,
    mut early_exit: Exit
) -> impl Iterator<Item=(&'a TileInfo, TilePos, RaycastResult)> + use<'a, Exit>
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

    let direction_inv = direction.map(|x| x.recip());

    (0..).scan((TilePos::from(Pos3::from(start)), inside_tile_pos(start)), move |(current_pos, current), _| -> Option<Option<_>>
    {
        let tile = *world.tile(*current_pos)?;
        let tile_info = world.tile_info(tile);

        let is_colliding = tile_info.colliding;

        let axis_distances = current.zip_map(&direction, |x, d|
        {
            if x < 0.0
            {
                if d < 0.0 { TILE_SIZE + x } else { -x }
            } else
            {
                if d < 0.0 { x } else { TILE_SIZE - x }
            }
        });

        let axis_amounts = axis_distances.component_mul(&direction_inv);

        let change_index = axis_amounts.iamin();
        let change: Pos3<i32> = {
            let mut value = Vector3::repeat(0);
            value[change_index] = if direction[change_index] < 0.0 { -1 } else { 1 };

            value.into()
        };

        let step_size = axis_amounts[change_index].abs();
        let direction_change = *direction * step_size;

        let next_start = {
            let mut offset = *current + direction_change;
            offset[change_index] = if direction[change_index] < 0.0 { TILE_SIZE } else { 0.0 };

            offset
        };

        let hit = is_colliding.then(||
        {
            let position = Vector3::from(current_pos.position()) + *current;

            let distance = position.metric_distance(&start);
            let pierce = direction_change.magnitude();
            let result = RaycastResult{
                distance,
                pierce
            };

            (tile_info, *current_pos, result)
        });

        if let Some(hit) = hit.as_ref()
        {
            if early_exit(hit.0, &hit.1, &hit.2)
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
    start: Vector3<f32>,
    direction: Unit<Vector3<f32>>,
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
    start: Vector3<f32>,
    direction: Unit<Vector3<f32>>,
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

fn line_rectangle_intersections(
    start: Vector3<f32>,
    direction: Unit<Vector3<f32>>,
    transform: &Transform
) -> Vector3<Range<f32>>
{
    let point = start - transform.position;

    let check_axis = |axis: VectorView3<f32>, d: f32|
    {
        let axis: Vector3<f32> = axis.into();
        let axis = Unit::new_unchecked(axis);

        ray_slab_interval(point, direction, &axis, d)
    };

    let rotation_matrix = Rotation3::from_axis_angle(
        &Vector3::z_axis(),
        transform.rotation
    );

    let rotation_matrix = rotation_matrix.matrix();

    let mut axes = (0..3).map(|i| check_axis(rotation_matrix.column(i), transform.scale[i]));

    let mut n = ||
    {
        axes.next().unwrap_or_else(|| unreachable!())
    };

    Vector3::new(n(), n(), n())
}

pub fn raycast_rectangle(
    start: Vector3<f32>,
    direction: Unit<Vector3<f32>>,
    transform: &Transform
) -> Option<RaycastResult>
{
    let intersections = line_rectangle_intersections(start, direction, transform);
    let [x, y, z] = intersections.as_ref();

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

pub fn swept_aabb_vs_aabb(
    this: &Transform,
    direction: Unit<Vector3<f32>>,
    other: &Transform
) -> Option<RaycastResult>
{
    let start = this.position;
    let other = Transform{
        scale: other.scale + this.scale,
        ..other.clone()
    };

    raycast_rectangle(start, direction, &other)
}
