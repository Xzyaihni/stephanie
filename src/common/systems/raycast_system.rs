use std::{
    ops::{RangeInclusive, ControlFlow},
    cmp::Ordering,
    cell::RefCell
};

use nalgebra::{Unit, Vector3};

use yanyaengine::Transform;

use crate::{
    debug_config::*,
    common::{
        some_or_value,
        collider::*,
        raycast::*,
        watcher::*,
        render_info::*,
        Entity,
        World,
        EntityInfo,
        AnyEntities,
        SpatialGrid,
        entity::{
            iterate_components_with,
            ClientEntities
        }
    }
};


pub fn raycast(
    entities: &ClientEntities,
    space: &SpatialGrid,
    world: Option<&World>,
    info: RaycastInfo,
    start: Vector3<f32>,
    end: Vector3<f32>
) -> RaycastHits
{
    let direction = end - start;
    let max_distance = direction.magnitude();

    let direction = Unit::new_unchecked(direction / max_distance);

    let before_raycast = before_raycast_default(info.layer, info.ignore_entity);

    let stage = |x|
    {
        (raycast_entities_raw_stage(RaycastEntitiesRawInfo{
            entities,
            start,
            direction,
            after_raycast: after_raycast_default(max_distance, info.ignore_end),
            raycast_fn: raycast_this
        }))(x).map(|(entity, result)| RaycastHit{id: RaycastHitId::Entity(entity), result})
    };

    let raycast_all = |entities, before_raycast, stage| -> Vec<RaycastHit>
    {
        raycast_entities_all_raw_setup(entities, before_raycast)
            .filter_map(stage)
            .collect()
    };

    let mut hits: Vec<_> = if !info.ignore_end
    {
        if let Some(zs) = raycast_space_zs(space, info.scale, start, end)
        {
            let mut hits = Vec::new();
            let _ = raycast_entities_space_raw_setup(entities, space, zs, before_raycast, |x| -> ControlFlow<(), ()>
            {
                if let Some(hit) = stage(x)
                {
                    hits.push(hit);
                }

                ControlFlow::Continue(())
            });

            hits
        } else
        {
            raycast_all(entities, before_raycast, stage)
        }
    } else
    {
        raycast_all(entities, before_raycast, stage)
    };

    if let Some(world) = world
    {
        let mut pierce_left = info.pierce;

        let world_hits = raycast_world(world, start, direction, |tile, _, result|
        {
            if let Some(left) = pierce_left.as_mut()
            {
                if *left <= 0.0
                {
                    return true;
                }

                *left -= result.pierce * match info.pierce_scale
                {
                    RaycastPierce::None => 1.0,
                    RaycastPierce::Ignore => 0.0,
                    RaycastPierce::Density{..} => tile.health
                };
            }

            if (result.distance > max_distance) && !info.ignore_end
            {
                return true;
            }

            false
        }).map(|(_, pos, result)| RaycastHit{id: RaycastHitId::Tile(pos), result});

        hits.extend(world_hits);
    }

    hits.sort_unstable_by(|a, b|
    {
        a.result.distance.partial_cmp(&b.result.distance).unwrap_or(Ordering::Equal)
    });

    let hits = if let Some(mut pierce) = info.pierce
    {
        hits.into_iter().take_while(|x|
        {
            if pierce > 0.0
            {
                pierce -= x.result.pierce * match info.pierce_scale
                {
                    RaycastPierce::None => 1.0,
                    RaycastPierce::Ignore => 0.0,
                    RaycastPierce::Density{ignore_anatomy} =>
                    {
                        match x.id
                        {
                            RaycastHitId::Tile(pos) =>
                            {
                                let world = world.expect("tile hits must only be possible with world");
                                world.tile(pos).map(|tile| world.tile_info(*tile).health).unwrap_or(1.0)
                            },
                            RaycastHitId::Entity(entity) =>
                            {
                                if ignore_anatomy && entities.anatomy_exists(entity)
                                {
                                    0.0
                                } else if let Some(physical) = entities.physical(entity)
                                {
                                    entities.transform(entity).map(|x|
                                    {
                                        x.scale.product()
                                    }).unwrap_or(1.0) * physical.inverse_mass.recip()
                                } else
                                {
                                    1.0
                                }
                            }
                        }
                    }
                };

                true
            } else
            {
                false
            }
        }).collect()
    } else
    {
        let first = hits.into_iter().next();

        first.map(|x| vec![x]).unwrap_or_default()
    };

    if DebugConfig::is_enabled(DebugTool::DisplayRaycast)
    {
        hits.iter().for_each(|hit|
        {
            let color = match hit.id
            {
                RaycastHitId::Entity(_) => [1.0, 0.0, 0.0, 1.0],
                RaycastHitId::Tile(_) => [0.0, 0.0, 1.0, 1.0]
            };

            let position = start + *direction * hit.result.distance;

            entities.push(true, EntityInfo{
                transform: Some(Transform{
                    position,
                    scale: Vector3::repeat(0.01),
                    ..Default::default()
                }),
                render: Some(RenderInfo{
                    object: Some(RenderObjectKind::Texture{
                        name: "circle.png".to_owned()
                    }.into()),
                    above_world: true,
                    mix: Some(MixColor{keep_transparency: true, ..MixColor::color(color)}),
                    ..Default::default()
                }),
                watchers: Some(Watchers::simple_disappearing(10.0)),
                ..Default::default()
            });

            let arrow_scale = hit.result.pierce;

            if let Some(arrow) = crate::common::direction_arrow_info(
                position,
                *direction,
                arrow_scale,
                [0.0, 0.0, 0.0]
            )
            {
                entities.push(true, EntityInfo{
                    transform: arrow.transform,
                    render: Some(RenderInfo{
                        object: Some(RenderObjectKind::Texture{
                            name: "arrow.png".to_owned()
                        }.into()),
                        above_world: true,
                        mix: Some(MixColor{keep_transparency: true, ..MixColor::color(color)}),
                        ..Default::default()
                    }),
                    watchers: Some(Watchers::simple_disappearing(10.0)),
                    ..Default::default()
                });
            }
        });
    }

    RaycastHits{start, direction, hits}
}

