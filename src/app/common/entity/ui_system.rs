use std::cell::RefCell;

use nalgebra::Vector2;

use crate::{
    client::ui_element::*,
    common::{
        AnyEntities,
        entity::{iterate_components_with, ClientEntities}
    }
};


pub fn update(
    entities: &mut ClientEntities,
    camera_position: Vector2<f32>,
    event: UiEvent
) -> bool
{
    let mut captured = false;
    // borrow checker more like goofy ahh
    // rev to early exit if child is captured
    iterate_components_with!(entities, ui_element, filter_map, |entity, ui_element|
    {
        entities.is_visible(entity).then_some((entity, ui_element))
    }).rev().for_each(|(entity, ui_element): (_, &RefCell<UiElement>)|
    {
        captured = ui_element.borrow_mut().update(
            &*entities,
            entity,
            camera_position,
            &event,
            captured
        ) || captured;
    });

    captured
}
