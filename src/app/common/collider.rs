use std::cmp::Ordering;

use serde::{Serialize, Deserialize};

use nalgebra::{Vector2, Vector3};

use yanyaengine::Transform;

use crate::common::{
    define_layers,
    define_layers_enum,
    point_line_side,
    point_line_distance,
    rectangle_points,
    Entity,
    Physical,
    world::{
        TILE_SIZE,
        Axis,
        World
    }
};


#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColliderType
{
    Point,
    Circle,
    Aabb,
    Rectangle
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ColliderLayer
{
    Normal,
    Damage,
    Ui,
    World,
    Door,
    Mouse
}

impl ColliderLayer
{
    pub fn collides(&self, other: &Self) -> bool
    {
        define_layers!{
            self, other,
            (Ui, Ui, true),

            (Normal, Normal, true),
            (Normal, Ui, false),

            (Damage, Damage, false),
            (Damage, Normal, true),
            (Damage, Ui, false),

            (World, World, false),
            (World, Normal, true),
            (World, Damage, true),
            (World, Ui, false),

            (Mouse, Mouse, false),
            (Mouse, Normal, true),
            (Mouse, Damage, false),
            (Mouse, Ui, true),
            (Mouse, World, false),

            (Door, Door, true),
            (Door, Normal, true),
            (Door, Damage, true),
            (Door, Ui, false),
            (Door, World, false),
            (Door, Mouse, false)
        }
    }
}

#[derive(Debug, Clone)]
pub struct ColliderInfo
{
    pub kind: ColliderType,
    pub layer: ColliderLayer,
    pub ghost: bool,
    pub scale: Option<Vector3<f32>>,
    pub move_z: bool,
    pub target_non_lazy: bool,
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
            scale: None,
            move_z: true,
            target_non_lazy: false,
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
    pub scale: Option<Vector3<f32>>,
    pub move_z: bool,
    pub target_non_lazy: bool,
    pub is_static: bool,
    collided: Vec<Entity>,
    previous_position: Option<Vector3<f32>>
}

impl From<ColliderInfo> for Collider
{
    fn from(info: ColliderInfo) -> Self
    {
        Self{
            kind: info.kind,
            layer: info.layer,
            ghost: info.ghost,
            scale: info.scale,
            move_z: info.move_z,
            target_non_lazy: info.target_non_lazy,
            is_static: info.is_static,
            collided: Vec::new(),
            previous_position: None
        }
    }
}

impl Collider
{
    pub fn save_previous(&mut self, position: Vector3<f32>)
    {
        self.previous_position = Some(position);
    }

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

pub struct RectangleCollisionResult
{

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

#[derive(Debug)]
pub struct BasicCollidingInfo<'a>
{
    pub transform: Transform,
    pub collider: &'a mut Collider
}

impl<'a> BasicCollidingInfo<'a>
{
    fn circle_circle(
        &self,
        other: &Self
    ) -> Option<CircleCollisionResult>
    {
        let this_radius = self.transform.max_scale() / 2.0;
        let other_radius = other.transform.max_scale() / 2.0;

        let offset = other.transform.position - self.transform.position;
        let distance = (offset.x.powi(2) + offset.y.powi(2) + offset.z.powi(2)).sqrt();

        let max_distance = this_radius + other_radius;
        let collided = distance < max_distance;

        collided.then_some(CircleCollisionResult{max_distance, distance, offset})
    }

    fn normal_collision(
        &self,
        other: &Self
    ) -> Option<CollisionResult>
    {
        let this_scale = self.scale();
        let other_scale = other.scale();

        let offset = other.transform.position - self.transform.position;

        let max_distance = other_scale + this_scale;
        let collided = (-max_distance.x..max_distance.x).contains(&offset.x)
            && (-max_distance.y..max_distance.y).contains(&offset.y)
            && (-max_distance.z..max_distance.z).contains(&offset.z);

        collided.then_some(CollisionResult{max_distance, offset})
    }

    fn rectangle_rectangle(
        &self,
        other: &Self
    ) -> Option<RectangleCollisionResult>
    {
        let points_outside = |points: [Vector2<f32>; 4], (a, b)| -> bool
        {
            let mapped = points.map(|point| point_line_side(point, a, b));

            mapped == [Ordering::Less; 4]
                || mapped == [Ordering::Greater; 4]
        };

        let other_points@[
            other_a,
            other_b,
            _other_c,
            other_d
        ] = rectangle_points(&other.transform);

        let this_points@[
            this_a,
            this_b,
            _this_c,
            this_d
        ] = rectangle_points(&self.transform);

        let colliding = !points_outside(other_points, (this_a, this_b))
            && !points_outside(other_points, (this_a, this_d))
            && !points_outside(this_points, (other_a, other_b))
            && !points_outside(this_points, (other_a, other_d));

        colliding.then_some(RectangleCollisionResult{})
    }

    fn inside_rectangle(p: Vector2<f32>, a: Vector2<f32>, b: Vector2<f32>, d: Vector2<f32>) -> bool
    {
        let inside = move |a, b|
        {
            point_line_side(p, a, b) == Ordering::Equal
        };

        inside(a, b) && inside(a, d)
    }

    fn rectangle_point(
        &self,
        other: &Self
    ) -> Option<RectangleCollisionResult>
    {
        let [a, b, _c, d] = rectangle_points(&self.transform);

        let p = other.transform.position.xy();
        (Self::inside_rectangle(p, a, b, d)).then_some(RectangleCollisionResult{})
    }

    fn rectangle_circle(
        &self,
        other: &Self
    ) -> Option<RectangleCollisionResult>
    {
        let [a, b, c, d] = rectangle_points(&self.transform);

        let circle_scale = other.transform.max_scale() / 2.0;

        let p = other.transform.position.xy();

        let inside_circle = |a, b| -> bool
        {
            point_line_distance(p, a, b) <= circle_scale
        };

        (Self::inside_rectangle(p, a, b, d)
            || inside_circle(a, b)
            || inside_circle(b, c)
            || inside_circle(c, d)
            || inside_circle(d, a)).then_some(RectangleCollisionResult{})
    }

    pub fn scale(&self) -> Vector3<f32>
    {
        let scale = match self.collider.kind
        {
            ColliderType::Point =>
            {
                let mut size = Vector3::zeros();

                size.z = self.transform.scale.z / 2.0;

                size
            },
            ColliderType::Circle => Vector3::repeat(self.transform.max_scale() / 2.0),
            ColliderType::Aabb
            | ColliderType::Rectangle => self.transform.scale / 2.0
        };

        if let Some(additional_scale) = self.collider.scale
        {
            scale.component_mul(&additional_scale)
        } else
        {
            scale
        }
    }

    fn collision<ThisF, OtherF>(
        &self,
        other: &Self
    ) -> Option<impl FnOnce(
            &mut CollidingInfo<ThisF>,
            &mut CollidingInfo<OtherF>,
            Option<Axis>
        ) -> (Option<Vector3<f32>>, Option<Vector3<f32>>)>
    where
        ThisF: FnMut(Vector3<f32>, Option<f32>) -> Vector3<f32>,
        OtherF: FnMut(Vector3<f32>, Option<f32>) -> Vector3<f32>
    {
        enum CollisionWhich
        {
            Circle(CircleCollisionResult),
            Normal(CollisionResult),
            Rectangle(RectangleCollisionResult)
        }

        if !self.collider.layer.collides(&other.collider.layer)
        {
            return None;
        }

        let handle = |collision|
        {
            move |this: &mut CollidingInfo<ThisF>, other: &mut CollidingInfo<OtherF>, axis: Option<Axis>|
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

                        this.resolve_with(other, direction * shift)
                    },
                    CollisionWhich::Normal(CollisionResult{max_distance, offset}) =>
                    {
                        this.resolve_with_offset(other, max_distance, offset, axis)
                    },
                    CollisionWhich::Rectangle(_) =>
                    {
                        fn h<F: FnMut(Vector3<f32>, Option<f32>) -> Vector3<f32>>(this: &mut CollidingInfo<F>)
                        {
                            if this.basic.collider.kind == ColliderType::Rectangle
                            {
                                (this.target)(Default::default(), Some(0.01));
                            }
                        }

                        h(this);
                        h(other);

                        let a = "reminder";

                        (None, None)
                    }
                }
            }
        };

        let normal_collision = ||
        {
            self.normal_collision(other).map(CollisionWhich::Normal).map(handle)
        };

        let rectangle_collision = ||
        {
            self.rectangle_rectangle(other).map(CollisionWhich::Rectangle).map(handle)
        };

        define_layers_enum!{
            self.collider.kind, other.collider.kind,
            ColliderType,

            (Point, Point, None),

            (Circle, Circle, self.circle_circle(other).map(CollisionWhich::Circle).map(handle)),
            (Circle, Point, normal_collision()),

            (Aabb, Aabb, normal_collision()),
            (Aabb, Point, normal_collision()),
            (Aabb, Circle, normal_collision()),

            (Rectangle, Rectangle, rectangle_collision()),
            (Rectangle, Aabb, rectangle_collision())
            (order_dependent, Rectangle, Point, self.rectangle_point(other).map(CollisionWhich::Rectangle).map(handle)),
            (order_dependent, Point, Rectangle, other.rectangle_point(self).map(CollisionWhich::Rectangle).map(handle)),
            (order_dependent, Rectangle, Circle, self.rectangle_circle(other).map(CollisionWhich::Rectangle).map(handle)),
            (order_dependent, Circle, Rectangle, other.rectangle_circle(self).map(CollisionWhich::Rectangle).map(handle))
        }
    }

    pub fn is_colliding(&self, other: &Self) -> bool
    {
        self.collision::<fn(_, _) -> _, fn(_, _) -> _>(other).is_some()
    }
}

