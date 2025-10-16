use std::{
    f32,
    ops::ControlFlow,
    cell::RefCell
};

use nalgebra::Vector3;

use serde::{Serialize, Deserialize};

use crate::{
    debug_config::*,
    client::{ConnectionsHandler, CommonTextures},
    common::{
        with_z,
        some_or_value,
        some_or_return,
        angle_between,
        opposite_angle,
        short_rotation,
        random_rotation,
        angle_to_direction_3d,
        aabb_bounds,
        damage::*,
        damaging::*,
        character::*,
        render_info::*,
        watcher::*,
        particle_creator::*,
        raycast::*,
        collider::*,
        item::*,
        systems::{raycast_system, collider_system},
        ENTITY_SCALE,
        Loot,
        LootState,
        SpatialGrid,
        EntityInfo,
        Transform,
        Side2d,
        AnyEntities,
        Entity,
        World,
        enemy_creator::ENEMY_MASS,
        entity::{iterate_components_with, ClientEntities},
        world::{TILE_SIZE, TilePos}
    }
};


const HIGHLIGHT_DURATION: f32 = 0.2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DamagingKind
{
    Entity(Entity, Faction, f32),
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

pub fn damager<'a, 'b, 'c>(
    world: &'b mut World,
    space: &'a SpatialGrid,
    entities: &'a ClientEntities,
    loot: &'a Loot,
    passer: &'c mut ConnectionsHandler,
    textures: &'a CommonTextures
) -> impl FnMut(DamagingResult) + use<'a, 'b, 'c>
{
    move |result|
    {
        if DebugConfig::is_enabled(DebugTool::DamagingAllResults)
        {
            eprintln!("damaging: {result:#?}");
        }

        let angle = result.angle;
        let damage = result.damage.clone();

        let create_particles = |textures: &CommonTextures, kind: ParticlesKind, weak: bool, position: Vector3<f32>|
        {
            let angle = if weak
            {
                opposite_angle(angle)
            } else
            {
                angle
            };

            let info = kind.create(textures, weak, angle);
            create_particles(
                entities,
                info.info,
                EntityInfo{
                    transform: Some(Transform{
                        position,
                        ..Default::default()
                    }),
                    ..info.prototype
                },
                Vector3::repeat(ENTITY_SCALE)
            );
        };

        if DebugConfig::is_enabled(DebugTool::DamagePoints)
        {
            let make_point = |position, color|
            {
                let entity = entities.push(true, EntityInfo{
                    transform: Some(Transform{
                        position,
                        scale: Vector3::repeat(0.02),
                        ..Default::default()
                    }),
                    render: Some(RenderInfo{
                        object: Some(RenderObjectKind::Texture{
                            name: "circle.png".into()
                        }.into()),
                        mix: Some(MixColor{keep_transparency: true, ..MixColor::color(color)}),
                        above_world: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                });

                entities.add_watcher(entity, Watcher::simple_disappearing(1.0));
            };

            make_point(result.damage_entry, [1.0, 0.0, 0.0, 1.0]);

            if let Some(point) = result.damage_exit
            {
                make_point(point, [0.0, 0.0, 1.0, 1.0]);
            }
        }

        match result.kind
        {
            DamagingKind::Entity(entity, faction, knockback_factor) =>
            {
                let has_anatomy = entities.anatomy_exists(entity);

                if let Some(parent_sibling) = entities.sibling_first(entity)
                {
                    let result = DamagingResult{
                        kind: DamagingKind::Entity(parent_sibling, faction, knockback_factor),
                        ..result.clone()
                    };

                    damager(world, space, entities, loot, passer, textures)(result);
                }

                if !has_anatomy
                    && !entities.health_exists(entity)
                {
                    return;
                }

                let entity_rotation = if let Some(transform) = entities.transform(entity)
                {
                    transform.rotation
                } else
                {
                    return;
                };

                let relative_rotation = angle + entity_rotation;

                let damage = if let Some(character) = entities.character(entity)
                {
                    let (height, angle) = character.remap_direction(damage.height, Side2d::from_angle(relative_rotation));

                    if !faction.aggressive(&character.faction)
                    {
                        return;
                    }

                    DamagePartial{height, ..damage}.with_direction(angle)
                } else
                {
                    damage.with_direction(Side2d::default())
                };

                let is_organic = has_anatomy;
                let particle = if is_organic && damage.data.is_piercing()
                {
                    ParticlesKind::Blood
                } else
                {
                    ParticlesKind::Dust
                };

                damage_entity(entities, textures, loot, entity, result.other_entity, damage);

                create_particles(textures, particle, true, result.damage_entry);
                if let Some(position) = result.damage_exit
                {
                    create_particles(textures, particle, false, position);
                }

                let knockback_direction = angle_to_direction_3d(opposite_angle(result.angle));
                let knockback_strength = match result.damage.data
                {
                    DamageType::Sharp{sharpness, ..} => 1.0 - sharpness,
                    DamageType::Bullet(_) => if result.damage_exit.is_some() { 0.5 } else { 1.0 },
                    _ => 1.0
                };

                let knockback = *knockback_direction * (knockback_strength * knockback_factor * ENEMY_MASS * 30.0);
                knockback_entity(entities, entity, knockback);

                flash_white(entities, entity);

                if DebugConfig::is_enabled(DebugTool::DamagingPassedResults)
                {
                    eprintln!("passed: {result:#?}");
                }
            },
            DamagingKind::Tile(tile_pos) =>
            {
                let destroyed = world.modify_tile(passer, tile_pos, |world, tile|
                {
                    let previous_tile = *tile;
                    let tilemap = world.tilemap();
                    tile.damage(tilemap, damage.data).then(||
                    {
                        tilemap.info(previous_tile).name.clone()
                    })
                }).unwrap_or_default();

                let transform = Transform{
                    position: Vector3::from(tile_pos.position()) + Vector3::repeat(TILE_SIZE / 2.0),
                    scale: Vector3::repeat(TILE_SIZE),
                    ..Default::default()
                };

                if let Some(name) = destroyed
                {
                    destroy_tile_dependent(entities, textures, space, loot, tile_pos);
                    spawn_items(entities, textures, loot, &transform, &name);
                }

                create_particles(textures, ParticlesKind::Dust, true, result.damage_entry);
                if let Some(position) = result.damage_exit
                {
                    create_particles(textures, ParticlesKind::Dust, false, position);
                }

                let entity = entities.push(true, EntityInfo{
                    render: Some(RenderInfo{
                        object: Some(RenderObjectKind::TextureId{
                            id: textures.solid
                        }.into()),
                        above_world: true,
                        mix: Some(MixColor::color([1.0, 1.0, 1.0, 0.005])),
                        ..Default::default()
                    }),
                    transform: Some(transform),
                    ..Default::default()
                });

                entities.add_watcher(entity, Watcher{
                    kind: WatcherType::Lifetime(HIGHLIGHT_DURATION.into()),
                    action: Box::new(|entities, entity| entities.remove(entity)),
                    ..Default::default()
                });
            }
        }
    }
}

fn damaging_raycasting(
    entities: &ClientEntities,
    space: &SpatialGrid,
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
        space,
        Some(world),
        info.clone(),
        *start,
        *target
    );

    let hits_len = hits.hits.len();
    hits.hits.iter().enumerate().map(|(index, hit)|
    {
        let angle = hits.direction.y.atan2(-hits.direction.x);

        let kind = match hit.id
        {
            RaycastHitId::Entity(entity) => DamagingKind::Entity(entity, damaging.faction, 1.0),
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

    let source_entity = if let Some(other) = damaging.source
    {
        other
    } else
    {
        entity
    };

    let this_transform = some_or_value!(entities.transform(source_entity), Vec::new());

    let meets_predicate = |damaging: &Damaging, collided_position: Vector3<f32>|
    {
        match damaging.predicate
        {
            DamagingPredicate::None => true,
            DamagingPredicate::ParentAngleLess{angle: less, minimum_distance} =>
            {
                let parent = some_or_value!(entities.parent(entity), true).entity();

                let parent_transform = some_or_value!(entities.transform(parent), true);

                if collided_position.xy().metric_distance(&this_transform.position.xy()) < minimum_distance
                {
                    return true;
                }

                let parent_angle_between = |collided_position| -> Option<_>
                {
                    let angle = angle_between(
                        parent_transform.position,
                        collided_position
                    );

                    let parent_angle = parent_transform.rotation;
                    let relative_angle = angle + parent_angle;

                    Some(short_rotation(relative_angle))
                };

                let parent_angle_between = parent_angle_between(collided_position).unwrap_or(0.0);

                let angle = parent_angle_between.abs();
                angle < (less / 2.0)
            }
        }
    };

    let faction = damaging.faction;
    let knockback = damaging.knockback;
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
        if entities.collider(collided).map(|x| x.ghost).unwrap_or(true)
        {
            return None;
        }

        let collided_transform = entities.transform(collided)?.clone();
        let collided_physical = entities.physical(collided);

        Some((
            collided_transform,
            collided_physical,
            DamagingKind::Entity(collided, faction, knockback),
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
                    let angle = match kind
                    {
                        DamagingKind::Entity(_, _, _) => angle,
                        DamagingKind::Tile(_) =>
                        {
                            angle_between(collided_transform.position, this_transform.position)
                        }
                    };

                    *angle_to_direction_3d(angle)
                };

                let damage_entry = collided_transform.position
                    + direction.component_mul(&(collided_transform.scale * 0.5));

                let damage_exit = None;

                DamagingResult{
                    kind,
                    other_entity: source_entity,
                    damage_entry,
                    damage_exit,
                    angle,
                    damage
                }
            })
        } else
        {
            None
        }
    }).collect::<Vec<_>>()
}

