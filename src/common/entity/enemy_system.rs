use std::cell::RefCell;

use parking_lot::RwLock;

use crate::common::{
    enemy,
    entity::{for_each_component, ClientEntities},
    World,
    AnyEntities,
    Enemy,
    EntityPasser,
    Message
};


pub fn update<Passer: EntityPasser>(
    entities: &mut ClientEntities,
    world: &World,
    passer: &RwLock<Passer>,
    dt: f32
)
{
    let on_state_change = |entity|
    {
        let enemy = entities.enemy(entity).unwrap().clone();
        let target = entities.target_ref(entity).unwrap().clone();

        let mut passer = passer.write();
        passer.send_message(Message::SetEnemy{
            entity,
            component: enemy.into()
        });

        passer.send_message(Message::SetTarget{
            entity,
            target: Box::new(target)
        });
    };

    for_each_component!(entities, enemy, |entity, enemy: &RefCell<Enemy>|
    {
        if enemy.borrow().check_hostiles()
        {
            let character = entities.character(entity).unwrap();
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

                    enemy::sees(entities, world, entity, other_entity).map(|visibility|
                    {
                        (other_entity, visibility)
                    })
                })
                .for_each(|(other_entity, visibility)|
                {
                    entities.set_changed().enemy(entity);

                    let mut enemy = enemy.borrow_mut();
                    if enemy.seen_timer() >= 1.0
                    {
                        enemy.set_attacking(other_entity);
                        drop(enemy);

                        on_state_change(entity);
                    } else
                    {
                        enemy.increase_seen(visibility * 2.0 * dt);
                    }
                });
        }

        let state_changed = enemy.borrow_mut().update(
            entities,
            world,
            entity,
            dt
        );

        if state_changed
        {
            on_state_change(entity);
        }
    });
}