#[derive(Debug)]
pub struct CollidingInfo<'a, F>
{
    pub entity: Option<Entity>,
    pub physical: Option<&'a mut Physical>,
    pub target: F,
    pub basic: BasicCollidingInfo<'a>
}

impl<'a, ThisF> CollidingInfo<'a, ThisF>
where
    ThisF: FnMut(Vector3<f32>, Option<f32>) -> Vector3<f32>
{
    fn resolve_with<OtherF>(
        &mut self,
        other: &mut CollidingInfo<OtherF>,
        offset: Vector3<f32>
    ) -> (Option<Vector3<f32>>, Option<Vector3<f32>>)
    where
        OtherF: FnMut(Vector3<f32>, Option<f32>) -> Vector3<f32>
    {
        fn transform_target(
            move_z: bool,
            target: impl FnOnce(Vector3<f32>, Option<f32>) -> Vector3<f32>
        ) -> impl FnOnce(Vector3<f32>) -> Vector3<f32>
        {
            let handle_z = move |mut values: Vector3<f32>|
            {
                if !move_z
                {
                    values.z = 0.0;
                }

                values
            };

            let add_epsilon = |values: Vector3<f32>|
            {
                const EPSILON: f32 = 0.0002;

                values.map(|x| if x == 0.0 { x } else { x + x.signum() * EPSILON })
            };

            move |offset: Vector3<f32>| target(add_epsilon(handle_z(offset)), None)
        }

        if self.basic.collider.is_static && other.basic.collider.is_static
        {
            return (None, None);
        }

        if self.basic.collider.ghost || other.basic.collider.ghost
        {
            return (None, None);
        }

        let this_target = transform_target(self.basic.collider.move_z, &mut self.target);
        let other_target = transform_target(other.basic.collider.move_z, &mut other.target);

        let elasticity = 0.5;

        let invert_some = |physical: &mut Physical|
        {
            let moved = offset.map(|x| x != 0.0);

            let new_velocity = -physical.velocity * elasticity;

            if moved.x { physical.velocity.x = new_velocity.x }
            if moved.y { physical.velocity.y = new_velocity.y }
            if moved.z { physical.velocity.z = new_velocity.z }
        };

        if self.basic.collider.is_static
        {
            let other_position = other_target(offset);
            if let Some(physical) = &mut other.physical
            {
                invert_some(physical);
            }

            (None, Some(other_position))
        } else if other.basic.collider.is_static
        {
            let this = this_target(-offset);
            if let Some(physical) = &mut self.physical
            {
                invert_some(physical);
            }

            (Some(this), None)
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

                    (
                        Some(this_target(-offset * this_scale)),
                        Some(other_target(offset * other_scale))
                    )
                },
                (Some(this_physical), None) =>
                {
                    let this = this_target(-offset);
                    invert_some(this_physical);

                    (Some(this), None)
                },
                (None, Some(other_physical)) =>
                {
                    let other = other_target(offset);
                    invert_some(other_physical);

                    (None, Some(other))
                },
                (None, None) =>
                {
                    let half_offset = offset / 2.0;
                    (Some(this_target(-half_offset)), Some(other_target(half_offset)))
                }
            }
        }
    }

    fn resolve_with_offset<OtherF>(
        &mut self,
        other: &mut CollidingInfo<OtherF>,
        max_distance: Vector3<f32>,
        offset: Vector3<f32>,
        axis: Option<Axis>
    ) -> (Option<Vector3<f32>>, Option<Vector3<f32>>)
    where
        OtherF: FnMut(Vector3<f32>, Option<f32>) -> Vector3<f32>
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
            if axis.is_some() && axis != Some(Axis::Z)
            {
                return (None, None);
            }

            Vector3::new(0.0, 0.0, offset.z)
        } else if (abs_offset.y <= abs_offset.x) && (abs_offset.y <= abs_offset.z)
        {
            if axis.is_some() && axis != Some(Axis::Y)
            {
                return (None, None);
            }

            Vector3::new(0.0, offset.y, 0.0)
        } else
        {
            if axis.is_some() && axis != Some(Axis::X)
            {
                return (None, None);
            }

            Vector3::new(offset.x, 0.0, 0.0)
        };

        self.resolve_with(other, offset)
    }

    pub fn resolve<OtherF>(
        &mut self,
        mut other: CollidingInfo<OtherF>
    ) -> bool
    where
        OtherF: FnMut(Vector3<f32>, Option<f32>) -> Vector3<f32>
    {
        let result = self.basic.collision(&other.basic);
        let collided = result.is_some();

        if let Some(handle) = result
        {
            handle(self, &mut other, None);
        }

        if collided
        {
            if let Some(other) = other.entity
            {
                self.basic.collider.push_collided(other);
            }

            if let Some(entity) = self.entity
            {
                other.basic.collider.push_collided(entity);
            }
        }

        collided
    }

    pub fn resolve_with_world(
        &mut self,
        world: &World
    ) -> bool
    {
        if let Some(old_position) = self.basic.collider.previous_position
        {
            let new_position = self.basic.transform.position;

            self.basic.transform.position = old_position;

            let mut collided = false;

            macro_rules! handle_axis
            {
                ($c:ident, $C:ident) =>
                {
                    self.basic.transform.position.$c = new_position.$c;
                    let (this_collided, resolved) = self.resolve_with_world_inner(
                        world,
                        Some(Axis::$C)
                    );

                    if let Some(resolved) = resolved
                    {
                        self.basic.transform.position = resolved;
                    }

                    collided |= this_collided;
                }
            }

            handle_axis!(x, X);
            handle_axis!(y, Y);
            handle_axis!(z, Z);

            collided
        } else
        {
            self.resolve_with_world_inner(world, None).0
        }
    }

    fn resolve_with_world_inner(
        &mut self,
        world: &World,
        axis: Option<Axis>
    ) -> (bool, Option<Vector3<f32>>)
    {
        let collisions = world.tiles_inside(&self.basic, |tile|
        {
            let colliding_tile = tile.map(|x| world.tile_info(*x).colliding);

            colliding_tile.unwrap_or(false)
        }).map(|pos| pos.entity_position());

        let mut collider = ColliderInfo{
            kind: ColliderType::Aabb,
            layer: ColliderLayer::World,
            ghost: false,
            scale: None,
            move_z: false,
            target_non_lazy: false,
            is_static: true
        }.into();

        let mut planes = Vec::new();

        macro_rules! cmp_axis
        {
            ($a:expr, $b:expr, $c:ident) =>
            {
                $a.$c == $b.$c
            }
        }

        macro_rules! axis_check
        {
            ($a:expr, $b:expr, $axis:expr) =>
            {
                match $axis
                {
                    Axis::X => cmp_axis!($a, $b, x),
                    Axis::Y => cmp_axis!($a, $b, y),
                    Axis::Z => cmp_axis!($a, $b, z)
                }
            }
        }

        if let Some(axis) = axis
        {
            collisions.for_each(|position|
            {
                if !planes.iter_mut().any(|plane: &mut Vec<Vector3<f32>>|
                {
                    let fits = axis_check!(plane[0], position, axis);
                    if fits
                    {
                        plane.push(position);
                    }

                    fits
                })
                {
                    planes.push(vec![position]);
                }
            });

            if planes.is_empty()
            {
                return (false, None);
            }
        } else
        {
            let collisions = collisions.collect::<Vec<_>>();

            if collisions.is_empty()
            {
                return (false, None);
            }

            planes = vec![collisions];
        }

        for plane in planes.into_iter()
        {
            let amount = plane.len();
            let total_position = plane.into_iter().reduce(|acc, x| acc + x).unwrap();

            let collision_point = total_position / amount as f32;

            let mut other = CollidingInfo{
                entity: None,
                physical: None,
                target: |x, _| x,
                basic: BasicCollidingInfo{
                    transform: Transform{
                        position: collision_point,
                        scale: Vector3::repeat(TILE_SIZE),
                        ..Default::default()
                    },
                    collider: &mut collider
                }
            };

            let result = self.basic.collision(&other.basic);

            if let Some(resolve) = result
            {
                return (true, resolve(self, &mut other, axis).0);
            }
        }

        (true, None)
    }
}
