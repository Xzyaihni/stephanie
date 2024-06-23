use serde::{Serialize, Deserialize};

use nalgebra::Vector3;

use yanyaengine::Transform;

use crate::common::{
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
    Ui
}

impl ColliderLayer
{
    pub fn collides(&self, other: &Self) -> bool
    {
        macro_rules! define_collisions
        {
            ($(($first:ident, $second:ident, $result:literal)),+) =>
            {
                #[allow(unreachable_patterns)]
                match (self, other)
                {
                    $(
                        (Self::$first, Self::$second) => $result,
                        (Self::$second, Self::$first) => $result
                    ),+
                }
            }
        }

        define_collisions!{
            (Normal, Normal, true),
            (Ui, Ui, true),
            (Normal, Ui, false),
            (Damage, Damage, false),
            (Damage, Normal, true),
            (Damage, Ui, false)
        }
    }
}

#[derive(Debug, Clone)]
pub struct ColliderInfo
{
    pub kind: ColliderType,
    pub layer: ColliderLayer,
    pub ghost: bool,
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

pub struct CollidingInfo<'a, F>
{
    pub entity: Entity,
    pub physical: Option<&'a mut Physical>,
    pub target: F,
    pub transform: Transform,
    pub collider: Collider
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

        let elasticity = 0.9;

        if self.collider.is_static
        {
            (other.target)(offset);
            if let Some(physical) = &mut other.physical
            {
                physical.invert_velocity();
                physical.velocity *= elasticity;
            }
        } else if other.collider.is_static
        {
            (self.target)(-offset);
            if let Some(physical) = &mut self.physical
            {
                physical.invert_velocity();
                physical.velocity *= elasticity;
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

                    (self.target)(-offset * this_scale);
                    (other.target)(offset * other_scale);
                },
                (Some(this_physical), None) =>
                {
                    (self.target)(-offset);
                    this_physical.invert_velocity();
                    this_physical.velocity *= elasticity;
                },
                (None, Some(other_physical)) =>
                {
                    (other.target)(offset);
                    other_physical.invert_velocity();
                    other_physical.velocity *= elasticity;
                },
                (None, None) =>
                {
                    let half_offset = offset / 2.0;
                    (self.target)(-half_offset);
                    (other.target)(half_offset);
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

        let offset = if (offset.x.abs() < offset.y.abs()) && (offset.x.abs() < offset.z.abs())
        {
            Vector3::new(offset.x, 0.0, 0.0)
        } else if (offset.y.abs() < offset.x.abs()) && (offset.y.abs() < offset.z.abs())
        {
            Vector3::new(0.0, offset.y, 0.0)
        } else
        {
            Vector3::new(0.0, 0.0, offset.z)
        };

        self.resolve_with(other, offset);
    }

    fn circle_circle<OtherF>(&mut self, other: &mut CollidingInfo<OtherF>) -> bool
    where
        OtherF: FnMut(Vector3<f32>)
    {
        let this_radius = self.transform.max_scale() / 2.0;
        let other_radius = other.transform.max_scale() / 2.0;

        let offset = other.transform.position - self.transform.position;
        let distance = (offset.x.powi(2) + offset.y.powi(2) + offset.z.powi(2)).sqrt();

        let max_distance = this_radius + other_radius;
        let collided = distance < max_distance;
        if collided
        {
            let direction = if distance == 0.0
            {
                Vector3::x()
            } else
            {
                offset.normalize()
            };

            let shift = max_distance - distance;

            self.resolve_with(other, direction * shift);
        }

        collided
    }

    fn normal_collision<OtherF>(&mut self, other: &mut CollidingInfo<OtherF>) -> bool
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

        if collided
        {
            self.resolve_with_offset(other, max_distance, offset);
        }

        collided
    }

    fn scale(&self) -> Vector3<f32>
    {
        match self.collider.kind
        {
            ColliderType::Point => Vector3::zeros(),
            ColliderType::Circle => Vector3::repeat(self.transform.max_scale() / 2.0),
            ColliderType::Aabb => self.transform.scale / 2.0
        }
    }

    pub fn resolve<OtherF>(
        mut self,
        mut other: CollidingInfo<OtherF>
    ) -> bool
    where
        OtherF: FnMut(Vector3<f32>)
    {
        if !self.collider.layer.collides(&other.collider.layer)
        {
            return false
        }

        let collided = match (self.collider.kind, other.collider.kind)
        {
            (ColliderType::Point, ColliderType::Point) => false,
            (ColliderType::Circle, ColliderType::Circle) =>
            {
                self.circle_circle(&mut other)
            },
            (ColliderType::Circle, ColliderType::Aabb)
            | (ColliderType::Aabb, ColliderType::Circle)
            | (ColliderType::Aabb, ColliderType::Aabb)
            | (ColliderType::Point, ColliderType::Aabb)
            | (ColliderType::Aabb, ColliderType::Point)
            | (ColliderType::Point, ColliderType::Circle)
            | (ColliderType::Circle, ColliderType::Point) =>
            {
                self.normal_collision(&mut other)
            }
        };

        self.collider.push_collided(other.entity);
        other.collider.push_collided(self.entity);

        collided
    }

    pub fn resolve_with_world(
        self,
        entities: &mut impl crate::common::AnyEntities,
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

        {
            use crate::common::{
                watcher::*,
                render_info::*
            };

            let mut make_thingy = |position, scale, name, z_level|
            {
                entities.push(true, crate::common::EntityInfo{
                    transform: Some(Transform{
                        position,
                        scale,
                        ..Default::default()
                    }),
                    render: Some(RenderInfo{
                        object: Some(RenderObject::Texture{name}),
                        z_level,
                        ..Default::default()
                    }),
                    watchers: Some(Watchers::new(vec![
                        Watcher{
                            kind: WatcherType::Lifetime(0.1.into()),
                            action: WatcherAction::Remove,
                            ..Default::default()
                        }
                    ])),
                    ..Default::default()
                });
            };

            let mut make_tile = |position|
            {
                make_thingy(
                    position,
                    Vector3::repeat(TILE_SIZE),
                    "placeholder.png".to_owned(),
                    ZLevel::Arms
                );
            };

            start_tile.tiles_between(end_tile).for_each(|tile|
            {
                make_tile(tile_pos(tile));
            });
        }

        false
    }
}
