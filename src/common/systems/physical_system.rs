use std::cell::RefCell;

use crate::{
    debug_config::*,
    common::{
        some_or_return,
        direction_arrow_info,
        physics::*,
        AnyEntities,
        Entity,
        world::{TILE_SIZE, World},
        entity::{
            for_each_component,
            ClientEntities
        }
    }
};


pub fn apply(entities: &mut ClientEntities, world: &World)
{
    for_each_component!(entities, physical, |entity, physical: &RefCell<Physical>|
    {
        if let Some(mut target) = entities.target(entity)
        {
            if !world.inside_chunk(target.position.into())
            {
                return;
            }

            physical.borrow_mut().apply(&mut target);
        }
    });
}

pub fn update(entities: &mut ClientEntities, world: &World, dt: f32)
{
    for_each_component!(entities, physical, |entity, physical: &RefCell<Physical>|
    {
        if let Some(mut target) = entities.target(entity)
        {
            if !world.inside_chunk(target.position.into())
            {
                return;
            }

            physical.borrow_mut().update(
                &mut target,
                |physical, transform|
                {
                    entities.collider(entity)
                        .map(|collider| collider.inverse_inertia(physical, transform.clone()))
                        .unwrap_or_default()
                },
                dt
            );

            if DebugConfig::is_enabled(DebugTool::Velocity)
            {
                drop(target);

                let velocity = *physical.borrow().velocity();
                let magnitude = velocity.magnitude();

                if let Some(info) = direction_arrow_info(
                    entities.transform(entity).unwrap().position,
                    velocity,
                    magnitude,
                    [0.0, 0.0, 1.0]
                )
                {
                    entities.push(true, info);
                }
            }
        }
    });
}

pub fn sleeping_update(
    entities: &ClientEntities,
    player: Entity
)
{
    let position = some_or_return!(entities.transform(player)).position;

    let z = position.z;
    let position = position.xy();

    for_each_component!(entities, physical, |entity, physical: &RefCell<Physical>|
    {
        let mut physical = physical.borrow_mut();

        let other_position = some_or_return!(entities.transform(entity)).position;

        let too_far = ((other_position.z - z).abs() > TILE_SIZE * 2.5)
            || (other_position.xy().metric_distance(&position) > TILE_SIZE * 30.0);

        physical.set_sleeping(too_far);
    });
}
