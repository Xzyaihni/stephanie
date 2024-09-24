use std::{
    cmp::Ordering,
    cell::RefCell
};

use nalgebra::{Unit, Vector3};

use crate::common::{
    collider::*,
    raycast::*,
    entity::{
        iterate_components_with,
        ClientEntities
    }
};


pub fn raycast(
    entities: &ClientEntities,
    info: RaycastInfo,
    start: &Vector3<f32>,
    end: &Vector3<f32>
) -> RaycastHits
{
    let direction = end - start;

    let max_distance = direction.magnitude();
    let direction = Unit::new_normalize(direction);

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
            raycast_this(start, &direction, kind, &transform).and_then(|hit|
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
                pierce -= x.result.pierce;

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

    RaycastHits{start: *start, direction, hits}
}
