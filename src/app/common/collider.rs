use std::{
    convert,
    num::FpCategory,
    ops::ControlFlow
};

use serde::{Serialize, Deserialize};

use nalgebra::{Matrix3, MatrixView3x1, Vector2, Vector3};

use yanyaengine::Transform;

use crate::common::{
    some_or_return,
    some_or_value,
    define_layers,
    rotate_point,
    point_line_side,
    point_line_distance,
    rectangle_points,
    Axis,
    Entity,
    Physical,
    world::{
        TILE_SIZE,
        Directions3dGroup,
        World
    }
};


pub fn rotate_point_z_3d(p: Vector3<f32>, angle: f32) -> Vector3<f32>
{
    let (asin, acos) = angle.sin_cos();

    Vector3::new(acos * p.x + asin * p.y, -asin * p.x + acos * p.y, p.z)
}

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
    Point,
    Tile(WorldTileInfo),
    Circle,
    Aabb,
    Rectangle
}

impl ColliderType
{
    pub fn inverse_inertia(
        &self,
        physical: &Physical,
        transform: &Transform
    ) -> f32
    {
        // to prevent div by zero cuz floating points suck and i hate them
        if (physical.inverse_mass.classify() == FpCategory::Zero) || physical.fixed.rotation
        {
            return 0.0;
        }

        match self
        {
            Self::Point => 0.0,
            Self::Tile(_) => 0.0,
            Self::Circle =>
            {
                (2.0/5.0) * physical.inverse_mass.recip() * transform.scale.max().powi(2)
            },
            Self::Aabb => 0.0,
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
    pub fn inverse_inertia(
        &self,
        physical: &Physical,
        transform: &Transform
    ) -> f32
    {
        self.kind.inverse_inertia(physical, transform)
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

    fn rectangle_rectangle_contact<'a>(
        &'a self,
        other: &'a Self,
        world: Option<&WorldTileInfo>
    ) -> Option<ContactRaw>
    {
        let dims = 3;

        let diff = other.transform.position - self.transform.position;

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
                    let dist = other.rotation_matrix.column(i).dot(&normal);

                    // if almost parallel pick vertex closest to this
                    let check = if dist.abs() < 0.001
                    {
                        -diff.index(i)
                    } else
                    {
                        dist
                    };

                    if check < 0.0
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

        // funy
        let try_penetrate = |axis: Vector3<f32>| -> _
        {
            let penetration = self.penetration_axis(other, &axis);

            move |this: &'a Self, other: &'a Self, ignore: bool| -> Option<(f32, _)>
            {
                if ignore && penetration > 0.0
                {
                    return None;
                }

                Some((penetration, handle_penetration(this, other, axis, penetration)))
            }
        };

        let mut penetrations = (0..dims).map(|i|
        {
            let axis: Vector3<f32> = self.rotation_matrix.column(i).into();

            let ignore = if let Some(world) = world
            {
                (0..2).any(|axis_i|
                {
                    let (low, high) = world.get_axis_index(axis_i);

                    let amount = -diff.index(axis_i);

                    let is_blocked = if amount < 0.0
                    {
                        low
                    } else
                    {
                        high
                    };

                    *is_blocked
                })
            } else
            {
                false
            };

            try_penetrate(axis)(self, other, ignore)
        }).chain((0..dims).map(|i|
        {
            let axis: Vector3<f32> = other.rotation_matrix.column(i).into();

            let ignore = if let Some(world) = world
            {
                let (low, high) = world.get_axis_index(i);

                let diff = -diff;

                let has_tile = if axis.dot(&diff) > 0.0
                {
                    high
                } else
                {
                    low
                };

                *has_tile
            } else
            {
                false
            };

            try_penetrate(axis)(other, self, ignore)
        }));

        let first = penetrations.find_map(convert::identity)?;
        let least_penetrating = penetrations.try_fold(first, |b, a|
        {
            let a = some_or_value!(a, ControlFlow::Continue(b));

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

        let (penetration, handler) = if let ControlFlow::Continue(x) = least_penetrating
        {
            x
        } else
        {
            return None;
        };

        if penetration <= 0.0
        {
            return None;
        }

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
            let points = rectangle_points(&self.transform);

            let size_axis = |i|
            {
                let points = points.iter().map(|x: &Vector2<f32>| -> f32
                {
                    let v: &f32 = x.index(i);
                    *v
                });

                points.clone().max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap()
                    - points.min_by(|a, b| a.partial_cmp(b).unwrap()).unwrap()
            };

            Vector3::new(
                size_axis(0),
                size_axis(1),
                self.transform.scale.z
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
            ColliderType::Tile(_) =>
            {
                unreachable!()
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

    fn rectangle_rectangle_inner(
        &self,
        other: &Self,
        world: Option<&WorldTileInfo>,
        mut add_contact: impl FnMut(ContactRaw)
    ) -> bool
    {
        let this = TransformMatrix::from_transform(&self.transform, self.entity);
        let other = TransformMatrix::from_transform(&other.transform, other.entity);

        let contact = this.rectangle_rectangle_contact(&other, world);
        let collided = contact.is_some();

        if let Some(contact) = contact
        {
            add_contact(contact);
        }

        collided
    }

    fn rectangle_rectangle(
        &self,
        other: &Self,
        add_contact: impl FnMut(ContactRaw)
    ) -> bool
    {
        false
        // self.rectangle_rectangle_inner(other, None, add_contact)
    }

    fn tile_point(
        &self,
        other: &Self,
        _world: &WorldTileInfo,
        add_contact: impl FnMut(ContactRaw)
    ) -> bool
    {
        // if i want proper contacts with point vs world then i have to rewrite this
        other.point_rectangle(self, add_contact)
    }

    fn tile_circle(
        &self,
        other: &Self,
        world: &WorldTileInfo,
        mut add_contact: impl FnMut(ContactRaw)
    ) -> bool
    {
        false
    }

    fn tile_rectangle(
        &self,
        other: &Self,
        world: &WorldTileInfo,
        add_contact: impl FnMut(ContactRaw)
    ) -> bool
    {
        let put_some_back = ();
        other.rectangle_rectangle_inner(self, None /*Some(world)*/, add_contact)
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
                    let contact: Contact = contact.into();
                    debug_assert!(!contact.penetration.is_nan());

                    if contact.point.magnitude() > 1000.0
                    {
                        let remove_me = ();
                        panic!("{self:?} {other:?} {contact:?}");
                    }

                    contacts.push(contact);
                }
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
            ignored(ColliderType::Point, ColliderType::Point),
            ignored(ColliderType::Tile(_), ColliderType::Tile(_)),

            with_world(Point, tile_point),
            with_world(Circle, tile_circle),
            with_world(Aabb, tile_rectangle),
            with_world(Rectangle, tile_rectangle),

            (Point, Circle, point_circle),
            (Point, Aabb, point_rectangle),
            (Point, Rectangle, point_rectangle),

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
        contacts: &mut Vec<Contact>,
        entities: &crate::common::entity::ClientEntities
    ) -> bool
    {
        if !self.collider.layer.collides(&ColliderLayer::World)
        {
            return false;
        }

        let collided = world.tiles_inside(self, false, Some(contacts), |tile|
        {
            let colliding_tile = tile.map(|x| world.tile_info(*x).colliding);

            colliding_tile.unwrap_or(true)
        }, |(dirs, pos)|
        {
            return;
            dirs.for_each(|dir, x|
            {
                if !x
                {
                    let tilepos = Vector3::from(pos) + Vector3::repeat(TILE_SIZE / 2.0);
                    let dirv: Vector3<i32> = Pos3::from(dir).into();

                    let mut scale = Vector3::repeat(TILE_SIZE).component_mul(&dirv.abs().cast());
                    let mut position = tilepos + Vector3::repeat(TILE_SIZE / 2.0).component_mul(&dirv.cast());
                    position.z = tilepos.z;

                    let color: [i32; 3] = Pos3::from(dir).into();
                    let mut color = color.map(|x| x.abs() as f32);

                    use crate::common::PosDirection;
                    if dir == PosDirection::Back
                    {
                        scale.x = TILE_SIZE;
                        scale.y = TILE_SIZE;

                        color[2] = 0.2;
                    }

                    if dir == PosDirection::Forward
                    {
                        scale.x = TILE_SIZE / 2.0;
                        scale.y = TILE_SIZE / 2.0;
                    }

                    let z_level = match dir
                    {
                        PosDirection::Back => ZLevel::UiMiddle,
                        PosDirection::Forward => ZLevel::UiHigher,
                        _ => ZLevel::UiHigh
                    };

                    use crate::common::{Pos3, EntityInfo, render_info::*, watcher::*, AnyEntities};
                    entities.push(true, EntityInfo{
                        transform: Some(Transform{
                            position,
                            scale: scale.yxz() + Vector3::repeat(TILE_SIZE / 7.0),
                            ..Default::default()
                        }),
                        render: Some(RenderInfo{
                            object: Some(RenderObjectKind::Texture{
                                name: "placeholder.png".to_owned()
                            }.into()),
                            z_level,
                            mix: Some(MixColor{color, amount: 0.9}),
                            ..Default::default()
                        }),
                        watchers: Some(Watchers::simple_one_frame()),
                        ..Default::default()
                    });
                }
            });
        }).count();

        collided > 0
    }
}
