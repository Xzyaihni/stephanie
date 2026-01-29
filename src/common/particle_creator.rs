use std::ops::{Range, RangeInclusive};

use nalgebra::{vector, Vector2, Vector3, Unit, Rotation as NRotation};

use serde::{Serialize, Deserialize};

use yanyaengine::Transform;

use crate::{
    client::CommonTextures,
    common::{
        with_z,
        random_rotation,
        random_f32,
        angle_to_direction_3d,
        ENTITY_PIXEL_SCALE,
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
    SameDirectionRandom{direction: Unit<Vector3<f32>>, speed: RangeInclusive<f32>},
    Random(RangeInclusive<f32>),
    DirectionSpread{direction: Unit<Vector3<f32>>, speed: RangeInclusive<f32>, spread: f32}
}

impl ParticleSpeed
{
    fn velocity(&self) -> Vector3<f32>
    {
        match self
        {
            Self::SameDirectionRandom{direction, speed} =>
            {
                **direction * random_f32(speed.clone())
            },
            Self::Random(speed) =>
            {
                *angle_to_direction_3d(random_rotation()) * random_f32(speed.clone())
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
    Exact(Vector3<f32>),
    Spread{scale: Vector3<f32>, variation: f32}
}

impl ParticleScale
{
    pub fn get(&self) -> Vector3<f32>
    {
        match self
        {
            Self::Exact(scale) => *scale,
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
                    fn r() -> f32 { fastrand::f32() * 2.0 - 1.0 }

                    let offset = with_z(parent_scale.xy().component_mul(&vector![r(), r()]), 0.0);
                    target.position += offset * mult;
                }
            }

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
                from: Vector2::zeros(),
                near: info.min_scale
            },
            action: Box::new(|entities, entity| entities.remove(entity)),
            ..Default::default()
        });
    })
}

#[derive(Debug, Clone, Copy)]
pub struct ParticleDirection
{
    pub weak: bool,
    pub angle: f32
}

impl ParticleDirection
{
    fn direction(&self) -> Unit<Vector3<f32>>
    {
        Unit::new_unchecked(Vector3::new(-self.angle.cos(), self.angle.sin(), 0.0))
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ParticlesKind
{
    Blood{direction: Option<ParticleDirection>},
    Dust{direction: Option<ParticleDirection>},
    Heal,
    LevelUp
}

impl ParticlesKind
{
    pub fn create(self, textures: &CommonTextures) -> ExplodeInfo
    {
        let keep = false;
        let min_scale = (3.0 / ENTITY_PIXEL_SCALE as f32) * ENTITY_SCALE;

        match self
        {
            Self::Blood{direction} =>
            {
                let scale = with_z(
                    textures.blood.scale * if direction.map(|x| x.weak).unwrap_or(false) { 0.8 } else { 1.0 },
                    1.0
                );

                let speed = 0.5..=0.7;

                ExplodeInfo{
                    keep,
                    info: ParticlesInfo{
                        amount: 2..4,
                        speed: direction.map(|x|
                        {
                            ParticleSpeed::DirectionSpread{
                                direction: x.direction(),
                                speed: if x.weak { speed.clone() } else { 1.7..=2.0 },
                                spread: 0.2
                            }
                        }).unwrap_or_else(||
                        {
                            ParticleSpeed::Random(speed.clone())
                        }),
                        decay: ParticleDecay::Random(7.0..=10.0),
                        position: ParticlePosition::Spread(0.1),
                        rotation: ParticleRotation::Exact(0.0),
                        scale: ParticleScale::Spread{scale, variation: 0.1},
                        min_scale
                    },
                    prototype: EntityInfo{
                        physical: Some(PhysicalProperties{
                            inverse_mass: 0.05_f32.recip(),
                            floating: true,
                            ..Default::default()
                        }.into()),
                        render: Some(RenderInfo{
                            object: Some(RenderObjectKind::TextureId{
                                id: textures.blood.id
                            }.into()),
                            z_level: ZLevel::Knee,
                            ..Default::default()
                        }),
                        ..Default::default()
                    }
                }
            },
            Self::Dust{direction} =>
            {
                let scale = with_z(
                    textures.dust.scale * if direction.map(|x| x.weak).unwrap_or(false) { 0.8 } else { 1.0 },
                    1.0
                );

                let speed = 0.08..=0.1;

                ExplodeInfo{
                    keep,
                    info: ParticlesInfo{
                        amount: 2..4,
                        speed: direction.map(|x|
                        {
                            ParticleSpeed::DirectionSpread{
                                direction: x.direction(),
                                speed: if x.weak { speed.clone() } else { 0.4..=0.5 },
                                spread: if x.weak { 1.0 } else { 0.3 }
                            }
                        }).unwrap_or_else(||
                        {
                            ParticleSpeed::Random(speed.clone())
                        }),
                        decay: ParticleDecay::Random(0.7..=1.0),
                        position: ParticlePosition::Spread(0.1),
                        rotation: ParticleRotation::Random,
                        scale: ParticleScale::Spread{scale, variation: 0.1},
                        min_scale
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
                                id: textures.dust.id
                            }.into()),
                            z_level: ZLevel::Knee,
                            ..Default::default()
                        }),
                        ..Default::default()
                    }
                }
            },
            Self::Heal =>
            {
                let scale = with_z(textures.health.scale, 1.0);

                ExplodeInfo{
                    keep,
                    info: ParticlesInfo{
                        amount: 4..6,
                        speed: ParticleSpeed::SameDirectionRandom{
                            direction: Unit::new_unchecked(vector![0.0, -1.0, 0.0]),
                            speed: 0.04..=0.05
                        },
                        decay: ParticleDecay::Random(0.7..=1.0),
                        position: ParticlePosition::Spread(1.1),
                        rotation: ParticleRotation::Exact(0.0),
                        scale: ParticleScale::Exact(scale),
                        min_scale
                    },
                    prototype: EntityInfo{
                        physical: Some(PhysicalProperties{
                            inverse_mass: 0.01_f32.recip(),
                            floating: true,
                            damping: 0.9,
                            ..Default::default()
                        }.into()),
                        render: Some(RenderInfo{
                            object: Some(RenderObjectKind::TextureId{
                                id: textures.health.id
                            }.into()),
                            z_level: ZLevel::AboveParticle,
                            ..Default::default()
                        }),
                        ..Default::default()
                    }
                }
            },
            Self::LevelUp =>
            {
                let scale = with_z(textures.level_up.scale, 1.0);

                ExplodeInfo{
                    keep,
                    info: ParticlesInfo{
                        amount: 4..6,
                        speed: ParticleSpeed::SameDirectionRandom{
                            direction: Unit::new_unchecked(vector![0.0, -1.0, 0.0]),
                            speed: 0.1..=0.12
                        },
                        decay: ParticleDecay::Random(0.7..=1.0),
                        position: ParticlePosition::Spread(1.1),
                        rotation: ParticleRotation::Exact(0.0),
                        scale: ParticleScale::Exact(scale),
                        min_scale
                    },
                    prototype: EntityInfo{
                        physical: Some(PhysicalProperties{
                            inverse_mass: 0.01_f32.recip(),
                            floating: true,
                            damping: 0.05,
                            ..Default::default()
                        }.into()),
                        render: Some(RenderInfo{
                            object: Some(RenderObjectKind::TextureId{
                                id: textures.level_up.id
                            }.into()),
                            z_level: ZLevel::AboveParticle,
                            ..Default::default()
                        }),
                        ..Default::default()
                    }
                }
            }
        }
    }
}
