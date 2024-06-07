use std::ops::Range;

use nalgebra::Vector3;

use serde::{Serialize, Deserialize};

use crate::common::{random_rotation, AnyEntities, EntityInfo};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParticleSpeed
{
    Random(f32),
    DirectionSpread(Vector3<f32>)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticlesInfo
{
    pub amount: Range<usize>,
    pub speed: ParticleSpeed
}

pub struct ParticleCreator
{
}

impl ParticleCreator
{
    pub fn create_particles<E: AnyEntities>(
        entities: &E,
        info: ParticlesInfo,
        prototype: EntityInfo
    )
    {
        /*let amount = fastrand::usize(info.amount);
        (0..amount).for_each(|_|
        {
            if let Some(target) = info.info.target()
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

            if let Some(physical) = info.info.physical.as_mut()
            {
                let r = random_rotation();
                let velocity = Vector3::new(r.cos(), r.sin(), 0.0) * info.speed;
                physical.velocity = parent_velocity.unwrap_or_default() + velocity;
                physical.velocity.z = 0.0;
            }

            // for now all watcher created entities r local (i might change that?)
            entities.push(true, info.info.clone());
        })*/
        /*ExplodeInfo{
            keep: false,
            amount: 3..5,
            speed: 0.1,
            info: EntityInfo{
                lazy_transform: Some(LazyTransformInfo{
                    scaling: Scaling::EaseOut{decay: 4.0},
                    transform: Transform{
                        scale: Vector3::repeat(ENTITY_SCALE * 0.4),
                        ..Default::default()
                    },
                    ..Default::default()
                }.into()),
                watchers: Some(Watchers::new(vec![
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
                ])),
                ..Default::default()
            }
        }*/
    }
}
