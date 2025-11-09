use crate::common::{
    some_or_return,
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

pub fn is_lootable(entities: &ClientEntities, entity: Entity) -> bool
{
    if entities.player_exists(entity)
    {
        return false;
    }

    if entities.item_exists(entity)
    {
        return !entities.damaging_exists(entity);
    }

    if let Some(inventory) = entities.inventory(entity)
    {
        if !inventory.is_empty()
        {
            if let Some(anatomy) = entities.anatomy(entity)
            {
                if anatomy.is_dead()
                {
                    return true;
                }
            } else
            {
                return true;
            }
        }
    }

    false
}

pub fn update(
    entities: &ClientEntities,
    player: Entity,
    mouse: Entity
) -> Option<Entity>
{
    let entity = some_or_return!(mouse_selected(entities, player, mouse));

    if !is_lootable(entities, entity)
    {
        return None;
    }

    if let Some(mut render) = entities.render_mut_no_change(entity)
    {
        render.outlined = true;
    }

    Some(entity)
}
