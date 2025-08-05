use std::cell::RefCell;

use crate::{
    debug_config::*,
    common::{
        direction_arrow_info,
        physics::*,
        AnyEntities,
        world::World,
        entity::{
            for_each_component,
            ClientEntities
        }
    }
};


pub fn apply(entities: &mut ClientEntities)
{
    for_each_component!(entities, physical, |entity, physical: &RefCell<Physical>|
    {
        if let Some(mut target) = entities.target(entity)
        {
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
