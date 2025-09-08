use std::cell::RefCell;

use crate::common::{
    physics::*,
    AnyEntities,
    world::World,
    entity::{
        for_each_component,
        ClientEntities
    }
};


pub fn apply(entities: &mut ClientEntities, world: &World)
{
    for_each_component!(entities, physical, |entity, physical: &RefCell<Physical>|
    {
        if entities.collider(entity).map(|x| x.sleeping).unwrap_or(false)
        {
            return;
        }

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
