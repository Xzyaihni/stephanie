use std::convert;

use serde::{Serialize, Deserialize};


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Entity(usize);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player
{
    pub name: String
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderInfo
{
    pub texture: String
}

macro_rules! define_entities
{
    ($(($name:ident,
        $mut_func:ident,
        $message_name:ident,
        $component_type:ident,
        $default_type:ident
    )),+) =>
    {
        use yanyaengine::{Object, Transform};

        use crate::common::{ObjectsStore, Message};


        pub enum Component
        {
            $($component_type,)+
        }

        impl Component
        {
            pub fn message_set(
                entities: &Entities,
                entity: Entity,
                components: &[Option<usize>]
            ) -> impl Iterator<Item=Message>
            {
                [$(
                    components[entity.0].map(|component|
                    {
                        Message::$message_name{entity, $name: entities.$name[component].clone()}
                    }),
                )+].into_iter().filter_map(convert::identity)
            }
        }

        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct EntityInfo<$($component_type=$default_type,)+>
        {
            $(pub $name: Option<$component_type>,)+
        }

        impl Default for EntityInfo
        {
            fn default() -> Self
            {
                Self{
                    $($name: None,)+
                }
            }
        }

        pub struct Entities<$($component_type=$default_type,)+>
        {
            components: ObjectsStore<Vec<Option<usize>>>,
            $(pub $name: ObjectsStore<$component_type>,)+
        }

        impl<$($component_type,)+> Entities<$($component_type,)+>
        {
            pub fn new() -> Self
            {
                Self{
                    components: ObjectsStore::new(),
                    $($name: ObjectsStore::new(),)+
                }
            }

            pub fn entities_iter(&self) -> impl Iterator<Item=(Entity, &[Option<usize>])>
            {
                self.components.iter().map(|(index, components)|
                {
                    let components: &[_] = components;

                    (Entity(index), components)
                })
            }

            $(
                pub fn $name(&self, entity: Entity) -> Option<&$component_type>
                {
                    self.components[entity.0][Component::$component_type as usize].map(|id|
                    {
                        &self.$name[id]
                    })
                }

                pub fn $mut_func(&mut self, entity: Entity) -> Option<&mut $component_type>
                {
                    self.components[entity.0][Component::$component_type as usize].map(|id|
                    {
                        &mut self.$name[id]
                    })
                }
            )+

            pub fn push(&mut self, info: EntityInfo<$($component_type,)+>) -> Entity
            {
                let indices = vec![
                    $({
                        info.$name.map(|component|
                        {
                            let id = self.$name.len();

                            self.$name.push(component);
                            
                            id
                        })
                    },)+
                ];

                let id = self.components.len();

                self.components.push(indices);

                Entity(id)
            }

            pub fn handle_message(&mut self, message: Message) -> Option<Message>
            {
                return None;

                todo!()
            }
        }
    }
}

define_entities!{
    (render, render_mut, SetRender, RenderType, RenderInfo),
    (transform, transform_mut, SetTransform, TransformType, Transform),
    (player, player_mut, SetPlayer, PlayerType, Player)
}
