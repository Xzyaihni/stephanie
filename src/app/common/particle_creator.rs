use std::ops::Range;

use nalgebra::{Vector3, Unit, Rotation as NRotation};

use serde::{Serialize, Deserialize};

use yanyaengine::Transform;

use crate::common::{
    random_rotation,
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
    DirectionSpread(Vector3<f32>, f32)
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
            Self::DirectionSpread(direction, spread) =>
            {
                let angle = (fastrand::f32() * 2.0 - 1.0) * spread;
                let spread = NRotation::from_axis_angle(&Unit::new_normalize(Vector3::z()), angle);

                spread * direction
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticlesInfo
{
    pub amount: Range<usize>,
    pub speed: ParticleSpeed,
    pub decay: f32,
    pub scale: f32
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
        prototype.lazy_transform = Some(LazyTransformInfo{
            scaling: Scaling::EaseOut{decay: info.decay},
            transform: Transform{
                scale: Vector3::repeat(info.scale),
                ..Default::default()
            },
            ..Default::default()
        }.into());
        
        prototype.watchers = Some(Watchers::new(vec![
            Watcher{
                kind: WatcherType::Instant,
                action: WatcherAction::SetTargetScale(Vector3::zeros()),
                ..Default::default()
            },
            Watcher{
                kind: WatcherType::ScaleDistance{
                    from: Vector3::zeros(),
                    near: 0.01
                },
                action: WatcherAction::Remove,
                ..Default::default()
            }
        ]));

        let position;
        let scale;
        {
            let transform = entities.transform(entity).unwrap();

            position = transform.position;
            scale = transform.scale;
        }

        let parent_velocity = entities.physical(entity).map(|x| x.velocity);

        let amount = fastrand::usize(info.amount);
        (0..amount).for_each(|_|
        {
            if let Some(target) = prototype.target()
            {
                let r = ||
                {
                    2.0 * fastrand::f32()
                };

                let offset = scale - Vector3::new(scale.x * r(), scale.y * r(), 0.0);
                target.position = position + offset / 2.0;
                target.position.z = 0.0;

                target.rotation = random_rotation();
            }

            if let Some(physical) = prototype.physical.as_mut()
            {
                let velocity = info.speed.velocity();

                physical.velocity = parent_velocity.unwrap_or_default() + velocity;
                physical.velocity.z = 0.0;
            }

            // for now particles r local (i might change that?)
            entities.push(true, prototype.clone());
        })
    }
}
