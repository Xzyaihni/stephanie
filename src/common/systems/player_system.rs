use std::cell::RefCell;

use nalgebra::Vector3;

use crate::common::{
    Entity,
    Player,
    entity::{for_each_component, iterate_components_with, ClientEntities}
};


pub fn players_near(
    entities: &ClientEntities,
    position: Vector3<f32>,
    distance: f32
) -> impl Iterator<Item=(Entity, &RefCell<Player>)>
{
    fn m(entity: Entity, player: &RefCell<Player>) -> (Entity, &RefCell<Player>)
    {
        (entity, player)
    }

    iterate_components_with!(
        entities,
        player,
        filter_map,
        move_outer,
        |entity, player|
        {
            let player_position = entities.transform(entity)?.position;

            (player_position.metric_distance(&position) < distance).then(|| m(entity, player))
        }
    )
}

pub fn update(entities: &mut ClientEntities, dt: f32)
{
    for_each_component!(entities, player, |_entity, player: &RefCell<Player>|
    {
        player.borrow_mut().screenshake.update(dt);
    });
}
