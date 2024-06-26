use serde::{Serialize, Deserialize};

use nalgebra::Vector3;

use yanyaengine::Transform;

use crate::common::{
    define_layers,
    group_by,
    Entity,
    Physical,
    world::{
        TILE_SIZE,
        TilePos,
        Pos3,
        World
    }
};


#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ColliderType
{
    Point,
    Circle,
    Aabb
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ColliderLayer
{
    Normal,
    Damage,
    Ui,
    World
}

impl ColliderLayer
{
    pub fn collides(&self, other: &Self) -> bool
    {
        define_layers!{
            self, other,
            (Normal, Normal, true),
            (Ui, Ui, true),
            (Normal, Ui, false),
            (Damage, Damage, false),
            (Damage, Normal, true),
            (Damage, Ui, false),
            (World, World, false),
            (World, Normal, true),
            (World, Damage, true),
            (World, Ui, false)
        }
    }
}

#[derive(Debug, Clone)]
pub struct ColliderInfo
{
    pub kind: ColliderType,
    pub layer: ColliderLayer,
    pub ghost: bool,
    pub move_z: bool,
    pub is_static: bool
}

impl Default for ColliderInfo
{
    fn default() -> Self
    {
        Self{
            kind: ColliderType::Circle,
            layer: ColliderLayer::Normal,
            ghost: false,
            move_z: true,
            is_static: false
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collider
{
    pub kind: ColliderType,
    pub layer: ColliderLayer,
    pub ghost: bool,
    pub move_z: bool,
    pub is_static: bool,
    collided: Vec<Entity>
}

impl From<ColliderInfo> for Collider
{
    fn from(info: ColliderInfo) -> Self
    {
        Self{
            kind: info.kind,
            layer: info.layer,
            ghost: info.ghost,
            move_z: info.move_z,
            is_static: info.is_static,
            collided: Vec::new()
        }
    }
}

impl Collider
{
    pub fn collided(&self) -> &[Entity]
    {
        &self.collided
    }

    pub fn push_collided(&mut self, entity: Entity)
    {
        self.collided.push(entity);
    }

    pub fn reset_frame(&mut self)
    {
        self.collided.clear();
    }
}

pub struct CollisionResult
{
    max_distance: Vector3<f32>,
    offset: Vector3<f32>
}

pub struct CircleCollisionResult
{
    max_distance: f32,
    distance: f32,
    offset: Vector3<f32>
}

pub struct CollidingInfo<'a, F>
{
    pub entity: Option<Entity>,
    pub physical: Option<&'a mut Physical>,
    pub target: F,
    pub transform: Transform,
    pub collider: &'a mut Collider
}

fn transform_target(
    move_z: bool,
    target: impl FnOnce(Vector3<f32>)
) -> impl FnOnce(Vector3<f32>)
{
    let handle_z = move |mut values: Vector3<f32>|
    {
        if !move_z
        {
            values.z = 0.0;
        }

        values
    };

    move |offset: Vector3<f32>| target(handle_z(offset))
}

impl<'a, ThisF> CollidingInfo<'a, ThisF>
where
    ThisF: FnMut(Vector3<f32>)
{
    // i hate rust traits so goddamn much >-< haskell ones r so much better
    fn resolve_with<OtherF>(
        &mut self,
        other: &mut CollidingInfo<OtherF>,
        offset: Vector3<f32>
    )
    where
        OtherF: FnMut(Vector3<f32>)
    {
        if self.collider.is_static && other.collider.is_static
        {
            return;
        }

        if self.collider.ghost || other.collider.ghost
        {
            return;
        }

        let this_target = transform_target(self.collider.move_z, &mut self.target);
        let other_target = transform_target(other.collider.move_z, &mut other.target);

        let elasticity = 0.9;

        let invert_some = |physical: &mut Physical|
        {
            let moved = offset.map(|x| x != 0.0);

            let new_velocity = -physical.velocity * elasticity;

            if moved.x { physical.velocity.x = new_velocity.x }
            if moved.y { physical.velocity.y = new_velocity.y }
            if moved.z { physical.velocity.z = new_velocity.z }
        };

        if self.collider.is_static
        {
            other_target(offset);
            if let Some(physical) = &mut other.physical
            {
                invert_some(physical);
            }
        } else if other.collider.is_static
        {
            this_target(-offset);
            if let Some(physical) = &mut self.physical
            {
                invert_some(physical);
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

                    this_target(-offset * this_scale);
                    other_target(offset * other_scale);
                },
                (Some(this_physical), None) =>
                {
                    this_target(-offset);
                    invert_some(this_physical);
                },
                (None, Some(other_physical)) =>
                {
                    other_target(offset);
                    invert_some(other_physical);
                },
                (None, None) =>
                {
                    let half_offset = offset / 2.0;
                    this_target(-half_offset);
                    other_target(half_offset);
                }
            }
        }
    }

    fn resolve_with_offset<OtherF>(
        &mut self,
        other: &mut CollidingInfo<OtherF>,
        max_distance: Vector3<f32>,
        offset: Vector3<f32>
    )
    where
        OtherF: FnMut(Vector3<f32>)
    {
        let offset = max_distance.zip_map(&offset, |max_distance, offset|
        {
            if offset < 0.0
            {
                -max_distance - offset
            } else
            {
                max_distance - offset
            }
        });

        let abs_offset = offset.map(|x| x.abs());

        let offset = if (abs_offset.z <= abs_offset.x) && (abs_offset.z <= abs_offset.y)
        {
            Vector3::new(0.0, 0.0, offset.z)
        } else if (abs_offset.y <= abs_offset.x) && (abs_offset.y <= abs_offset.z)
        {
            Vector3::new(0.0, offset.y, 0.0)
        } else
        {
            Vector3::new(offset.x, 0.0, 0.0)
        };

        self.resolve_with(other, offset);
    }

    fn circle_circle<OtherF>(
        &self,
        other: &CollidingInfo<OtherF>
    ) -> Option<CircleCollisionResult>
    where
        OtherF: FnMut(Vector3<f32>)
    {
        let this_radius = self.transform.max_scale() / 2.0;
        let other_radius = other.transform.max_scale() / 2.0;

        let offset = other.transform.position - self.transform.position;
        let distance = (offset.x.powi(2) + offset.y.powi(2) + offset.z.powi(2)).sqrt();

        let max_distance = this_radius + other_radius;
        let collided = distance < max_distance;

        collided.then(|| CircleCollisionResult{max_distance, distance, offset})
    }

    fn normal_collision<OtherF>(
        &self,
        other: &CollidingInfo<OtherF>
    ) -> Option<CollisionResult>
    where
        OtherF: FnMut(Vector3<f32>)
    {
        let this_scale = self.scale();
        let other_scale = other.scale();

        let offset = other.transform.position - self.transform.position;

        let max_distance = other_scale + this_scale;
        let collided = (-max_distance.x..max_distance.x).contains(&offset.x)
            && (-max_distance.y..max_distance.y).contains(&offset.y)
            && (-max_distance.z..max_distance.z).contains(&offset.z);

        collided.then(|| CollisionResult{max_distance, offset})
    }

    fn scale(&self) -> Vector3<f32>
    {
        match self.collider.kind
        {
            ColliderType::Point =>
            {
                let mut size = Vector3::zeros();

                size.z = self.transform.scale.z / 2.0;

                size
            },
            ColliderType::Circle => Vector3::repeat(self.transform.max_scale() / 2.0),
            ColliderType::Aabb => self.transform.scale / 2.0
        }
    }

    fn collision<OtherF>(
        &self,
        other: &CollidingInfo<OtherF>
    ) -> Option<impl FnOnce(&mut Self, &mut CollidingInfo<OtherF>)>
    where
        OtherF: FnMut(Vector3<f32>)
    {
        enum CollisionWhich
        {
            Circle(CircleCollisionResult),
            Normal(CollisionResult)
        }

        if !self.collider.layer.collides(&other.collider.layer)
        {
            return None;
        }

        let handle = |collision|
        {
            move |this: &mut Self, other: &mut CollidingInfo<OtherF>|
            {
                match collision
                {
                    CollisionWhich::Circle(CircleCollisionResult{max_distance, distance, offset}) =>
                    {
                        let direction = if distance == 0.0
                        {
                            Vector3::x()
                        } else
                        {
                            offset.normalize()
                        };

                        let shift = max_distance - distance;

                        this.resolve_with(other, direction * shift);
                    },
                    CollisionWhich::Normal(CollisionResult{max_distance, offset}) =>
                    {
                        this.resolve_with_offset(other, max_distance, offset);
                    }
                }
            }
        };

        match (self.collider.kind, other.collider.kind)
        {
            (ColliderType::Point, ColliderType::Point) => None,
            (ColliderType::Circle, ColliderType::Circle) =>
            {
                self.circle_circle(other).map(|x| CollisionWhich::Circle(x)).map(handle)
            },
            (ColliderType::Circle, ColliderType::Aabb)
            | (ColliderType::Aabb, ColliderType::Circle)
            | (ColliderType::Aabb, ColliderType::Aabb)
            | (ColliderType::Point, ColliderType::Aabb)
            | (ColliderType::Aabb, ColliderType::Point)
            | (ColliderType::Point, ColliderType::Circle)
            | (ColliderType::Circle, ColliderType::Point) =>
            {
                self.normal_collision(other).map(|x| CollisionWhich::Normal(x)).map(handle)
            }
        }
    }

    pub fn resolve<OtherF>(
        &mut self,
        mut other: CollidingInfo<OtherF>
    ) -> bool
    where
        OtherF: FnMut(Vector3<f32>)
    {
        let result = self.collision(&other);
        let collided = result.is_some();

        if let Some(handle) = result
        {
            handle(self, &mut other);
        }

        if collided
        {
            if let Some(other) = other.entity
            {
                self.collider.push_collided(other);
            }

            if let Some(entity) = self.entity
            {
                other.collider.push_collided(entity);
            }
        }

        collided
    }

    pub fn resolve_with_world(
        &mut self,
        world: &World
    ) -> bool
    {
        let tile_of = |pos: Vector3<f32>|
        {
            world.tile_of(pos.into())
        };

        let size = self.scale();

        let pos = self.transform.position;

        let start_tile = tile_of(pos - size);
        let end_tile = tile_of(pos + size);

        let tile_pos = |tile: TilePos| -> Vector3<f32>
        {
            (tile.position() + Pos3::repeat(TILE_SIZE / 2.0)).into()
        };

        let mut collider = ColliderInfo{
            kind: ColliderType::Aabb,
            layer: ColliderLayer::World,
            ghost: false,
            move_z: false,
            is_static: true
        }.into();

        let collisions = start_tile.tiles_between(end_tile).filter(|tile|
        {
            let empty_tile = world.tile(*tile).map(|x| x.is_none()).unwrap_or(false);

            !empty_tile
        }).map(tile_pos).filter_map(|position|
        {
            self.collision(&CollidingInfo{
                entity: None,
                physical: None,
                target: |_| {},
                transform: Transform{
                    position,
                    scale: Vector3::repeat(TILE_SIZE),
                    ..Default::default()
                },
                collider: &mut collider
            }).map(|_| position)
        });


        let collisions = group_by(|group, value|
        {
            group.iter().all(|check| value.x == check.x && value.y == check.y)
            || group.iter().all(|check| value.x == check.x && value.z == check.z)
            || group.iter().all(|check| value.y == check.y && value.z == check.z)
        }, collisions);

        let collided = !collisions.is_empty();
        collisions.into_iter().for_each(|group|
        {
            let amount = group.len() as f32;

            let total_position = group.into_iter().reduce(|acc, x| acc + x)
                .expect("must have at least one element");

            let collision_point = total_position / amount;

            let mut other = CollidingInfo{
                entity: None,
                physical: None,
                target: |_| {},
                transform: Transform{
                    position: collision_point,
                    scale: Vector3::repeat(TILE_SIZE),
                    ..Default::default()
                },
                collider: &mut collider
            };

            let result = self.collision(&other);

            if let Some(resolve) = result
            {
                resolve(self, &mut other);
            }
        });

        collided
    }
}
