use std::cell::RefCell;

use crate::common::{
    entity::{for_each_component, ClientEntities},
    Anatomy
};


pub fn update(
    entities: &mut ClientEntities,
    dt: f32
)
{
    for_each_component!(entities, anatomy, |entity, anatomy: &RefCell<Anatomy>|
    {
        if anatomy.borrow_mut().update(entities.player_exists(entity), dt)
        {
            entities.set_changed().anatomy(entity);
        }
    });
}
