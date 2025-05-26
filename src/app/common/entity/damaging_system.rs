use std::{
    f32,
    cell::RefCell
};

use parking_lot::RwLock;

use nalgebra::{Unit, Vector3};

use serde::{Serialize, Deserialize};

use crate::{
    client::CommonTextures,
    common::{
        some_or_value,
        angle_between,
        short_rotation,
        damage::*,
        damaging::*,
        character::*,
        render_info::*,
        watcher::*,
        particle_creator::*,
        raycast::*,
        ENTITY_SCALE,
        EntityInfo,
        PhysicalProperties,
        Transform,
        Message,
        Side2d,
        AnyEntities,
        Entity,
        EntityPasser,
        World,
        entity::{iterate_components_with, raycast_system, ClientEntities},
        world::{TILE_SIZE, TilePos}
    }
};


const HIGHLIGHT_DURATION: f32 = 0.2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DamagingKind
{
    Entity(Entity, Faction),
    Tile(TilePos)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DamagingResult
{
    pub kind: DamagingKind,
    pub angle: f32,
    pub damage: DamagePartial
}

pub fn handle_message<E: AnyEntities, TileDamager: FnMut(TilePos, DamagePartial)>(
    entities: &E,
    textures: Option<&CommonTextures>,
    message: Message,
    damage_tile: TileDamager
) -> Option<Message>
{
    match message
    {
        Message::Damage(damage) =>
        {
            damager::<E, (), TileDamager>(entities, None, textures, damage_tile)(damage);

            None
        },
        x => Some(x)
    }
}

pub fn damager<'a, 'b, E: AnyEntities, Passer: EntityPasser, TileDamager: FnMut(TilePos, DamagePartial)>(
    entities: &'a E,
    mut passer: Option<&'b RwLock<Passer>>,
    textures: Option<&'a CommonTextures>,
    mut damage_tile: TileDamager
) -> impl FnMut(DamagingResult) + use<'a, 'b, Passer, E, TileDamager>
{
    move |result|
    {
        let angle = result.angle;
        let damage = result.damage.clone();

        let create_particles = |textures: &CommonTextures|
        {
            let direction = Unit::new_unchecked(
                Vector3::new(-angle.cos(), angle.sin(), 0.0)
            );

            let scale = Vector3::repeat(ENTITY_SCALE * 0.1)
                .component_mul(&Vector3::new(4.0, 1.0, 1.0));

            Watcher{
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
                        rotation: ParticleRotation::Exact(-angle),
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
                                id: textures.blood
                            }.into()),
                            z_level: ZLevel::Knee,
                            ..Default::default()
                        }),
                        ..Default::default()
                    }
                })),
                ..Default::default()
            }
        };

        match result.kind
        {
            DamagingKind::Entity(entity, faction) =>
            {
                let entity_rotation = if let Some(transform) = entities.transform(entity)
                {
                    transform.rotation
                } else
                {
                    return;
                };

                let relative_rotation = angle + entity_rotation;
                let damage = damage.with_direction(Side2d::from_angle(relative_rotation));

                if let Some(other) = entities.character(entity).map(|x| x.faction)
                {
                    if !faction.aggressive(&other)
                    {
                        return;
                    }
                } else
                {
                    return;
                }

                let mut damaged = false;
                if entities.anatomy_exists(entity)
                {
                    damage_entity(entities, entity, damage);

                    damaged = true;
                }

                if !damaged
                {
                    return;
                }

                if let Some(textures) = textures.as_ref()
                {
                    entities.watchers_mut(entity).unwrap().push(create_particles(textures));

                    flash_white(entities, entity);
                }
            },
            DamagingKind::Tile(tile_pos) =>
            {
                damage_tile(tile_pos, damage);

                if let Some(textures) = textures.as_ref()
                {
                    entities.push(true, EntityInfo{
                        render: Some(RenderInfo{
                            object: Some(RenderObjectKind::TextureId{
                                id: textures.solid
                            }.into()),
                            above_world: true,
                            mix: Some(MixColor::color([1.0, 1.0, 1.0, 0.005])),
                            ..Default::default()
                        }),
                        transform: Some(Transform{
                            position: Vector3::from(tile_pos.position()) + Vector3::repeat(TILE_SIZE / 2.0),
                            scale: Vector3::repeat(TILE_SIZE),
                            ..Default::default()
                        }),
                        watchers: Some(Watchers::new(vec![
                            create_particles(textures),
                            Watcher{
                                kind: WatcherType::Lifetime(HIGHLIGHT_DURATION.into()),
                                action: WatcherAction::Remove,
                                ..Default::default()
                            }
                        ])),
                        ..Default::default()
                    });
                }
            }
        }

        if let Some(passer) = passer.as_mut()
        {
            passer.write().send_message(Message::Damage(result));
        }
    }
}

fn damaging_raycasting(
    entities: &ClientEntities,
    world: &World,
    damaging: &mut Damaging,
    entity: Entity
) -> Vec<DamagingResult>
{
    let info;
    let damage;
    let start;
    let target;
    let scale_pierce;

    if let DamagingType::Raycast{
        info: this_info,
        damage: this_damage,
        start: this_start,
        target: this_target,
        scale_pierce: this_scale_pierce
    } = &damaging.damage
    {
        info = this_info;
        damage = this_damage;
        start = this_start;
        target = this_target;
        scale_pierce = this_scale_pierce;
    } else
    {
        unreachable!()
    }

    let hits = raycast_system::raycast(
        entities,
        world,
        info.clone(),
        &start,
        &target
    );

    hits.hits.iter().map(|hit|
    {
        let angle = hits.direction.x.acos();

        let kind = match hit.id
        {
            RaycastHitId::Entity(entity) => DamagingKind::Entity(entity, damaging.faction),
            RaycastHitId::Tile(tile) => DamagingKind::Tile(tile)
        };

        entities.remove_deferred(entity);

        let mut damage = damage.clone();

        if *scale_pierce
        {
            damage.data *= hit.result.pierce.min(1.0);
        }

        DamagingResult{kind, angle, damage}
    }).collect()
}

