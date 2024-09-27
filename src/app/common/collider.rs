use std::{
    num::FpCategory,
    ops::ControlFlow
};

use serde::{Serialize, Deserialize};

use nalgebra::{Unit, Matrix3, Vector2, Vector3};

use yanyaengine::Transform;

use crate::common::{
    some_or_value,
    define_layers,
    rectangle_points,
    Entity,
    Physical,
    raycast::raycast_this,
    world::{
        Directions3dGroup,
        World
    }
};


const DIMS: usize = 3;

pub type WorldTileInfo = Directions3dGroup<bool>;

#[derive(Debug, Clone)]
pub struct ContactGeneral<T>
{
    pub a: T,
    pub b: Option<Entity>,
    pub point: Vector3<f32>,
    pub normal: Vector3<f32>,
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
    pub fn inverse_inertia_tensor(
        &self,
        physical: &Physical,
        transform: &Transform
    ) -> Matrix3<f32>
    {
        // to prevent div by zero cuz floating points suck and i hate them
        if (physical.inverse_mass.classify() == FpCategory::Zero) || physical.fixed.rotation
        {
            return Matrix3::zeros();
        }

        let m = physical.inverse_mass.recip();

        let inertia = match self
        {
            Self::RayZ
            | Self::Aabb
            | Self::Tile(_) => return Matrix3::zeros(),
            Self::Circle =>
            {
                Matrix3::from_diagonal_element((2.0/5.0) * m * transform.scale.max().powi(2))
            },
            Self::Rectangle =>
            {
                let w = transform.scale.x;
                let h = transform.scale.y;
                let d = transform.scale.z;

                let at_axis = |a: f32, b: f32|
                {
                    (1.0/12.0) * m * (a.powi(2) + b.powi(2))
                };

                Matrix3::from_partial_diagonal(&[at_axis(h, d), at_axis(w, d), at_axis(w, h)])
            }
        };

        inertia.try_inverse().expect("must have inverse")
    }

    pub fn inverse_inertia(
        &self,
        physical: &Physical,
        transform: &Transform
    ) -> f32
    {
        self.inverse_inertia_tensor(physical, transform).m33
    }
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
    pub scale: Option<Vector3<f32>>
}

impl Default for ColliderInfo
{
    fn default() -> Self
    {
        Self{
            kind: ColliderType::Circle,
            layer: ColliderLayer::Normal,
            ghost: false,
            scale: None
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
            scale: info.scale,
            collided: Vec::new()
        }
    }
}

impl Collider
{
    pub fn inverse_inertia_tensor(
        &self,
        physical: &Physical,
        mut transform: Transform
    ) -> Matrix3<f32>
    {
        if let Some(scale) = self.scale
        {
            transform.scale = scale;
        }

        self.kind.inverse_inertia_tensor(physical, &transform)
    }

    pub fn inverse_inertia(
        &self,
        physical: &Physical,
        transform: Transform
    ) -> f32
    {
        self.inverse_inertia_tensor(physical, transform).m33
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

fn limit_obb_local(scale: Vector3<f32>, local_point: Vector3<f32>) -> Vector3<f32>
{
    local_point.zip_map(&(scale / 2.0), |x, limit|
    {
        x.clamp(-limit, limit)
    })
}

#[derive(Debug)]
struct TransformMatrix<'a>
{
    pub transform: &'a Transform,
    pub entity: Option<Entity>,
    pub rotation_matrix: Matrix3<f32>
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
            rotation_matrix: Matrix3::new(
                rotation.cos(), rotation.sin(), 0.0,
                -rotation.sin(), rotation.cos(), 0.0,
                0.0, 0.0, 1.0
            )
        }
    }

    fn rectangle_on_axis(&self, axis: &Vector3<f32>) -> f32
    {
        self.transform.scale.iter().zip(self.rotation_matrix.column_iter()).map(|(scale, column)|
        {
            (scale / 2.0) * axis.dot(&column).abs()
        }).sum()
    }

    pub fn penetration_axis(&self, other: &Self, axis: &Vector3<f32>) -> f32
    {
        let this_projected = self.rectangle_on_axis(axis);
        let other_projected = other.rectangle_on_axis(axis);

        let diff = other.transform.position - self.transform.position;

        let axis_distance = diff.dot(axis).abs();

        this_projected + other_projected - axis_distance
    }

