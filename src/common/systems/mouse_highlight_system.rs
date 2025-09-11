use std::cell::RefCell;

use crate::common::{
    some_or_return,
    watcher::*,
    Entity,
    Outlineable,
    entity::{for_each_component, ClientEntities}
};


pub fn mouse_selected(entities: &ClientEntities, player: Entity, mouse: Entity) -> Option<Entity>
{
    let mouse_collider = some_or_return!(entities.collider(mouse));

    let mouse_collided = mouse_collider.collided().iter()
        .filter_map(|x| entities.render(*x).map(|render| (x, render)))
        .max_by_key(|(_x, render)| render.z_level())
        .map(|(x, _)| x)
        .copied()?;

    (entities.within_interactable_distance(player, mouse_collided)).then_some(mouse_collided)
}

pub fn update(
    entities: &ClientEntities,
    player: Entity,
    mouse: Entity
)
{
    let mouse_collided = some_or_return!(mouse_selected(entities, player, mouse));

    for_each_component!(entities, outlineable, |entity, outlineable: &RefCell<Outlineable>|
    {
        let overlapping = mouse_collided == entity;

        if !overlapping || !entities.is_lootable(entity)
        {
            return;
        }

        if let Some(mut watchers) = entities.watchers_mut(entity)
        {
            outlineable.borrow_mut().enable();

            let kind = WatcherType::Lifetime(0.1.into());
            if let Some(found) = watchers.find(|watcher|
            {
                // comparison considered harmful
                if let WatcherAction::OutlineableDisable = watcher.action
                {
                    true
                } else
                {
                    false
                }
            })
            {
                found.kind = kind;
            } else
            {
                watchers.push(Watcher{
                    kind,
                    action: WatcherAction::OutlineableDisable,
                    ..Default::default()
                });
            }
        }
    });
}
