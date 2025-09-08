use std::{
    convert,
    num::FpCategory,
    ops::ControlFlow
};

use serde::{Serialize, Deserialize};

use nalgebra::{Unit, Matrix2, Vector2, Vector3};

use yanyaengine::Transform;

use crate::common::{
    some_or_value,
    define_layers,
    aabb_bounds,
    rectangle_edges,
    is_intersection_lines,
    ENTITY_SCALE,
    Line,
    Entity,
    Physical,
    raycast::{raycast_this, swept_aabb_world_with_before},
    world::{
        TILE_SIZE,
        TilePos,
        DirectionsGroup,
        World
    }
};


fn non_colliding_z(this: &Transform, other: &Transform) -> bool
{
    (this.position.z - other.position.z).abs() > (this.scale.z + other.scale.z) * 0.5
}

pub type WorldTileInfo = DirectionsGroup<bool>;

#[derive(Debug, Clone)]
pub struct ContactGeneral<T>
{
    pub a: T,
    pub b: Option<Entity>,
    pub point: Vector2<f32>,
    pub normal: Unit<Vector2<f32>>,
    pub penetration: f32
}

pub type ContactRaw = ContactGeneral<Option<Entity>>;
pub type Contact = ContactGeneral<Entity>;

impl From<ContactRaw> for Contact
{
    fn from(v: ContactRaw) -> Contact
    {
        if let Some(a) = v.a
        {
            Contact{
                a,
                b: v.b,
                point: v.point,
                normal: v.normal,
                penetration: v.penetration
            }
        } else
        {
            Contact{
                a: v.b.expect("at least 1 object in contact must have an entity"),
                b: v.a,
                point: v.point,
                normal: -v.normal,
                penetration: v.penetration
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColliderType
{
    RayZ,
    Tile(WorldTileInfo),
    Circle,
    Aabb,
    Rectangle
}

impl ColliderType
{
    pub fn half_size(&self, scale: Vector3<f32>) -> Vector3<f32>
    {
        match self
        {
            ColliderType::RayZ =>
            {
                Vector3::new(0.0, 0.0, scale.z)
            },
            ColliderType::Tile(_) =>
            {
                unreachable!()
            },
            ColliderType::Circle => Vector3::repeat(scale.max() / 2.0),
            ColliderType::Aabb
            | ColliderType::Rectangle => scale / 2.0
        }
    }

    pub fn inverse_inertia(
        &self,
        physical: &Physical,
        scale: &Vector3<f32>
    ) -> f32
    {
        // to prevent div by zero cuz floating points suck and i hate them
        if (physical.inverse_mass.classify() == FpCategory::Zero) || physical.fixed.rotation
        {
            return 0.0;
        }

        let m = physical.inverse_mass.recip();

        match self
        {
            Self::RayZ
            | Self::Aabb
            | Self::Tile(_) => 0.0,
            Self::Circle =>
            {
                ((2.0/5.0) * m * scale.max().powi(2)).recip()
            },
            Self::Rectangle =>
            {
                ((1.0/12.0) * m * (scale.x.powi(2) + scale.y.powi(2))).recip()
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ColliderLayer
{
    Normal,
    Damage,
    World,
    Door,
    Mouse,
    Player,
    NormalEnemy,
    LyingEnemy,
    Vision,
    ThrownDecal
}

impl ColliderLayer
{
    pub fn collides(&self, other: &Self) -> bool
    {
        define_layers!{
            self, other,

            (Normal, Normal, true),

            (Damage, Damage, false),
            (Damage, Normal, true),

            (World, World, false),
            (World, Normal, true),
            (World, Damage, true),

            (Mouse, Mouse, false),
            (Mouse, Normal, true),
            (Mouse, Damage, false),
            (Mouse, World, false),

            (Door, Door, false),
            (Door, Normal, true),
            (Door, Damage, true),
            (Door, World, false),
            (Door, Mouse, false),

            (Player, Player, false),
            (Player, Normal, true),
            (Player, Damage, true),
            (Player, World, true),
            (Player, Mouse, true),
            (Player, Door, true),

            (NormalEnemy, NormalEnemy, true),
            (NormalEnemy, Normal, true),
            (NormalEnemy, Damage, true),
            (NormalEnemy, World, true),
            (NormalEnemy, Mouse, true),
            (NormalEnemy, Door, true),
            (NormalEnemy, Player, false),

            (LyingEnemy, LyingEnemy, true),
            (LyingEnemy, Normal, false),
            (LyingEnemy, Damage, true),
            (LyingEnemy, World, true),
            (LyingEnemy, Mouse, true),
            (LyingEnemy, Door, true),
            (LyingEnemy, Player, false),
            (LyingEnemy, NormalEnemy, false),

            (Vision, Vision, false),
            (Vision, Normal, true),
            (Vision, Damage, false),
            (Vision, World, true),
            (Vision, Mouse, false),
            (Vision, Door, true),
            (Vision, Player, true),
            (Vision, NormalEnemy, true),
            (Vision, LyingEnemy, false),

            (ThrownDecal, ThrownDecal, true),
            (ThrownDecal, Normal, false),
            (ThrownDecal, Damage, false),
            (ThrownDecal, World, true),
            (ThrownDecal, Mouse, false),
            (ThrownDecal, Door, false),
            (ThrownDecal, Player, false),
            (ThrownDecal, NormalEnemy, false),
            (ThrownDecal, LyingEnemy, false),
            (ThrownDecal, Vision, false)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverrideTransform
{
    pub transform: Transform,
    pub override_position: bool
}

#[derive(Debug, Clone)]
pub struct ColliderInfo
{
    pub kind: ColliderType,
    pub layer: ColliderLayer,
    pub ghost: bool,
    pub sleeping: bool,
    pub override_transform: Option<OverrideTransform>
}

impl Default for ColliderInfo
{
    fn default() -> Self
    {
        Self{
            kind: ColliderType::Circle,
            layer: ColliderLayer::Normal,
            ghost: false,
            sleeping: false,
            override_transform: None
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collider
{
    pub kind: ColliderType,
    pub layer: ColliderLayer,
    pub ghost: bool,
    pub sleeping: bool,
    pub override_transform: Option<OverrideTransform>,
    collided: Vec<Entity>,
    collided_tiles: Vec<TilePos>
}

impl From<ColliderInfo> for Collider
{
    fn from(info: ColliderInfo) -> Self
    {
        Self{
            kind: info.kind,
            layer: info.layer,
            ghost: info.ghost,
            sleeping: info.sleeping,
            override_transform: info.override_transform,
            collided: Vec::new(),
            collided_tiles: Vec::new()
        }
    }
}

impl Collider
{
    pub fn half_bounds(&self, transform: &Transform) -> Vector3<f32>
    {
        let scale = self.kind.half_size(transform.scale);

        if self.kind == ColliderType::Rectangle
        {
            aabb_bounds(transform) * 0.5
        } else
        {
            scale
        }
    }

    pub fn inverse_inertia(
        &self,
        physical: &Physical,
        scale: &Vector3<f32>
    ) -> f32
    {
        self.kind.inverse_inertia(physical, scale)
    }

    pub fn collided(&self) -> &[Entity]
    {
        &self.collided
    }

    pub fn collided_tiles(&self) -> &[TilePos]
    {
        &self.collided_tiles
    }

    pub fn push_collided(&mut self, entity: Entity)
    {
        if !self.collided.contains(&entity)
        {
            self.collided.push(entity);
        }
    }

    pub fn push_collided_tile(&mut self, tile: TilePos)
    {
        self.collided_tiles.push(tile);
    }

    pub fn reset_frame(&mut self)
    {
        self.collided.clear();
        self.collided_tiles.clear();
    }
}

fn limit_obb_local(scale: Vector2<f32>, local_point: Vector2<f32>) -> Vector2<f32>
{
    local_point.zip_map(&(scale.abs() / 2.0), |x, limit|
    {
        x.clamp(-limit, limit)
    })
}

#[derive(Debug)]
pub struct TransformMatrix<'a>
{
    pub transform: &'a Transform,
    pub entity: Option<Entity>,
    pub rotation_matrix: Matrix2<f32>
}

impl<'b> TransformMatrix<'b>
{
    pub fn from_transform(
        transform: &'b Transform,
        entity: Option<Entity>
    ) -> TransformMatrix<'b>
    {
        let rotation = -transform.rotation;

        Self{
            transform,
            entity,
            rotation_matrix: Matrix2::new(
                rotation.cos(), rotation.sin(),
                -rotation.sin(), rotation.cos()
            )
        }
    }

    pub fn new_identity(
        transform: &'b Transform,
        entity: Option<Entity>
    ) -> TransformMatrix<'b>
    {
        Self{
            transform,
            entity,
            rotation_matrix: Matrix2::identity()
        }
    }

    fn rectangle_on_axis(&self, axis: &Vector2<f32>) -> f32
    {
        self.transform.scale.iter().zip(self.rotation_matrix.column_iter()).map(|(scale, column)|
        {
            (scale / 2.0) * axis.dot(&column).abs()
        }).sum()
    }

    pub fn penetration_axis(&self, other: &Self, axis: &Vector2<f32>) -> f32
    {
        let this_projected = self.rectangle_on_axis(axis);
        let other_projected = other.rectangle_on_axis(axis);

        let diff = other.transform.position.xy() - self.transform.position.xy();

        let axis_distance = diff.dot(axis).abs();

        this_projected + other_projected - axis_distance
    }

    fn rectangle_points(&self) -> impl Iterator<Item=Vector2<f32>> + '_
    {
        const DIMS: usize = 2;

        let half_scale = self.transform.scale.xy() / 2.0;
        (0..2_usize.pow(DIMS as u32)).map(move |i|
        {
            let mut local_point = half_scale;

            // wow its binary!
            (0..DIMS).for_each(|axis_i|
            {
                if ((i >> axis_i) & 1) == 1
                {
                    *local_point.index_mut(axis_i) = -local_point.index(axis_i);
                }
            });

            self.transform.position.xy() + self.rotation_matrix * local_point
        })
    }

    #[allow(clippy::wrong_self_convention)]
    fn into_obb_local(&self, point: Vector2<f32>) -> Vector2<f32>
    {
        self.rotation_matrix.transpose() * (point - self.transform.position.xy())
    }

    #[allow(clippy::wrong_self_convention)]
    fn from_obb_local(&self, point: Vector2<f32>) -> Vector2<f32>
    {
        (self.rotation_matrix * point) + self.transform.position.xy()
    }

    pub fn inside_obb(&self, point: Vector2<f32>) -> bool
    {
        self.into_obb_local(point).zip_map(&(self.transform.scale.xy().abs() / 2.0), |x, limit|
        {
            (-limit..limit).contains(&x)
        }).fold(true, |a, b| a && b)
    }

    pub fn try_project_onto_obb_edge(&self, point: Vector2<f32>) -> Option<(Vector2<f32>, Vector2<f32>)>
    {
        if !self.inside_obb(point) { return None; }

        Some(self.project_onto_obb_edge(point))
    }

    pub fn project_onto_obb_edge(&self, point: Vector2<f32>) -> (Vector2<f32>, Vector2<f32>)
    {
        let local_point = self.into_obb_local(point);

        let projected = limit_obb_local(self.transform.scale.xy(), local_point);

        let mut limited = projected;

        let index = (projected.abs().component_div(&self.transform.scale.xy())).imax();

        let face_sign = local_point.index(index).signum();
        *limited.index_mut(index) = (*self.transform.scale.index(index) / 2.0) * face_sign;

        (self.from_obb_local(limited), self.from_obb_local(projected))
    }

    pub fn project_onto_obb(&self, point: Vector2<f32>) -> Vector2<f32>
    {
        let local_point = self.into_obb_local(point);

        let limited = limit_obb_local(self.transform.scale.xy(), local_point.xy());

        self.from_obb_local(limited)
    }

    fn distance_to_obb(&self) -> impl Fn(Vector2<f32>) -> (f32, Vector2<f32>) + '_
    {
        |point|
        {
            let projected = self.project_onto_obb(point);

            (point.metric_distance(&projected), point)
        }
    }

    fn handle_penetration<'a, F>(
        &self,
        other: &'a Self,
        axis: Unit<Vector2<f32>>,
        penetration: f32
    ) -> impl FnOnce(F) + use<'a, '_, F>
    where
        F: FnMut(Contact)
    {
        move |mut add_contact: F|
        {
            let diff = other.transform.position.xy() - self.transform.position.xy();

            let normal = if axis.dot(&diff) > 0.0
            {
                axis
            } else
            {
                -axis
            };

            let (_distance, point) = other.rectangle_points().map(self.distance_to_obb())
                .chain(self.rectangle_points().map(other.distance_to_obb())).min_by(|a, b|
                {
                    a.0.partial_cmp(&b.0).unwrap()
                }).unwrap();

            add_contact(ContactRaw{
                a: self.entity,
                b: other.entity,
                point: point,
                penetration,
                normal: -normal
            }.into());
        }
    }

    fn rectangle_rectangle_contact_special<'a, F>(
        &'a self,
        other: &'a Self,
        mut add_contact: F,
        this_axis: impl Fn((Unit<Vector2<f32>>, f32, usize)) -> bool,
        other_axis: impl Fn((Unit<Vector2<f32>>, f32, usize)) -> bool
    ) -> bool
    where
        F: FnMut(Contact)
    {
        // funy
        let try_penetrate = |axis: Unit<Vector2<f32>>| -> (f32, _)
        {
            let penetration = self.penetration_axis(other, &axis);

            (penetration, move |this: &'a Self, other| -> (f32, _)
            {
                (penetration, this.handle_penetration(other, axis, penetration))
            })
        };

        enum PenetrationInfo<F>
        {
            ThisAxis((f32, F)),
            OtherAxis((Unit<Vector2<f32>>, f32, usize), F)
        }

        impl<F> PenetrationInfo<F>
        {
            fn penetration(&self) -> f32
            {
                match self
                {
                    Self::ThisAxis((p, _)) => *p,
                    Self::OtherAxis((_, p, _), _) => *p
                }
            }
        }

        let mut penetrations = (0..2).filter_map(|i|
        {
            let axis: Vector2<f32> = self.rotation_matrix.column(i).into();
            let axis = Unit::new_unchecked(axis);

            let (penetration, handler) = try_penetrate(axis);

            this_axis((axis, penetration, i)).then(||
            {
                PenetrationInfo::ThisAxis(handler(self, other))
            })
        }).chain((0..2).map(|i|
        {
            let axis: Vector2<f32> = other.rotation_matrix.column(i).into();
            let axis = Unit::new_unchecked(axis);

            let (_penetration, handler) = try_penetrate(axis);

            let (penetration, handle) = handler(other, self);
            PenetrationInfo::OtherAxis((axis, penetration, i), handle)
        }));

        let first = some_or_value!(penetrations.next(), false);
        let least_penetrating = penetrations.try_fold(first, |b, a|
        {
            let next = if a.penetration() < b.penetration()
            {
                a
            } else
            {
                b
            };

            if next.penetration() <= 0.0
            {
                ControlFlow::Break(())
            } else
            {
                ControlFlow::Continue(next)
            }
        });

        let info = if let ControlFlow::Continue(x) = least_penetrating
        {
            x
        } else
        {
            return false;
        };

        if info.penetration() <= 0.0
        {
            return false;
        }

        let handler = match info
        {
            PenetrationInfo::ThisAxis((_, handler)) => handler,
            PenetrationInfo::OtherAxis(info, handler) =>
            {
                if !other_axis(info)
                {
                    return false;
                }

                handler
            }
        };

        handler(&mut add_contact);

        true
    }

    fn rectangle_circle_inner(
        &self,
        other: &CollidingInfo,
        mut add_contact: impl FnMut(Vector2<f32>, Vector2<f32>, f32, Option<Unit<Vector2<f32>>>) -> bool
    ) -> bool
    {

        if non_colliding_z(&self.transform, &other.transform)
        {
            return false;
        }

        let circle_pos = other.transform.position.xy();

        let radius = other.transform.scale.xy().max() / 2.0;

        let (projected, projected_inside) = self.project_onto_obb_edge(circle_pos);

        let diff = projected_inside - circle_pos;
        let magnitude = diff.magnitude();

        let penetration = radius - magnitude;

        if penetration <= 0.0
        {
            if !self.inside_obb(circle_pos)
            {
                return false;
            }
        }

        let normal = Unit::try_new(projected_inside - circle_pos, 0.0001);

        add_contact(projected, projected_inside, penetration, normal)
    }
}

#[derive(Debug)]
pub struct CollidingInfo<'a>
{
    pub entity: Option<Entity>,
    pub transform: Transform,
    pub collider: &'a mut Collider
}

impl<'a> CollidingInfo<'a>
{
    pub fn half_bounds(&self) -> Vector3<f32>
    {
        self.collider.half_bounds(&self.transform)
    }

    pub fn half_size(&self) -> Vector3<f32>
    {
        self.collider.kind.half_size(self.transform.scale)
    }

    fn circle_circle(
        &self,
        other: &Self,
        mut add_contact: impl FnMut(Contact)
    ) -> bool
    {
        if non_colliding_z(&self.transform, &other.transform)
        {
            return false;
        }

        let this_radius = self.transform.scale.xy().max() / 2.0;
        let other_radius = other.transform.scale.xy().max() / 2.0;

        let diff = other.transform.position.xy() - self.transform.position.xy();
        let distance = diff.magnitude();

        if (distance - this_radius - other_radius) >= 0.0
        {
            return false;
        }

        let normal = if distance.classify() == FpCategory::Zero
        {
            Vector2::x_axis()
        } else
        {
            Unit::new_unchecked(diff / distance)
        };

        add_contact(Contact{
            a: self.entity.unwrap(),
            b: other.entity,
            point: self.transform.position.xy() + *normal * this_radius,
            penetration: this_radius + other_radius - distance,
            normal: -normal
        });

        true
    }

    pub fn transform_matrix(&self) -> TransformMatrix
    {
        TransformMatrix::from_transform(&self.transform, self.entity)
    }

    fn is_rectangle_rectangle_colliding(
        &self,
        other: &Transform,
        mut other_edges: impl Iterator<Item=Line>
    ) -> bool
    {
        if non_colliding_z(&self.transform, other)
        {
            return false;
        }

        let this_edges: [Line; 4] = rectangle_edges(&self.transform).collect::<Vec<_>>().try_into().unwrap();

        other_edges.any(|line0|
        {
            this_edges.iter().any(|line1| is_intersection_lines(line0, *line1))
        })
    }

    fn rectangle_rectangle_inner(
        &self,
        other: &Self,
        add_contact: impl FnMut(Contact)
    ) -> bool
    {
        let colliding = self.is_rectangle_rectangle_colliding(&other.transform, rectangle_edges(&other.transform));

        if !colliding
        {
            return false;
        }

        self.transform_matrix().rectangle_rectangle_contact_special(
            &other.transform_matrix(),
            add_contact,
            |_| {true},
            |_| {true}
        );

        true
    }

    fn rectangle_rectangle(
        &self,
        other: &Self,
        add_contact: impl FnMut(Contact)
    ) -> bool
    {
        self.rectangle_rectangle_inner(other, add_contact)
    }

    fn tile_rayz(
        &self,
        other: &Self,
        _world: &WorldTileInfo,
        add_contact: impl FnMut(Contact)
    ) -> bool
    {
        // if i want proper contacts with ray vs world then i have to rewrite this
        other.rayz_rectangle(self, add_contact)
    }

    fn tile_circle(
        &self,
        other: &Self,
        world: &WorldTileInfo,
        mut add_contact: impl FnMut(Contact)
    ) -> bool
    {
        let this = TransformMatrix::new_identity(&self.transform, self.entity);

        this.rectangle_circle_inner(other, |projected, projected_inside, penetration, normal|
        {
            let (point, normal) = if let Some(normal) = normal
            {
                (projected, normal)
            } else
            {
                let limited = this.into_obb_local(projected_inside);

                let d = limited.abs();

                let sorted = if d.x < d.y
                {
                    [0, 1]
                } else
                {
                    [1, 0]
                };

                fn with_limited(limited: Vector2<f32>, f: impl Fn(f32) -> f32) -> impl Fn(usize) -> (usize, f32)
                {
                    move |axis_i|
                    {
                        (axis_i, f(limited.index(axis_i).signum()))
                    }
                }

                let mut point = projected;
                let data = sorted.into_iter().rev().map(with_limited(limited, |x| -x))
                    .chain(sorted.into_iter().map(with_limited(limited, convert::identity)))
                    .find_map(|(axis_i, amount)|
                    {
                        let (low, high) = world.get_axis_index(axis_i);

                        let available = if amount < 0.0
                        {
                            !high
                        } else
                        {
                            !low
                        };

                        available.then(||
                        {
                            let mut normal = Vector2::zeros();
                            *normal.index_mut(axis_i) = amount;

                            (axis_i, (TILE_SIZE / 2.0) * amount, Unit::new_unchecked(normal))
                        })
                    });

                let (axis_i, amount, normal) = some_or_value!(data, false);

                *point.index_mut(axis_i) = this.transform.position.index(axis_i) - amount;

                (point, normal)
            };

            let mut axis: Vector2<f32> = *normal;

            (0..2).for_each(|axis_i|
            {
                let (low, high) = world.get_axis_index(axis_i);

                let amount = -axis.index(axis_i);
                let epsilon = 0.00001;

                if amount < -epsilon
                {
                    if *low
                    {
                        *axis.index_mut(axis_i) = 0.0;
                    }
                } else if amount > epsilon
                {
                    if *high
                    {
                        *axis.index_mut(axis_i) = 0.0;
                    }
                };
            });

            let magnitude = axis.magnitude();

            if magnitude > 0.0001
            {
                add_contact(ContactRaw{
                    a: self.entity,
                    b: other.entity,
                    point: point,
                    penetration: magnitude * penetration,
                    normal: Unit::new_unchecked(axis / magnitude)
                }.into());

                true
            } else
            {
                false
            }
        })
    }

    fn allowed_axis(world: &WorldTileInfo, axis: Vector2<f32>) -> bool
    {
        (0..2).all(|axis_i|
        {
            let (low, high) = world.get_axis_index(axis_i);

            let amount = *axis.index(axis_i);

            let is_blocked = if amount.classify() == FpCategory::Zero
            {
                false
            } else if amount < 0.0
            {
                *low
            } else
            {
                *high
            };

            !is_blocked
        })
    }

    fn world_handler(
        check: impl Fn(Unit<Vector2<f32>>, usize) -> bool
    ) -> impl Fn((Unit<Vector2<f32>>, f32, usize)) -> bool
    {
        move |(axis, penetration, i)|
        {
            let ignore = check(axis, i);

            let ignored_axis = ignore && penetration > 0.0;

            !ignored_axis
        }
    }

    fn tile_rectangle(
        &self,
        other: &Self,
        world: &WorldTileInfo,
        add_contact: impl FnMut(Contact)
    ) -> bool
    {
        let colliding = other.is_rectangle_rectangle_colliding(&self.transform, world.map(|dir, x| (dir, x)).filter_map(|(dir, x)|
        {
            (!x).then(|| dir.edge_line_2d(Vector2::repeat(TILE_SIZE)).map(|x| x + self.transform.position.xy()))
        }).into_iter());

        if !colliding
        {
            return false;
        }

        let diff = other.transform.position.xy() - self.transform.position.xy();

        let this = self.transform_matrix();
        let other = other.transform_matrix();

        other.rectangle_rectangle_contact_special(
            &this,
            add_contact,
            Self::world_handler(|_axis, _i|
            {
                !Self::allowed_axis(world, diff.xy())
            }),
            Self::world_handler(|axis, i|
            {
                let (low, high) = world.get_axis_index(i);

                let has_tile = if axis.dot(&diff) > 0.0
                {
                    high
                } else
                {
                    low
                };

                *has_tile
            })
        );

        true
    }

    fn rayz_this(&self, other: &Self) -> bool
    {
        let half_z = self.transform.scale.z / 2.0;
        let mut start = self.transform.position;
        start.z -= half_z;

        let direction = Unit::new_unchecked(Vector3::z());

        if let Some(result) = raycast_this(
            start,
            direction,
            other.collider.kind,
            &other.transform
        )
        {
            result.distance <= half_z
        } else
        {
            false
        }
    }

    fn rayz_circle(
        &self,
        other: &Self,
        _add_contact: impl FnMut(Contact)
    ) -> bool
    {
        self.rayz_this(other)
    }

    fn rayz_rectangle(
        &self,
        other: &Self,
        _add_contact: impl FnMut(Contact)
    ) -> bool
    {
        self.rayz_this(other)
    }

    fn rectangle_circle(
        &self,
        other: &Self,
        mut add_contact: impl FnMut(Contact)
    ) -> bool
    {
        let this = self.transform_matrix();

        this.rectangle_circle_inner(other, |projected, projected_inside, penetration, normal|
        {
            let normal = normal.unwrap_or_else(||
            {
                let limited = this.into_obb_local(projected_inside);

                let axis_i = limited.abs().imax();
                let mut normal = Vector2::zeros();
                *normal.index_mut(axis_i) = -limited.index(axis_i).signum();

                Unit::new_unchecked(this.rotation_matrix * normal)
            });

            add_contact(ContactRaw{
                a: self.entity,
                b: other.entity,
                point: projected,
                penetration,
                normal
            }.into());

            true
        })
    }

    pub fn collide_immutable(
        &self,
        other: &Self,
        mut add_contact: impl FnMut(Contact)
    ) -> bool
    {
        if !self.collider.layer.collides(&other.collider.layer)
        {
            return false;
        }

        let ignore_contacts = self.collider.ghost || other.collider.ghost;

        let add_contact = |contact: Contact|
        {
            if !ignore_contacts
            {
                debug_assert!(!contact.penetration.is_nan());
                debug_assert!(!contact.normal.x.is_nan());

                add_contact(contact);
            }
        };

        macro_rules! define_collisions
        {
            (
                $(ignored($a_ignored:pat, $b_ignored:pat)),+,
                $(with_world($b_world:ident, $world_name:ident)),+,
                $(($a:ident, $b:ident, $name:ident)),+
            ) =>
            {
                {
                    #[allow(unreachable_patterns)]
                    match (self.collider.kind, other.collider.kind)
                    {
                        $(
                            ($a_ignored, $b_ignored) => false,
                            ($b_ignored, $a_ignored) => false,
                        )+
                        $(
                            (ColliderType::Tile(_), ColliderType::$b_world) => unreachable!(),
                            (ColliderType::$b_world, ColliderType::Tile(ref info)) =>
                            {
                                if info.fold(true, |acc, (_, x)| acc && x)
                                {
                                    // early exit if all directions r blocked
                                    false
                                } else
                                {
                                    other.$world_name(self, info, add_contact)
                                }
                            },
                        )+
                        $(
                            (ColliderType::$a, ColliderType::$b) =>
                            {
                                self.$name(other, add_contact)
                            },
                            (ColliderType::$b, ColliderType::$a) =>
                            {
                                other.$name(self, add_contact)
                            },
                        )+
                    }
                }
            }
        }

        define_collisions!{
            ignored(ColliderType::RayZ, ColliderType::RayZ),
            ignored(ColliderType::Tile(_), ColliderType::Tile(_)),

            with_world(RayZ, tile_rayz),
            with_world(Circle, tile_circle),
            with_world(Aabb, tile_rectangle),
            with_world(Rectangle, tile_rectangle),

            (RayZ, Circle, rayz_circle),
            (RayZ, Aabb, rayz_rectangle),
            (RayZ, Rectangle, rayz_rectangle),

            (Aabb, Aabb, rectangle_rectangle),
            (Aabb, Circle, rectangle_circle),

            (Circle, Circle, circle_circle),

            (Rectangle, Circle, rectangle_circle),
            (Rectangle, Aabb, rectangle_rectangle),
            (Rectangle, Rectangle, rectangle_rectangle)
        }
    }

    pub fn collide(
        &mut self,
        other: CollidingInfo,
        add_contact: impl FnMut(Contact)
    ) -> bool
    {
        let collided = self.collide_immutable(&other, add_contact);

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

    pub fn collide_with_world_z(
        &mut self,
        world: &World,
        next_position: Vector3<f32>
    ) -> bool
    {
        if !self.collider.layer.collides(&ColliderLayer::World)
        {
            return false;
        }

        let direction = next_position - self.transform.position;

        const MARGIN: Vector3<f32> = Vector3::new(ENTITY_SCALE * 0.1, ENTITY_SCALE * 0.1, 0.0);

        if let Some((tile_pos, distance)) = swept_aabb_world_with_before(
            world,
            &Transform{
                scale: self.transform.scale - MARGIN, // make sure that entities cant stick to walls
                ..self.transform
            },
            Vector3::new(0.0, 0.0, direction.z)
        ).filter(|(_, x)| *x > -TILE_SIZE * 0.1).min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        {
            self.transform.position += Vector3::new(0.0, 0.0, distance * direction.z.signum());
            self.collider.push_collided_tile(tile_pos);

            return true;
        }

        false
    }

    pub fn collide_with_world(
        &mut self,
        world: &World,
        contacts: &mut Vec<Contact>
    ) -> bool
    {
        if !self.collider.layer.collides(&ColliderLayer::World)
        {
            return false;
        }

        let collided: Vec<_> = world.tiles_contacts(self, |contact| contacts.push(contact), |tile|
        {
            let colliding_tile = tile.map(|x| world.tile_info(*x).colliding);

            colliding_tile.unwrap_or(true)
        }).collect();

        let is_collided = !collided.is_empty();

        collided.into_iter().for_each(|pos| self.collider.push_collided_tile(pos));

        is_collided
    }
}

#[cfg(test)]
mod tests
{
    use super::*;

    use crate::common::{with_z, rotate_point};


    #[test]
    fn rotation_matrix()
    {
        for _ in 0..50
        {
            let p = Vector2::new(fastrand::f32(), fastrand::f32());
            let r = fastrand::f32() * 6.3;

            let transform = Transform{
                position: with_z(p, 0.0),
                rotation: r,
                ..Default::default()
            };

            let m = TransformMatrix::from_transform(&transform, None);

            println!("rotating {p:?} by {r}");
            assert_eq!(m.rotation_matrix * p, rotate_point(p, r));
        }
    }
}