    fn cuboid_points(&self) -> impl Iterator<Item=Vector3<f32>> + '_
    {
        let half_scale = self.transform.scale / 2.0;
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

            self.transform.position + self.rotation_matrix * local_point
        })
    }

    fn into_obb_local(&self, point: Vector3<f32>) -> Vector3<f32>
    {
        self.rotation_matrix.transpose() * (point - self.transform.position)
    }

    fn from_obb_local(&self, point: Vector3<f32>) -> Vector3<f32>
    {
        (self.rotation_matrix * point) + self.transform.position
    }

    fn project_onto_obb(&self, point: Vector3<f32>) -> Vector3<f32>
    {
        let local_point = self.into_obb_local(point);

        let limited = limit_obb_local(self.transform.scale, local_point);

        self.from_obb_local(limited)
    }

    fn distance_to_obb(&self) -> impl Fn(Vector3<f32>) -> (f32, Vector3<f32>) + '_
    {
        |point|
        {
            let projected = self.project_onto_obb(point);

            (point.metric_distance(&projected), point)
        }
    }

    fn handle_penetration<'a, F>(
        &'a self,
        other: &'a Self,
        axis: Vector3<f32>,
        penetration: f32
    ) -> impl FnOnce(F) + 'a
    where
        F: FnMut(Contact)
    {
        move |mut add_contact: F|
        {
            let diff = other.transform.position - self.transform.position;

            let normal = if axis.dot(&diff) > 0.0
            {
                axis
            } else
            {
                -axis
            };

            let (_distance, point) = other.cuboid_points().map(self.distance_to_obb())
                .chain(self.cuboid_points().map(other.distance_to_obb())).min_by(|a, b|
                {
                    a.0.partial_cmp(&b.0).unwrap()
                }).unwrap();

            add_contact(ContactRaw{
                a: self.entity,
                b: other.entity,
                point,
                penetration,
                normal: -normal
            }.into());
        }
    }

    // this will generate wrong contacts if its an edge/edge collision
    // im not handling edge/edge collisions cuz i dont wanna (the contact will be close enough)
    fn rectangle_rectangle_contact_special<'a, F>(
        &'a self,
        other: &'a Self,
        mut add_contact: F,
        this_axis: impl Fn((Vector3<f32>, f32, usize)) -> bool,
        other_axis: impl Fn((Vector3<f32>, f32, usize)) -> bool
    ) -> bool
    where
        F: FnMut(Contact)
    {
        // funy
        let try_penetrate = |axis: Vector3<f32>| -> (f32, _)
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
            OtherAxis((Vector3<f32>, f32, usize), F)
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

        let mut penetrations = (0..DIMS).filter_map(|i|
        {
            let axis: Vector3<f32> = self.rotation_matrix.column(i).into();
            let (penetration, handler) = try_penetrate(axis);

            this_axis((axis, penetration, i)).then(||
            {
                PenetrationInfo::ThisAxis(handler(self, other))
            })
        }).chain((0..DIMS).map(|i|
        {
            let axis: Vector3<f32> = other.rotation_matrix.column(i).into();
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
    pub fn bounds(&self) -> Vector3<f32>
    {
        let scale = self.half_size();

        if self.collider.kind == ColliderType::Rectangle
        {
            let points = rectangle_points(&self.transform);

            let size_axis = |i|
            {
                let points = points.iter().map(|x: &Vector2<f32>| -> f32
                {
                    let v: &f32 = x.index(i);
                    *v
                });

                let size = points.clone().max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap()
                    - points.min_by(|a, b| a.partial_cmp(b).unwrap()).unwrap();

                size / 2.0
            };

            Vector3::new(
                size_axis(0),
                size_axis(1),
                scale.z
            )
        } else
        {
            scale
        }
    }

    pub fn half_size(&self) -> Vector3<f32>
    {
        match self.collider.kind
        {
            ColliderType::RayZ =>
            {
                Vector3::new(0.0, 0.0, self.transform.scale.z)
            },
            ColliderType::Tile(_) =>
            {
                unreachable!()
            },
            ColliderType::Circle => Vector3::repeat(self.transform.scale.max() / 2.0),
            ColliderType::Aabb
            | ColliderType::Rectangle => self.transform.scale / 2.0
        }
    }

    fn circle_circle(
        &self,
        other: &Self,
        mut add_contact: impl FnMut(Contact)
    ) -> bool
    {
        let this_radius = self.transform.scale.max() / 2.0;
        let other_radius = other.transform.scale.max() / 2.0;

        let diff = other.transform.position - self.transform.position;
        let distance = diff.magnitude();

        if (distance - this_radius - other_radius) >= 0.0
        {
            return false;
        }

        let normal = if distance.classify() == FpCategory::Zero
        {
            Vector3::x()
        } else
        {
            diff / distance
        };

        add_contact(Contact{
            a: self.entity.unwrap(),
            b: other.entity,
            point: self.transform.position + normal * this_radius,
            penetration: this_radius + other_radius - distance,
            normal: -normal
        });

        true
    }

    fn transform_matrix(&self) -> TransformMatrix
    {
        TransformMatrix::from_transform(&self.transform, self.entity)
    }

    fn rectangle_rectangle_inner(
        &self,
        other: &Self,
        add_contact: impl FnMut(Contact)
    ) -> bool
    {
        self.transform_matrix().rectangle_rectangle_contact_special(
            &other.transform_matrix(),
            add_contact,
            |_| {true},
            |_| {true}
        )
    }

    fn rectangle_circle_inner(
        &self,
        other: &Self,
        mut add_contact: impl FnMut(Contact),
        axis_check: impl FnOnce(Vector3<f32>) -> bool
    ) -> bool
    {
        let circle_pos = other.transform.position;
        let radius = other.transform.scale.max() / 2.0;

        let this = self.transform_matrix();
        let local_point = this.into_obb_local(circle_pos);

        let limited = limit_obb_local(self.transform.scale, local_point);

        let diff = local_point - limited;
        let distance = diff.magnitude();

        if distance >= radius
        {
            return false;
        }

        let projected = this.from_obb_local(limited);

        let normal = (projected - circle_pos).try_normalize(0.0001)
            .unwrap_or_else(||
            {
                let axis_i = limited.abs().imax();
                let mut normal = Vector3::zeros();
                *normal.index_mut(axis_i) = limited.index(axis_i).signum();

                this.rotation_matrix * normal
            });

        if !axis_check(normal)
        {
            return false;
        }

        add_contact(ContactRaw{
            a: self.entity,
            b: other.entity,
            point: projected,
            penetration: radius - distance,
            normal
        }.into());

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

    fn allowed_axis(world: &WorldTileInfo, axis: Vector3<f32>) -> bool
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

    fn ground_case(
        &self,
        tile: &Self,
        world: &WorldTileInfo,
        mut add_contact: impl FnMut(Contact)
    ) -> Option<bool>
    {
        // if all but (or including) forward plane is blocked
        if world.left && world.right
            && world.down && world.up
            && world.back
        {
            let half_height = self.transform.scale.z / 2.0;

            let diff = self.transform.position.z - tile.transform.position.z;
            let distance = diff - (tile.transform.scale.z / 2.0) - half_height;

            if distance >= 0.0
            {
                return Some(false);
            }

            let penetration = -distance;

            let mut point = self.transform.position;
            point.z -= half_height;

            add_contact(Contact{
                a: self.entity.unwrap(),
                b: None,
                point,
                penetration,
                normal: Vector3::z()
            });

            return Some(true);
        }

        None
    }

    fn tile_circle(
        &self,
        other: &Self,
        world: &WorldTileInfo,
        mut add_contact: impl FnMut(Contact)
    ) -> bool
    {
        if let Some(collided) = other.ground_case(self, world, &mut add_contact)
        {
            return collided;
        }

        self.rectangle_circle_inner(other, add_contact, |axis|
        {
            Self::allowed_axis(world, -axis)
        })
    }

    fn world_handler(
        check: impl Fn(Vector3<f32>, usize) -> bool
    ) -> impl Fn((Vector3<f32>, f32, usize)) -> bool
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
        mut add_contact: impl FnMut(Contact)
    ) -> bool
    {
        if let Some(collided) = other.ground_case(self, world, &mut add_contact)
        {
            return collided;
        }

        let diff = other.transform.position - self.transform.position;

        let this = self.transform_matrix();
        let other = other.transform_matrix();

        other.rectangle_rectangle_contact_special(
            &this,
            add_contact,
            Self::world_handler(|_axis, _i|
            {
                !Self::allowed_axis(world, diff)
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
        )
    }

    fn rayz_this(&self, other: &Self) -> bool
    {
        let half_z = self.transform.scale.z / 2.0;
        let mut start = self.transform.position;
        start.z -= half_z;

        let direction = Unit::new_unchecked(Vector3::z());

        if let Some(result) = raycast_this(
            &start,
            &direction,
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
        add_contact: impl FnMut(Contact)
    ) -> bool
    {
        self.rectangle_circle_inner(other, add_contact, |_| true)
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
                #[allow(unreachable_patterns)]
                match (self.collider.kind, other.collider.kind)
                {
                    $(
                        ($a_ignored, $b_ignored) => false,
                        ($b_ignored, $a_ignored) => false,
                    )+
                    $(
                        (ColliderType::Tile(_), ColliderType::$b_world) => unreachable!(),
                        (ColliderType::$b_world, ColliderType::Tile(ref info)) => other.$world_name(self, info, add_contact),
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

        // !!!!!!!!!!!DONT REMOVE COUNT, IT NEEDS TO CONSUME THE WHOLE THING!!!!!!!!!!!
        let collided = world.tiles_inside(self, |contact| contacts.push(contact), |tile|
        {
            let colliding_tile = tile.map(|x| world.tile_info(*x).colliding);

            colliding_tile.unwrap_or(true)
        }).count() > 0;

        collided
    }
}

#[cfg(test)]
mod tests
{
    use super::*;

    use crate::common::rotate_point_z_3d;


    #[test]
    fn rotation_matrix()
    {
        for _ in 0..50
        {
            let p = Vector3::new(fastrand::f32(), fastrand::f32(), fastrand::f32());
            let r = fastrand::f32() * 6.3;

            let transform = Transform{
                position: p,
                rotation: r,
                ..Default::default()
            };

            let m = TransformMatrix::from_transform(&transform, None);

            println!("rotating {p:?} by {r}");
            assert_eq!(m.rotation_matrix * p, rotate_point_z_3d(p, r));
        }
    }
}
