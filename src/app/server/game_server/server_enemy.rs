use std::{
    borrow::Borrow,
    ops::{Deref, DerefMut}
};

use crate::{
    basic_entity_forward,
    server::ConnectionsHandler,
    common::{
        Enemy,
        EntityPasser,
        message::Message
    }
};


#[derive(Debug)]
pub struct ServerEnemy
{
    enemy: Enemy,
    current_state_left: f32
}

impl ServerEnemy
{
    pub fn new(enemy: Enemy) -> Self
    {
        let current_state_left = Self::state_duration(&enemy);

        Self{enemy, current_state_left}
    }

    fn state_duration(enemy: &Enemy) -> f32
    {
        enemy.behavior().duration_of(enemy.behavior_state())
    }

    pub fn update(
        &mut self,
        messager: &mut ConnectionsHandler,
        id: usize,
        dt: f32
    )
    {
        self.current_state_left -= dt;

        if self.current_state_left <= 0.0
        {
            self.enemy.next_state();
            self.current_state_left = Self::state_duration(&self.enemy);

            let message = Message::EnemyStateChanged{
                id,
                state: self.enemy.behavior_state().clone()
            };

            messager.send_message(message);
        }
    }
}

basic_entity_forward!{ServerEnemy, enemy}

impl Deref for ServerEnemy
{
    type Target = Enemy;

    fn deref(&self) -> &Self::Target
    {
        &self.enemy
    }
}

impl DerefMut for ServerEnemy
{
    fn deref_mut(&mut self) -> &mut Self::Target
    {
        &mut self.enemy
    }
}

impl Borrow<Enemy> for ServerEnemy
{
    fn borrow(&self) -> &Enemy
    {
        &self.enemy
    }
}
