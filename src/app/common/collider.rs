use std::ops::ControlFlow;

use serde::{Serialize, Deserialize};

use nalgebra::{Matrix3, MatrixView3x1, Vector2, Vector3};

use yanyaengine::Transform;

use crate::common::{
    some_or_return,
    define_layers,
    rotate_point,
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


pub fn rotate_point_z_3d(p: Vector3<f32>, angle: f32) -> Vector3<f32>
{
    let (asin, acos) = angle.sin_cos();

    Vector3::new(acos * p.x + asin * p.y, -asin * p.x + acos * p.y, p.z)
}

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
    Point,
    Circle,
    Aabb,
    Rectangle
}

impl ColliderType
{
    pub fn inertia(
        &self,
        physical: &Physical,
        transform: &Transform
    ) -> f32
    {
        match self
        {
            Self::Point => 0.0,
            Self::Circle =>
            {
                (2.0/5.0) * physical.inverse_mass.recip() * transform.scale.max().powi(2)
            },
            Self::Aabb => Self::Rectangle.inertia(physical, transform),
            Self::Rectangle =>
            {
                let w = transform.scale.x;
                let h = transform.scale.y;

                (1.0/12.0) * physical.inverse_mass.recip() * (w.powi(2) + h.powi(2))
            }
        }
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
    pub scale: Option<Vector3<f32>>,
    pub move_z: bool,
    pub target_non_lazy: bool
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
            target_non_lazy: false
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
            move_z: info.move_z,
            target_non_lazy: info.target_non_lazy,
            collided: Vec::new()
        }
    }
}

impl Collider
{
    pub fn inertia(
        &self,
        physical: &Physical,
        transform: &Transform
    ) -> f32
    {
        self.kind.inertia(physical, transform)
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

    pub fn rectangle_rectangle_contact<'a>(
        &'a self,
        other: &'a Self
    ) -> Option<ContactRaw>
    {
        let dims = 3;

        let handle_penetration = move |
            this: &'a Self,
            other: &'a Self,
            axis: Vector3<f32>,
            penetration: f32
        |
        {
            move ||
            {
                let diff = other.transform.position - this.transform.position;

                let normal = if axis.dot(&diff) > 0.0
                {
                    -axis
                } else
                {
                    axis
                };

                let mut local_point = other.transform.scale / 2.0;

                (0..dims).for_each(|i|
                {
                    if other.rotation_matrix.column(i).dot(&normal) < 0.0
                    {
                        let value = -local_point.index(i);
                        *local_point.index_mut(i) = value;
                    }
                });

                let point = other.rotation_matrix * local_point + other.transform.position;

                ContactRaw{
                    a: this.entity,
                    b: other.entity,
                    point,
                    penetration,
                    normal
                }
            }
        };

        // good NAME
        let try_penetrate = |axis: MatrixView3x1<f32>| -> _
        {
            let axis: Vector3<f32> = axis.into();
            let penetration = self.penetration_axis(other, &axis);

            move |this: &'a Self, other: &'a Self| -> (f32, _)
            {
                (penetration, handle_penetration(this, other, axis, penetration))
            }
        };

        let mut penetrations = (0..dims).map(|i|
        {
            try_penetrate(self.rotation_matrix.column(i))(self, other)
        }).chain((0..dims).map(|i|
        {
            try_penetrate(other.rotation_matrix.column(i))(other, self)
        }));

        let first = penetrations.next()?;
        let least_penetrating = penetrations.try_fold(first, |b, a|
        {
            let next = if a.0 < b.0
            {
                a
            } else
            {
                b
            };

            if next.0 <= 0.0
            {
                ControlFlow::Break(())
            } else
            {
                ControlFlow::Continue(next)
            }
        });

        let (_penetration, handler) = if let ControlFlow::Continue(x) = least_penetrating
        {
            x
        } else
        {
            return None;
        };

        Some(handler())
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
            let bl = rotate_point(-scale.xy(), self.transform.rotation);
            let tr = rotate_point(scale.xy(), self.transform.rotation);

            let size = tr - bl;

            Vector3::new(
                size.x,
                size.y,
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
            ColliderType::Point =>
            {
                let mut scale = Vector3::zeros();
                scale.z = self.transform.scale.z / 2.0;

                scale
            },
            ColliderType::Circle
            | ColliderType::Aabb
            | ColliderType::Rectangle => self.transform.scale / 2.0
        }
    }

