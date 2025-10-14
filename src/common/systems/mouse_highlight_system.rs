use crate::common::{
    some_or_return,
    watcher::*,
    Entity,
    entity::ClientEntities
};


pub fn mouse_selected(entities: &ClientEntities, player: Entity, mouse: Entity) -> Option<Entity>
{
    if let Some(anatomy) = entities.anatomy(player)
    {
        if !anatomy.can_move()
        {
            return None;
        }
    }

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
    let entity = some_or_return!(mouse_selected(entities, player, mouse));

    if !entities.is_lootable(entity)
    {
        return;
    }

    entities.replace_watcher(entity, Watcher{
        kind: WatcherType::Lifetime(0.1.into()),
        action: WatcherAction::OutlineableDisable,
        id: Some(WatcherId::Outline),
        ..Default::default()
    });
}
