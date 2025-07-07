use std::ops::{Range, RangeInclusive};

use nalgebra::{Vector3, Unit, Rotation as NRotation};

use serde::{Serialize, Deserialize};

use yanyaengine::Transform;

use crate::common::{
    random_rotation,
    random_f32,
    lazy_transform::*,
    watcher::*,
    AnyEntities,
    Entity,
    EntityInfo
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
        let angle_to_direction = |r: f32| -> Vector3<f32>
        {
            Vector3::new(r.cos(), r.sin(), 0.0)
        };

        match self
        {
            Self::Random(speed) =>
            {
                angle_to_direction(random_rotation()) * *speed
            },
            Self::DirectionSpread{direction, speed, spread} =>
            {
                let angle = random_f32(-spread..=*spread);
                let spread = NRotation::from_axis_angle(&Vector3::z_axis(), angle);

                spread * direction.into_inner() * random_f32(speed.clone())
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

pub struct ParticleCreator
{
}

impl ParticleCreator
{
    pub fn create_particles<E: AnyEntities>(
        entities: &mut E,
        entity: Entity,
        info: ParticlesInfo,
        mut prototype: EntityInfo
    )
    {
        prototype.watchers = Some(Watchers::new(vec![
            Watcher{
                kind: WatcherType::Instant,
                action: WatcherAction::SetTargetScale(Vector3::zeros()),
                ..Default::default()
            },
            Watcher{
                kind: WatcherType::ScaleDistance{
                    from: Vector3::zeros(),
                    near: info.min_scale
                },
                action: WatcherAction::Remove,
                ..Default::default()
            }
        ]));

        let position;
        let scale;
        {
            let transform = entities.transform(entity).expect("particle creator must have a transform");

            position = transform.position;
            scale = transform.scale;
        }

        let parent_velocity = entities.physical(entity).map(|x| *x.velocity());

        let amount = fastrand::usize(info.amount);
        (0..amount).for_each(|_|
        {
            let mut prototype = prototype.clone();
            prototype.lazy_transform = Some(LazyTransformInfo{
                scaling: Scaling::EaseOut{decay: info.decay.get()},
                transform: Transform{
                    scale: info.scale.get(),
                    ..Default::default()
                },
                ..Default::default()
            }.into());

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

                        let offset = scale - Vector3::new(scale.x * r(), scale.y * r(), 0.0);
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
                let velocity = info.speed.velocity();
                let mut velocity = parent_velocity.unwrap_or_default() + velocity;
                velocity.z = 0.0;

                physical.set_velocity_raw(velocity);
            }

            // for now particles r local (i might change that?)
            entities.push_eager(true, prototype);
        })
    }
}