fn damaging_colliding(
    entities: &ClientEntities,
    damaging: &mut Damaging,
    entity: Entity
) -> Vec<DamagingResult>
{
    let collider = entities.collider(entity).unwrap();

    let parent_angle_between = |collided_position| -> Option<_>
    {
        let parent = entities.parent(entity)?.entity;

        let parent_transform = entities.transform(parent)?;

        let angle = angle_between(
            parent_transform.position,
            collided_position
        );

        let parent_angle = parent_transform.rotation;
        let relative_angle = angle + parent_angle;

        Some(short_rotation(relative_angle))
    };

    let meets_predicate = |damaging: &Damaging, collided_position|
    {
        damaging.predicate.meets(|| parent_angle_between(collided_position).unwrap_or(0.0))
    };

    let source_entity = if let Some(other) = damaging.source
    {
        other
    } else
    {
        entity
    };

    let this_transform = some_or_value!(entities.transform(source_entity), Vec::new());

    let faction = damaging.faction;
    let same_tile_z = damaging.same_tile_z;
    collider.collided_tiles().iter().copied().filter_map(|tile_pos|
    {
        let position: Vector3<f32> = tile_pos.position().into();

        if same_tile_z
        {
            if ((position.z + TILE_SIZE / 2.0) - this_transform.position.z).abs() > (TILE_SIZE / 2.0)
            {
                return None;
            }
        }

        let transform = Transform{
            position,
            scale: Vector3::repeat(TILE_SIZE),
            ..Default::default()
        };

        Some((
            transform,
            None,
            DamagingKind::Tile(tile_pos),
            DamagedId::Tile(tile_pos)
        ))
    }).chain(collider.collided().iter().copied().filter_map(|collided|
    {
        let collided_transform = entities.transform(collided)?.clone();
        let collided_physical = entities.physical(collided);

        Some((
            collided_transform,
            collided_physical,
            DamagingKind::Entity(collided, faction),
            DamagedId::Entity(collided)
        ))
    })).filter_map(|(collided_transform, collided_physical, kind, id)|
    {
        if damaging.can_damage(&id) && meets_predicate(&damaging, collided_transform.position)
        {
            damaging.damaged(id);

            damaging.damage.as_damage(||
            {
                let this_physical = entities.physical(entity);

                Some(CollisionInfo::new(
                    &this_transform,
                    &collided_transform,
                    this_physical.as_deref(),
                    collided_physical.as_deref()
                ))
            }).map(|(angle, damage)|
            {
                DamagingResult{kind, angle, damage}
            })
        } else
        {
            None
        }
    }).collect::<Vec<_>>()
}

pub fn update<Passer: EntityPasser>(
    entities: &mut ClientEntities,
    world: &mut World,
    passer: &RwLock<Passer>,
    textures: &CommonTextures
)
{
    // "zero" "cost" "abstractions" "borrow" "checker"
    let damage_entities = {
        let entities: &ClientEntities = entities;

        iterate_components_with!(entities, damaging, flat_map, |entity, damaging: &RefCell<Damaging>|
        {
            let mut damaging = damaging.borrow_mut();

            match &damaging.damage
            {
                DamagingType::Raycast{..} =>
                {
                    damaging_raycasting(entities, world, &mut damaging, entity)
                },
                _ => damaging_colliding(entities, &mut damaging, entity)
            }
        }).collect::<Vec<_>>()
    };

    damage_entities.into_iter().for_each(damager(entities, Some(passer), Some(textures), |tile_pos, damage|
    {
        world.modify_tile(tile_pos, |world, tile|
        {
            tile.damage(world.tilemap(), damage.data);
        })
    }));
}

fn flash_white_single(entities: &impl AnyEntities, entity: Entity)
{
    if let Some(mut watchers) = entities.watchers_mut(entity)
    {
        if let Some(mut mix_color) = entities.mix_color_target(entity)
        {
            *mix_color = Some(MixColor{color: [1.0; 4], amount: 0.8, keep_transparency: true});

            watchers.push(
                Watcher{
                    kind: WatcherType::Lifetime(HIGHLIGHT_DURATION.into()),
                    action: WatcherAction::SetMixColor(None),
                    ..Default::default()
                }
            );
        }
    }
}

fn flash_white(entities: &impl AnyEntities, entity: Entity)
{
    flash_white_single(entities, entity);
    entities.for_every_child(entity, |child| flash_white_single(entities, child));
}

pub fn damage_entity(entities: &impl AnyEntities, entity: Entity, damage: Damage)
{
    if let Some(enemy) = entities.enemy(entity)
    {
        if !enemy.is_attacking()
        {
            let change = damage.direction.side.to_angle();
            if let Some(mut character) = entities.character_mut(entity)
            {
                if entities.anatomy(entity).map(|x| x.speed().is_some()).unwrap_or(false)
                {
                    if let Some(x) = character.rotation_mut()
                    {
                        *x -= change;
                    }
                }
            }
        }
    }

    if let Some(mut anatomy) = entities.anatomy_mut(entity)
    {
        anatomy.damage(damage);
    }
}