    fn circle_circle(
        &self,
        other: &Self,
        mut add_contact: impl FnMut(ContactRaw)
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

        let normal = diff / distance;

        add_contact(ContactRaw{
            a: self.entity,
            b: other.entity,
            point: self.transform.position + normal * this_radius,
            penetration: this_radius + other_radius - distance,
            normal: -normal
        });

        true
    }

    fn rectangle_rectangle(
        &self,
        other: &Self,
        mut add_contact: impl FnMut(ContactRaw)
    ) -> bool
    {
        let this = TransformMatrix::from_transform(&self.transform, self.entity);
        let other = TransformMatrix::from_transform(&other.transform, other.entity);

        let contact = this.rectangle_rectangle_contact(&other);
        let collided = contact.is_some();

        if let Some(contact) = contact
        {
            add_contact(contact);
        }

        collided
    }

    fn point_circle(
        &self,
        other: &Self,
        mut add_contact: impl FnMut(ContactRaw)
    ) -> bool
    {
        false
    }

    fn point_rectangle(
        &self,
        other: &Self,
        mut add_contact: impl FnMut(ContactRaw)
    ) -> bool
    {
        false
    }

    fn rectangle_circle(
        &self,
        other: &Self,
        mut add_contact: impl FnMut(ContactRaw)
    ) -> bool
    {
        let circle_projected = rotate_point_z_3d(
            other.transform.position - self.transform.position,
            self.transform.rotation
        );

        let closest_point_local = (self.transform.scale / 2.0).zip_map(&circle_projected, |a, b|
        {
            b.clamp(-a, a)
        });

        let diff = circle_projected - closest_point_local;
        let squared_distance = diff.x.powi(2) + diff.y.powi(2);

        let radius = other.transform.scale.max() / 2.0;
        if squared_distance > radius.powi(2)
        {
            return false;
        }

        let closest_point =  rotate_point_z_3d(
            closest_point_local,
            -self.transform.rotation
        ) + self.transform.position;

        let normal = -(other.transform.position - closest_point).try_normalize(0.0001)
            .unwrap_or_else(||
            {
                -(self.transform.position - closest_point).try_normalize(0.0001).unwrap_or_else(||
                {
                    Vector3::new(1.0, 0.0, 0.0)
                })
            });

        add_contact(ContactRaw{
            a: self.entity,
            b: other.entity,
            point: closest_point,
            penetration: radius - squared_distance.sqrt(),
            normal
        });

        true
    }

    pub fn collide_immutable(
        &self,
        other: &Self,
        mut contacts: Option<&mut Vec<Contact>>
    ) -> bool
    {
        if !self.collider.layer.collides(&other.collider.layer)
        {
            return false;
        }

        let ignore_contacts = contacts.is_none() || self.collider.ghost || other.collider.ghost;

        let add_contact = |contact: ContactRaw|
        {
            if !ignore_contacts
            {
                if let Some(ref mut contacts) = contacts
                {
                    contacts.push(contact.into());
                }
            }
        };

        macro_rules! define_collisions
        {
            (
                $((special, $a_special:ident, $b_special:ident, $special:expr)),+,
                $(($a:ident, $b:ident, $name:ident)),+
            ) =>
            {
                #[allow(unreachable_patterns)]
                match (self.collider.kind, other.collider.kind)
                {
                    $((ColliderType::$a_special, ColliderType::$b_special) => $special,)+
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
            (special, Point, Point, false),

            (Point, Circle, point_circle),
            (Point, Aabb, point_rectangle),
            (Point, Rectangle, point_rectangle),

            (Circle, Circle, circle_circle),

            (Aabb, Aabb, rectangle_rectangle),
            (Aabb, Circle, rectangle_circle),

            (Rectangle, Circle, rectangle_circle),
            (Rectangle, Aabb, rectangle_rectangle),
            (Rectangle, Rectangle, rectangle_rectangle)
        }
    }

    pub fn collide(
        &mut self,
        other: CollidingInfo,
        contacts: Option<&mut Vec<Contact>>
    ) -> bool
    {
        let collided = self.collide_immutable(&other, contacts);

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
        let collided = world.tiles_inside(self, Some(contacts), |tile|
        {
            let colliding_tile = tile.map(|x| world.tile_info(*x).colliding);

            colliding_tile.unwrap_or(false)
        }).count();

        collided > 0
    }
}
