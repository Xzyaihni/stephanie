use std::{
    borrow::Borrow,
    ops::{Deref, DerefMut}
};

use crate::{
    basic_entity_forward,
    server::ConnectionsHandler,
    common::{
        Enemy,
        EntityType,
        EntityAnyWrappable,
        EntityPasser,
        message::Message
    }
};


#[derive(Debug)]
pub struct ServerEnemy
{
    enemy: Enemy
}

impl ServerEnemy
{
    pub fn new(enemy: Enemy) -> Self
    {
        Self{enemy}
    }

    pub fn update(
        &mut self,
        messager: &mut ConnectionsHandler,
        id: usize,
        dt: f32
    )
    {
        let needs_sync = self.enemy.update(dt);

        if needs_sync
        {
            let message = Message::EntitySet{
                id: EntityType::Enemy(id),
                entity: self.clone().wrap_any()
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
