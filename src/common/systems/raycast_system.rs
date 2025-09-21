use std::{
    cmp::Ordering,
    cell::RefCell
};

use nalgebra::{Unit, Vector3};

use yanyaengine::Transform;

use crate::{
    debug_config::*,
    common::{
        collider::*,
        raycast::*,
        watcher::*,
        render_info::*,
        World,
        EntityInfo,
        AnyEntities,
        entity::{
            iterate_components_with,
            ClientEntities
        }
    }
};


pub fn raycast(
    entities: &ClientEntities,
    world: Option<&World>,
    info: RaycastInfo,
    start: Vector3<f32>,
    end: Vector3<f32>
) -> RaycastHits
{
    let direction = end - start;

    let max_distance = direction.magnitude();
    let direction = Unit::new_unchecked(direction / max_distance);

    let mut hits: Vec<_> = iterate_components_with!(
        entities,
        collider,
        filter_map,
        |entity, collider: &RefCell<Collider>|
        {
            let collider = collider.borrow();
            let collides = collider.layer.collides(&info.layer);

            (collides && !collider.ghost).then(|| (entity, collider.kind))
        })
        .filter_map(|(entity, kind)|
        {
            let transform = entities.transform(entity);

            transform.and_then(|transform|
            {
                if let Some(ignore_entity) = info.ignore_entity
                {
                    (entity != ignore_entity).then_some((entity, kind, transform))
                } else
                {
                    Some((entity, kind, transform))
                }
            })
        })
        .filter_map(|(entity, kind, transform)|
        {
            raycast_this(start, direction, kind, &transform).and_then(|hit|
            {
                let backwards = hit.is_behind();
                let past_end = (hit.distance > max_distance) && !info.ignore_end;

                if backwards || past_end
                {
                    None
                } else
                {
                    let id = RaycastHitId::Entity(entity);
                    Some(RaycastHit{id, result: hit})
                }
            })
        })
        .collect();

    if let Some(world) = world
    {
        let mut pierce_left = info.pierce;

        let world_hits = raycast_world(world, start, direction, |tile, hit|
        {
            if let Some(left) = pierce_left.as_mut()
            {
                if *left <= 0.0
                {
                    return true;
                }

                *left -= hit.result.pierce * match info.pierce_scale
                {
                    RaycastPierce::None => 1.0,
                    RaycastPierce::Ignore => 0.0,
                    RaycastPierce::Density{..} => tile.health
                };
            }

            if (hit.result.distance > max_distance) && !info.ignore_end
            {
                return true;
            }

            false
        });

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
