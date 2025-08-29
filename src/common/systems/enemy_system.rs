use std::cell::RefCell;

use crate::common::{
    some_or_return,
    enemy,
    entity::{for_each_component, ClientEntities},
    World,
    Enemy
};


pub fn update(
    entities: &mut ClientEntities,
    world: &World,
    dt: f32
)
{
    for_each_component!(entities, enemy, |entity, enemy: &RefCell<Enemy>|
    {
        if entities.physical(entity).map(|x| x.sleeping()).unwrap_or(true)
        {
            return;
        }

        let mut enemy = enemy.borrow_mut();
        if enemy.check_hostiles()
        {
            let character = some_or_return!(entities.character(entity));
            entities.character.iter()
                .map(|(_, x)| x)
                .filter(|x| x.entity != entity)
                .filter(|x|
                {
                    let other_character = x.get();
                    character.aggressive(&other_character)
                })
                .filter_map(|x|
                {
                    let other_entity = x.entity;

                    enemy::sees(entities, world, entity, other_entity).map(|(_, visibility)|
                    {
                        (other_entity, visibility)
                    })
                })
                .for_each(|(other_entity, visibility)|
                {
                    entities.set_changed().enemy(entity);

                    if enemy.seen_timer() >= 1.0
                    {
                        enemy.set_attacking(other_entity);
                    } else
                    {
                        enemy.increase_seen(visibility * 4.0 * dt);
                    }
                });
        }

        let _state_changed = enemy.update(
            entities,
            world,
            entity,
            dt
        );
    });
}
