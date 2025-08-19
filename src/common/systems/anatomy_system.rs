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
    for_each_component!(entities, anatomy, |_entity, anatomy: &RefCell<Anatomy>|
    {
        anatomy.borrow_mut().update(dt);
    });
}
