use std::cell::RefCell;

use crate::common::{
    entity::{for_each_component, ClientEntities, ComponentWrapper},
    AnyEntities,
    Enemy,
    EntityPasser,
    Message
};


pub fn update(entities: &mut ClientEntities, passer: &mut impl EntityPasser, dt: f32)
{
    let mut on_state_change = |entity|
    {
        let enemy = entities.enemy(entity).unwrap().clone();
        let target = entities.target_ref(entity).unwrap().clone();

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
            let character = entities.character_mut(entity).unwrap();
            entities.character.iter()
                .map(|(_, x)| x)
                .filter(|x| x.entity != entity)
                .filter(|x|
                {
                    let other_character = x.get();
                    character.aggressive(&other_character)
                })
                .filter(|x|
                {
                    let other_entity = x.entity;

                    let anatomy = entities.anatomy(entity).unwrap();
                    let other_visibility = x.get().visibility();

                    let transform = entities.transform(entity).unwrap();
                    let other_transform = entities.transform(other_entity).unwrap();

                    anatomy.sees(&transform, other_visibility, &other_transform.position)
                })
                .for_each(|&ComponentWrapper{
                    entity: other_entity,
                    ..
                }|
                {
                    enemy.borrow_mut().set_attacking(other_entity);
                    on_state_change(entity);
                });
        }

        let state_changed = enemy.borrow_mut().update(
            entities,
            entity,
            dt
        );

        if state_changed
        {
            on_state_change(entity);
        }
    });
}