pub fn update(
    entities: &mut ClientEntities,
    space: &SpatialGrid,
    world: &mut World,
    loot: &Loot,
    passer: &mut ConnectionsHandler,
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
                    damaging_raycasting(entities, space, world, &mut damaging, entity)
                },
                _ => damaging_colliding(entities, &mut damaging, entity)
            }
        }).collect::<Vec<_>>()
    };

    damage_entities.into_iter().for_each(damager(world, space, entities, loot, passer, textures));
}

fn flash_white_single(entities: &ClientEntities, entity: Entity)
{
    if let Some(mut mix_color) = entities.mix_color_target(entity)
    {
        *mix_color = Some(MixColor{color: [1.0; 4], amount: 0.8, keep_transparency: true});

        entities.add_watcher(entity, Watcher{
            kind: WatcherType::Lifetime(HIGHLIGHT_DURATION.into()),
            action: Box::new(|entities, entity|
            {
                if let Some(mut render) = entities.render_mut(entity)
                {
                    render.mix = None;
                }
            }),
            ..Default::default()
        });
    }
}

fn spawn_item(entities: &ClientEntities, textures: &CommonTextures, transform: &Transform, item: &Item)
{
    let item_info = entities.infos().items_info.get(item.id);

    let rotation = random_rotation();

    let item_scale = aabb_bounds(&Transform{
        scale: item_info.scale3() * ENTITY_SCALE,
        rotation,
        ..Default::default()
    });

    let scale = transform.scale.xy() - item_scale.xy();

    let position = transform.position.xy() + scale.map(|limit|
    {
        limit * (fastrand::f32() - 0.5)
    });

    let position = with_z(position, transform.position.z);

    let lazy_transform = item_lazy_transform(item_info, position, rotation);

    let entity = entities.push(true, EntityInfo{
        render: Some(RenderInfo{
            object: Some(RenderObjectKind::TextureId{
                id: item_info.texture.unwrap()
            }.into()),
            z_level: ZLevel::BelowFeet,
            ..Default::default()
        }),
        physical: Some(item_physical(item_info).into()),
        lazy_transform: Some(lazy_transform.into()),
        collider: Some(ColliderInfo{
            layer: ColliderLayer::ThrownDecal,
            ..item_collider()
        }.into()),
        light: Some(item_info.lighting),
        item: Some(item.clone()),
        ..Default::default()
    });

    entities.add_watcher(entity, item_disappear_watcher(textures));
}