pub fn raycast_entities_any_raw<'a, BeforeRaycast, AfterRaycast, Raycast>(
    space: &'a SpatialGrid,
    scale: f32,
    end: Vector3<f32>,
    before_raycast: BeforeRaycast,
    info: RaycastEntitiesRawInfo<'a, AfterRaycast, Raycast>
) -> bool
where
    BeforeRaycast: Fn(&Collider, Entity) -> bool,
    AfterRaycast: Fn(Entity, &RaycastResult) -> bool,
    Raycast: Fn(Vector3<f32>, Unit<Vector3<f32>>, ColliderType, &Transform) -> Option<RaycastResult>
{
    let entities = info.entities;
    let start = info.start;

    let mut stage = raycast_entities_raw_stage(info);

    if let Some(zs) = raycast_space_zs(space, scale, start, end)
    {
        raycast_entities_space_raw_setup(
            entities,
            space,
            zs,
            before_raycast,
            move |x|
            {
                if stage(x).is_some() { ControlFlow::Break(()) } else { ControlFlow::Continue(()) }
            }
        ).is_break()
    } else
    {
        raycast_entities_all_raw_setup(
            entities,
            before_raycast
        ).any(move |x| stage(x).is_some())
    }
}

pub fn before_raycast_default(layer: ColliderLayer, ignore_entity: Option<Entity>) -> impl Fn(&Collider, Entity) -> bool
{
    move |collider, entity|
    {
        if collider.ghost
        {
            return false;
        }

        let collides = collider.layer.collides(&layer);

        collides && ignore_entity.as_ref().map(|ignore_entity|
        {
            entity != *ignore_entity
        }).unwrap_or(true)
    }
}

pub fn after_raycast_default(max_distance: f32, ignore_end: bool) -> impl Fn(Entity, &RaycastResult) -> bool
{
    move |_entity, hit|
    {
        let backwards = hit.is_behind();
        let past_end = (hit.distance > max_distance) && !ignore_end;

        !(backwards || past_end)
    }
}

pub fn raycast_entities_raw_stage<'a, AfterRaycast, Raycast>(
    info: RaycastEntitiesRawInfo<'a, AfterRaycast, Raycast>
) -> impl FnMut((Entity, ColliderType)) -> Option<(Entity, RaycastResult)> + use<'a, AfterRaycast, Raycast>
where
    AfterRaycast: Fn(Entity, &RaycastResult) -> bool,
    Raycast: Fn(Vector3<f32>, Unit<Vector3<f32>>, ColliderType, &Transform) -> Option<RaycastResult>
{
    move |(entity, kind)|
    {
        let transform = info.entities.transform(entity)?;

        let hit = (info.raycast_fn)(info.start, info.direction, kind, &transform)?;

        (info.after_raycast)(entity, &hit).then_some((entity, hit))
    }
}

pub struct RaycastEntitiesRawInfo<'a, AfterRaycast, Raycast>
{
    pub entities: &'a ClientEntities,
    pub start: Vector3<f32>,
    pub direction: Unit<Vector3<f32>>,
    pub after_raycast: AfterRaycast,
    pub raycast_fn: Raycast
}

pub fn raycast_space_zs(
    space: &SpatialGrid,
    scale: f32,
    start: Vector3<f32>,
    end: Vector3<f32>
) -> Option<RangeInclusive<usize>>
{
    if space.inside_simulated(start, scale) && space.inside_simulated(end, scale)
    {
        let a = space.z_of(start.z)?;
        let b = space.z_of(end.z)?;

        let zs = if a > b
        {
            b..=a
        } else
        {
            a..=b
        };

        Some(zs)
    } else
    {
        None
    }
}

pub fn raycast_entities_space_raw_setup<'a, BeforeRaycast, F, Break>(
    entities: &'a ClientEntities,
    space: &'a SpatialGrid,
    zs: RangeInclusive<usize>,
    before_raycast: BeforeRaycast,
    mut f: F
) -> ControlFlow<Break, ()>
where
    BeforeRaycast: Fn(&Collider, Entity) -> bool,
    F: FnMut((Entity, ColliderType)) -> ControlFlow<Break, ()>
{
    space.z_nodes[zs].iter().try_for_each(|node|
    {
        node.try_fold((), &mut |_, entity|
        {
            let collider = some_or_value!(entities.collider(entity), ControlFlow::Continue(()));
            if before_raycast(&collider, entity)
            {
                f((entity, collider.kind))
            } else
            {
                ControlFlow::Continue(())
            }
        })
    })
}

pub fn raycast_entities_all_raw_setup<'a, BeforeRaycast>(
    entities: &'a ClientEntities,
    before_raycast: BeforeRaycast
) -> impl Iterator<Item=(Entity, ColliderType)> + use<'a, BeforeRaycast>
where
    BeforeRaycast: Fn(&Collider, Entity) -> bool
{
    iterate_components_with!(
        entities,
        collider,
        filter_map,
        move_outer,
        |entity, collider: &RefCell<Collider>|
        {
            let collider = collider.borrow();
            before_raycast(&collider, entity).then(|| (entity, collider.kind))
        }
    )
}
