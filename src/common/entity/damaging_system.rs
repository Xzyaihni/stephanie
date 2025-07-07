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
        some_or_return,
        angle_between,
        opposite_angle,
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

enum ParticlesKind
{
    Blood,
    Dust
}

impl ParticlesKind
{
    fn create(self, textures: &CommonTextures, weak: bool, angle: f32) -> Watcher
    {
        let direction = Unit::new_unchecked(
            Vector3::new(-angle.cos(), angle.sin(), 0.0)
        );

        let keep = false;

        let info = match self
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
        };

        Watcher{
            kind: WatcherType::Instant,
            action: WatcherAction::Explode(Box::new(info)),
            ..Default::default()
        }
    }
}

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
    pub other_entity: Entity,
    pub damage_entry: Vector3<f32>,
    pub damage_exit: Option<Vector3<f32>>,
    pub angle: f32,
    pub damage: DamagePartial
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

        let create_particles = |textures: &CommonTextures, kind: ParticlesKind, weak: bool, position: Vector3<f32>|
        {
            let angle = if weak
            {
                angle
            } else
            {
                opposite_angle(angle)
            };

            let watcher = kind.create(textures, weak, angle);

            entities.push(true, EntityInfo{
                transform: Some(Transform{
                    position,
                    scale: Vector3::repeat(ENTITY_SCALE),
                    ..Default::default()
                }),
                watchers: Some(Watchers::new(vec![watcher])),
                ..Default::default()
            });
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

                let damage = {
                    let character = some_or_return!(entities.character(entity));
                    let (height, angle) = character.remap_direction(damage.height, Side2d::from_angle(relative_rotation));

                    if !faction.aggressive(&character.faction)
                    {
                        return;
                    }

                    DamagePartial{height, ..damage}.with_direction(angle)
                };

                let mut damaged = false;
                if entities.anatomy_exists(entity)
                {
                    damage_entity(entities, entity, result.other_entity, damage);

                    if let Some(passer) = passer.as_mut()
                    {
                        passer.write().send_message(Message::SetAnatomy{
                            entity,
                            component: Box::new(entities.anatomy(entity).unwrap().clone())
                        });
                    }

                    damaged = true;
                }

                if !damaged
                {
                    return;
                }

                if let Some(textures) = textures.as_ref()
                {
                    create_particles(textures, ParticlesKind::Blood, true, result.damage_entry);
                    if let Some(position) = result.damage_exit
                    {
                        create_particles(textures, ParticlesKind::Blood, false, position);
                    }

                    flash_white(entities, entity);
                }
            },
            DamagingKind::Tile(tile_pos) =>
            {
                damage_tile(tile_pos, damage);

                if let Some(textures) = textures.as_ref()
                {
                    create_particles(textures, ParticlesKind::Dust, true, result.damage_entry);
                    if let Some(position) = result.damage_exit
                    {
                        create_particles(textures, ParticlesKind::Dust, false, position);
                    }

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
        start,
        target
    );

    let hits_len = hits.hits.len();
    hits.hits.iter().enumerate().map(|(index, hit)|
    {
        let angle = (-hits.direction.y).atan2(hits.direction.x);

        let kind = match hit.id
        {
            RaycastHitId::Entity(entity) => DamagingKind::Entity(entity, damaging.faction),
            RaycastHitId::Tile(tile) => DamagingKind::Tile(tile)
        };

        entities.remove_deferred(entity);

        let other_entity = if let Some(source) = damaging.source
        {
            source
        } else
        {
            entity
        };

        let mut damage = damage.clone();

        if let Some(s) = scale_pierce
        {
            damage.data *= (hit.result.pierce * s).min(1.0);
        }

        let is_last_hit = (index + 1) == hits_len;

        let (damage_entry, damage_exit) = hit.result.hit_points(hits.start, hits.direction);

        let damage_exit = if is_last_hit { None } else { damage_exit };

        DamagingResult{kind, other_entity, damage_entry, damage_exit, angle, damage}
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
        let position = Vector3::from(tile_pos.position()) + Vector3::repeat(TILE_SIZE / 2.0);

        if same_tile_z
        {
            if (position.z - this_transform.position.z).abs() > (TILE_SIZE / 2.0)
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
        if damaging.can_damage(&id) && meets_predicate(damaging, collided_transform.position)
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
                let direction = {
                    let angle = angle_between(collided_transform.position, this_transform.position);
                    Vector3::new(angle.cos(), -angle.sin(), 0.0)
                };

                let damage_entry = collided_transform.position
                    + direction.component_mul(&(collided_transform.scale * 0.5));

                let damage_exit = None;

                DamagingResult{
                    kind,
                    other_entity: source_entity,
                    damage_entry,
                    damage_exit,
                    angle: opposite_angle(angle),
                    damage
                }
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

fn turn_towards_other(
    entities: &impl AnyEntities,
    entity: Entity,
    other_entity: Entity,
)
{
    let mut enemy = some_or_return!(entities.enemy_mut(entity));

    if !enemy.is_attacking()
    {
        let mut character = some_or_return!(entities.character_mut(entity));
        let anatomy = some_or_return!(entities.anatomy(entity));

        if anatomy.speed().is_some()
        {
            let rotation = some_or_return!(character.rotation_mut());

            let this_position = some_or_return!(entities.transform(entity)).position;
            let other_position = some_or_return!(entities.transform(other_entity)).position;

            enemy.set_waiting();

            *rotation = -angle_between(this_position, other_position);
        }
    }
}

pub fn damage_entity(
    entities: &impl AnyEntities,
    entity: Entity,
    other_entity: Entity,
    damage: Damage
)
{
    turn_towards_other(entities, entity, other_entity);

    if let Some(mut anatomy) = entities.anatomy_mut(entity)
    {
        anatomy.damage(damage);
    }
}