fn spawn_items(entities: &ClientEntities, textures: &CommonTextures, loot: &Loot, transform: &Transform, name: &str)
{
    loot.create(LootState::Destroy, name).into_iter().for_each(|item|
    {
        spawn_item(entities, textures, &transform, &item)
    });
}

fn destroy_entity(entities: &ClientEntities, textures: &CommonTextures, loot: &Loot, entity: Entity)
{
    entities.remove_deferred(entity);

    let transform = some_or_return!(entities.transform(entity));

    if let Some(inventory) = entities.inventory(entity)
    {
        inventory.items().for_each(|item|
        {
            spawn_item(entities, textures, &transform, item);
        });
    }

    let name = some_or_return!(entities.named(entity));

    spawn_items(entities, textures, loot, &transform, &name);
}

fn destroy_tile_dependent(
    entities: &ClientEntities,
    textures: &CommonTextures,
    space: &SpatialGrid,
    loot: &Loot,
    tile_pos: TilePos
)
{
    let check_collider = ColliderInfo{
        kind: ColliderType::Rectangle,
        layer: ColliderLayer::Normal,
        ghost: true,
        ..Default::default()
    }.into();

    let transform = Transform{
        position: tile_pos.center_position().into(),
        scale: Vector3::repeat(TILE_SIZE * 0.95),
        ..Default::default()
    };

    let check_collider = CollidingInfoRef{
        entity: None,
        transform,
        collider: &check_collider
    };

    if DebugConfig::is_enabled(DebugTool::CollisionBounds)
    {
        collider_system::debug_collision_bounds(entities, &check_collider);
    }

    let try_collide = |entity|
    {
        let other_collider = some_or_return!(entities.collider(entity));

        let is_door = matches!(other_collider.layer, ColliderLayer::Door);

        if !is_door
        {
            return;
        }

        let other = CollidingInfoRef::new(
            some_or_return!(entities.transform(entity)).clone(),
            &other_collider
        );

        let collided = check_collider.collide_immutable(&other, |_| {});

        if collided
        {
            destroy_entity(entities, textures, loot, entity);
        }
    };

    let _ = space.try_for_each_near(tile_pos, |entity| -> ControlFlow<(), ()>
    {
        try_collide(entity);

        ControlFlow::Continue(())
    });
}

