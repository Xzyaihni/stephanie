use std::{
    f32,
    cell::RefCell
};

use crate::common::{
    angle_between,
    short_rotation,
    damage::*,
    damaging::*,
    character::*,
    Entity,
    EntityPasser,
    entity::{iterate_components_with, ClientEntities}
};

use yanyaengine::TextureId;


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

    damage_entities.into_iter().for_each(|DamagingResult{
        collided,
        angle,
        faction,
        damage
    }|
    {
        entities.damage_entity(passer, blood_texture, angle, collided, faction, damage);
    });
}
