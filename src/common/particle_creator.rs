use std::ops::{Range, RangeInclusive};

use nalgebra::{Vector3, Unit, Rotation as NRotation};

use serde::{Serialize, Deserialize};

use yanyaengine::Transform;

use crate::{
    client::CommonTextures,
    common::{
        random_rotation,
        random_f32,
        angle_to_direction_3d,
        ENTITY_SCALE,
        lazy_transform::*,
        watcher::*,
        render_info::*,
        physics::*,
        EntityInfo,
        entity::ClientEntitiesPush
    }
};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParticleSpeed
{
    Random(f32),
    DirectionSpread{direction: Unit<Vector3<f32>>, speed: RangeInclusive<f32>, spread: f32}
}

impl ParticleSpeed
{
    fn velocity(&self) -> Vector3<f32>
    {
        match self
        {
            Self::Random(speed) =>
            {
                *angle_to_direction_3d(random_rotation()) * *speed
            },
            Self::DirectionSpread{direction, speed, spread} =>
            {
                let angle = random_f32(-spread..=*spread);
                let spread = NRotation::from_axis_angle(&Vector3::z_axis(), angle);

                spread * **direction * random_f32(speed.clone())
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParticleDecay
{
    Random(RangeInclusive<f32>)
}

impl ParticleDecay
{
    pub fn get(&self) -> f32
    {
        match self
        {
            ParticleDecay::Random(range) =>
            {
                random_f32(range.clone())
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParticlePosition
{
    Exact,
    Spread(f32)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParticleRotation
{
    Exact(f32),
    Random
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParticleScale
{
    Spread{scale: Vector3<f32>, variation: f32}
}

impl ParticleScale
{
    pub fn get(&self) -> Vector3<f32>
    {
        match self
        {
            Self::Spread{scale, variation} =>
            {
                let mult = 1.0 + random_f32(-variation..=*variation);

                scale * mult
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticlesInfo
{
    pub amount: Range<usize>,
    pub speed: ParticleSpeed,
    pub decay: ParticleDecay,
    pub position: ParticlePosition,
    pub rotation: ParticleRotation,
    pub scale: ParticleScale,
    pub min_scale: f32
}

pub fn create_particles(
    mut entities: impl ClientEntitiesPush,
    info: ParticlesInfo,
    prototype: EntityInfo,
    parent_scale: Vector3<f32>
)
{
    debug_assert!(prototype.transform.is_some());

    let position = prototype.transform.as_ref().map(|x| x.position).unwrap_or_default();

    let amount = fastrand::usize(info.amount);
    (0..amount).for_each(|_|
    {
        let mut prototype = prototype.clone();

        prototype.lazy_transform = Some(LazyTransformInfo{
            scaling: Scaling::EaseOut{decay: info.decay.get()},
            transform: Transform{
                scale: Vector3::zeros(),
                ..Default::default()
            },
            ..Default::default()
        }.into());

        prototype.transform = Some(Transform{
            scale: info.scale.get(),
            ..Default::default()
        });

        if let Some(target) = prototype.target()
        {
            target.position = position;

            match info.position
            {
                ParticlePosition::Exact => (),
                ParticlePosition::Spread(mult) =>
                {
                    let r = ||
                    {
                        2.0 * fastrand::f32()
                    };

                    let offset = parent_scale - Vector3::new(parent_scale.x * r(), parent_scale.y * r(), 0.0);
                    target.position += (offset / 2.0) * mult;
                }
            }

            target.position.z = position.z;

            target.rotation = match info.rotation
            {
                ParticleRotation::Exact(x) => x,
                ParticleRotation::Random => random_rotation()
            };
        }

        if let Some(physical) = prototype.physical.as_mut()
        {
            physical.add_velocity_raw(info.speed.velocity());
            physical.remove_velocity_axis(2);
        }

        let prototype_entity = entities.push(prototype);

        entities.entities_ref().add_watcher(prototype_entity, Watcher{
            kind: WatcherType::ScaleDistance{
                from: Vector3::zeros(),
                near: info.min_scale
            },
            action: Box::new(|entities, entity| entities.remove(entity)),
            ..Default::default()
        });
    })
}

#[derive(Debug, Clone, Copy)]
pub enum ParticlesKind
{
    Blood,
    Dust
}

impl ParticlesKind
{
    pub fn create(self, textures: &CommonTextures, weak: bool, angle: f32) -> ExplodeInfo
    {
        let direction = Unit::new_unchecked(
            Vector3::new(-angle.cos(), angle.sin(), 0.0)
        );

        let keep = false;

        match self
        {
            Self::Blood =>
            {
                let scale_single = ENTITY_SCALE * 0.1 * if weak { 0.8 } else { 1.0 };
                let scale = Vector3::repeat(scale_single)
                    .component_mul(&Vector3::new(4.0, 1.0, 1.0));

                ExplodeInfo{
                    keep,
                    info: ParticlesInfo{
                        amount: 2..4,
                        speed: ParticleSpeed::DirectionSpread{
                            direction,
                            speed: if weak { 0.5..=0.7 } else { 1.7..=2.0 },
                            spread: 0.2
                        },
                        decay: ParticleDecay::Random(7.0..=10.0),
                        position: ParticlePosition::Spread(0.1),
                        rotation: ParticleRotation::Exact(-angle),
                        scale: ParticleScale::Spread{scale, variation: 0.1},
                        min_scale: scale_single * 1.1
                    },
                    prototype: EntityInfo{
                        physical: Some(PhysicalProperties{
                            inverse_mass: 0.05_f32.recip(),
                            floating: true,
                            ..Default::default()
                        }.into()),
                        render: Some(RenderInfo{
                            object: Some(RenderObjectKind::TextureId{
                                id: textures.blood
                            }.into()),
                            z_level: ZLevel::Knee,
                            ..Default::default()
                        }),
                        ..Default::default()
                    }
                }
            },
            Self::Dust =>
            {
                let scale_single = ENTITY_SCALE * 0.3 * if weak { 0.8 } else { 1.0 };
                let scale = Vector3::repeat(scale_single);

                ExplodeInfo{
                    keep,
                    info: ParticlesInfo{
                        amount: 2..4,
                        speed: ParticleSpeed::DirectionSpread{
                            direction,
                            speed: if weak { 0.08..=0.1 } else { 0.4..=0.5 },
                            spread: if weak { 1.0 } else { 0.3 }
                        },
                        decay: ParticleDecay::Random(0.7..=1.0),
                        position: ParticlePosition::Spread(0.1),
                        rotation: ParticleRotation::Random,
                        scale: ParticleScale::Spread{scale, variation: 0.1},
                        min_scale: scale_single * 0.3
                    },
                    prototype: EntityInfo{
                        physical: Some(PhysicalProperties{
                            inverse_mass: 0.01_f32.recip(),
                            floating: true,
                            damping: 0.1,
                            ..Default::default()
                        }.into()),
                        render: Some(RenderInfo{
                            object: Some(RenderObjectKind::TextureId{
                                id: textures.dust
                            }.into()),
                            z_level: ZLevel::Knee,
                            ..Default::default()
                        }),
                        ..Default::default()
                    }
                }
            }
        }
    }
}