fn knockback_entity(entities: &ClientEntities, entity: Entity, knockback: Vector3<f32>)
{
    let mut physical = some_or_return!(entities.physical_mut(entity));

    physical.add_force(knockback);

    some_or_return!(entities.character_mut(entity)).knockbacked();
}

fn flash_white(entities: &ClientEntities, entity: Entity)
{
    let flash_single = |entity| flash_white_single(entities, entity);

    entities.sibling(entity).map(|x| flash_single(*x));
    entities.for_every_child(entity, flash_single);
}

fn turn_towards_other(
    entities: &ClientEntities,
    entity: Entity,
    other_entity: Entity,
)
{
    let mut enemy = some_or_return!(entities.enemy_mut(entity));

    if !enemy.is_attacking()
    {
        let mut character = some_or_return!(entities.character_mut(entity));
        let anatomy = some_or_return!(entities.anatomy(entity));

        if anatomy.speed() != 0.0
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
    entities: &ClientEntities,
    textures: &CommonTextures,
    loot: &Loot,
    entity: Entity,
    other_entity: Entity,
    damage: Damage
)
{
    turn_towards_other(entities, entity, other_entity);

    if let Some(mut health) = entities.health_mut(entity)
    {
        *health -= damage.data.as_flat();

        if *health <= 0.0
        {
            destroy_entity(entities, textures, loot, entity);
        }
    }

    if let Some(mut anatomy) = entities.anatomy_mut(entity)
    {
        anatomy.damage(damage);
    }
}
