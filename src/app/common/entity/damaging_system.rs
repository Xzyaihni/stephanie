use std::{
    f32,
    cell::RefCell
};

use nalgebra::{Unit, Vector3};

use crate::common::{
    angle_between,
    short_rotation,
    damage::*,
    damaging::*,
    character::*,
    render_info::*,
    watcher::*,
    particle_creator::*,
    ENTITY_SCALE,
    EntityInfo,
    PhysicalProperties,
    Message,
    Side2d,
    AnyEntities,
    Entity,
    EntityPasser,
    entity::{iterate_components_with, ClientEntities}
};

use yanyaengine::TextureId;


pub fn entity_damager<'a>(
    entities: &'a ClientEntities,
    passer: &'a mut impl EntityPasser,
    blood_texture: TextureId
) -> impl FnMut(Entity, f32, Faction, DamagePartial) + 'a
{
    move |entity, angle, faction, damage|
    {
        let entity_rotation = if let Some(transform) = entities.transform(entity)
        {
            transform.rotation
        } else
        {
            return;
        };

        let relative_rotation = angle - (-entity_rotation);
        let damage = damage.with_direction(Side2d::from_angle(relative_rotation));

        let damaged = entities.damage_entity_common(entity, faction, damage.clone());

        if damaged
        {
            let direction = Unit::new_unchecked(
                Vector3::new(-angle.cos(), angle.sin(), 0.0)
            );

            passer.send_message(Message::EntityDamage{entity, faction, damage});

            let scale = Vector3::repeat(ENTITY_SCALE * 0.1)
                .component_mul(&Vector3::new(4.0, 1.0, 1.0));

            entities.watchers_mut(entity).unwrap().push(Watcher{
                kind: WatcherType::Instant,
                action: WatcherAction::Explode(Box::new(ExplodeInfo{
                    keep: true,
                    info: ParticlesInfo{
                        amount: 2..4,
                        speed: ParticleSpeed::DirectionSpread{
                            direction,
                            speed: 1.7..=2.0,
                            spread: 0.2
                        },
                        decay: ParticleDecay::Random(7.0..=10.0),
                        position: ParticlePosition::Spread(0.1),
                        rotation: ParticleRotation::Exact(f32::consts::PI - angle),
                        scale: ParticleScale::Spread{scale, variation: 0.1},
                        min_scale: ENTITY_SCALE * 0.15
                    },
                    prototype: EntityInfo{
                        physical: Some(PhysicalProperties{
                            inverse_mass: 0.05_f32.recip(),
                            floating: true,
                            ..Default::default()
                        }.into()),
                        render: Some(RenderInfo{
                            object: Some(RenderObjectKind::TextureId{
                                id: blood_texture
                            }.into()),
                            z_level: ZLevel::Knee,
                            ..Default::default()
                        }),
                        ..Default::default()
                    }
                })),
                ..Default::default()
            });
        }
    }
}

pub fn update(
    entities: &mut ClientEntities,
    passer: &mut impl EntityPasser,
    blood_texture: TextureId
)
{
    struct DamagingResult
    {
        collided: Entity,
        angle: f32,
        faction: Faction,
        damage: DamagePartial
    }

    // "zero" "cost" "abstractions" "borrow" "checker"
    let damage_entities = iterate_components_with!(entities, damaging, flat_map, |entity, damaging: &RefCell<Damaging>|
    {
        let collider = entities.collider(entity).unwrap();

        collider.collided().iter().copied().filter_map(|collided|
        {
            let mut damaging = damaging.borrow_mut();

            let parent_angle_between = || -> Option<_>
            {
                let parent = entities.parent(entity)?.entity;

                let parent_transform = entities.transform(parent)?;
                let collided_transform = entities.transform(collided)?;

                let angle = angle_between(
                    parent_transform.position,
                    collided_transform.position
                );

                let parent_angle = -parent_transform.rotation;
                let relative_angle = angle + (f32::consts::PI - parent_angle);

                Some(short_rotation(relative_angle))
            };

            if damaging.can_damage(collided)
                && damaging.predicate.meets(|| parent_angle_between().unwrap_or(0.0))
            {
                damaging.damaged(collided);

                let collision_info = || -> Option<_>
                {
                    let source_entity = if let Some(other) = damaging.source
                    {
                        other
                    } else
                    {
                        entity
                    };

                    let this_transform = entities.transform(source_entity)?;
                    let collided_transform = entities.transform(collided)?;

                    let this_physical = entities.physical(entity);
                    let collided_physical = entities.physical(collided);

                    Some(CollisionInfo::new(
                        &this_transform,
                        &collided_transform,
                        this_physical.as_deref(),
                        collided_physical.as_deref()
                    ))
                };

                return damaging.damage.as_damage(collision_info).map(|(angle, damage)|
                {
                    DamagingResult{collided, angle, faction: damaging.faction, damage}
                });
            }

            None
        }).collect::<Vec<_>>()
    }).collect::<Vec<_>>();

    let mut damager = entity_damager(entities, passer, blood_texture);
    damage_entities.into_iter().for_each(|DamagingResult{
        collided,
        angle,
        faction,
        damage
    }|
    {
        damager(collided, angle, faction, damage)
    });
}

pub fn damage(entities: &impl AnyEntities, entity: Entity, damage: Damage)
{
    let flash_white = |entity: Entity|
    {
        if let Some(mut watchers) = entities.watchers_mut(entity)
        {
            if let Some(mut mix_color) = entities.mix_color_target(entity)
            {
                *mix_color = Some(MixColor{color: [1.0; 4], amount: 0.8, keep_transparency: true});

                watchers.push(
                    Watcher{
                        kind: WatcherType::Lifetime(0.2.into()),
                        action: WatcherAction::SetMixColor(None),
                        ..Default::default()
                    }
                );
            }
        }
    };

    flash_white(entity);
    entities.for_every_child(entity, flash_white);

    if let Some(mut anatomy) = entities.anatomy_mut(entity)
    {
        anatomy.damage(damage);
    }
}
