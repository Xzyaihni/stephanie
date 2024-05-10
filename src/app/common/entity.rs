use serde::{Serialize, Deserialize};

use yanyaengine::{DefaultModel, Object, ObjectInfo, game_object::*};


pub trait ServerToClient<T>
{
    fn server_to_client(
        self,
        transform: Option<Transform>,
        create_info: &mut ObjectCreateInfo
    ) -> T;
}

impl<T> ServerToClient<T> for T
{
    fn server_to_client(
        self,
        _transform: Option<Transform>,
        _create_info: &mut ObjectCreateInfo
    ) -> T
    {
        self
    }
}

impl ServerToClient<Object> for RenderInfo
{
    fn server_to_client(
        self,
        transform: Option<Transform>,
        create_info: &mut ObjectCreateInfo
    ) -> Object
    {
        let assets = create_info.partial.assets.lock();

        let info = ObjectInfo{
            model: assets.model(assets.default_model(DefaultModel::Square)).clone(),
            texture: assets.texture_by_name(&self.texture).clone(),
            transform: transform.expect("renderable must have a transform")
        };

        create_info.partial.object_factory.create(info)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Entity(usize);

impl Entity
{
    pub fn get_raw(&self) -> usize
    {
        self.0
    }
}

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
        use yanyaengine::Transform;

        use crate::common::{ObjectsStore, Message};


        pub enum Component
        {
            $($component_type,)+
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

        pub type ClientEntities = Entities<Object>;
        pub type ServerEntities = Entities;

        pub struct Entities<$($component_type=$default_type,)+>
        {
            pub components: ObjectsStore<Vec<Option<usize>>>,
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

            pub fn exists(&self, entity: Entity) -> bool
            {
                self.components.contains(entity.0)
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
                let indices = self.info_components(info);

                let id = self.components.len();

                self.components.push(indices);

                Entity(id)
            }

            pub fn remove(&mut self, entity: Entity)
            {
                if !self.exists(entity)
                {
                    return;
                }

                let components = &self.components[entity.0];

                $(if let Some(id) = components[Component::$component_type as usize]
                {
                    self.$name.remove(id);
                })+

                self.components.remove(entity.0);
            }

            fn info_components(
                &mut self,
                info: EntityInfo<$($component_type,)+>
            ) -> Vec<Option<usize>>
            {
                vec![
                    $({
                        info.$name.map(|component|
                        {
                            self.$name.push(component)
                        })
                    },)+
                ]
            }

            fn empty_components() -> Vec<Option<usize>>
            {
                vec![$(
                    {
                        let _ = Component::$component_type;
                        None
                    }
                ,)+]
            }

            fn handle_message_common(&mut self, message: Message) -> Option<Message>
            {
                match message
                {
                    Message::EntityDestroy{entity} =>
                    {
                        self.remove(entity);

                        None
                    },
                    x => Some(x)
                }
            }
        }

        impl ClientEntities
        {
            pub fn handle_message(
                &mut self,
                create_info: &mut ObjectCreateInfo,
                message: Message
            ) -> Option<Message>
            {
                let message = self.handle_message_common(message)?;

                match message
                {
                    Message::EntitySet{entity, info} =>
                    {
                        if self.exists(entity)
                        {
                            self.set_existing_entity(create_info, entity, info);
                        } else
                        {
                            let transform = info.transform.clone();

                            let components = vec![
                                $({
                                    info.$name.map(|component|
                                    {
                                        let component = component.server_to_client(
                                            transform.clone(),
                                            create_info
                                        );

                                        self.$name.push(component)
                                    })
                                },)+
                            ];

                            self.components.insert(entity.0, components);
                        }

                        None
                    },
                    $(Message::$message_name{entity, $name} =>
                    {
                        // i could pass a some here but its just wasted effort
                        let component = $name.server_to_client(None, create_info);

                        if !self.exists(entity)
                        {
                            self.components.insert(entity.0, Self::empty_components());
                        }

                        let slot = &mut self.components
                            [entity.0]
                            [Component::$component_type as usize];

                        if let Some(id) = slot
                        {
                            self.$name.insert(*id, component);
                        } else
                        {
                            let id = self.$name.push(component);
                            
                            *slot = Some(id);
                        }

                        None
                    },)+
                    x => Some(x)
                }
            }

            fn set_existing_entity(
                &mut self,
                create_info: &mut ObjectCreateInfo,
                entity: Entity,
                info: EntityInfo
            )
            {
                let components = &mut self.components[entity.0];

                let transform = info.transform.clone();

                $({
                    let component = &mut components[Component::$component_type as usize];

                    let new_component = info.$name.map(|c|
                    {
                        c.server_to_client(transform.clone(), create_info)
                    });

                    if let Some(new_component) = new_component
                    {
                        if let Some(id) = component
                        {
                            self.$name[*id] = new_component;
                        } else
                        {
                            let id = self.$name.push(new_component);

                            *component = Some(id);
                        }
                    } else
                    {
                        *component = None;
                    }
                })+
            }
        }

        impl ServerEntities
        {
            pub fn info(&self, entity: Entity) -> EntityInfo
            {
                let components = &self.components[entity.0];

                EntityInfo{$(
                    $name: components[Component::$component_type as usize].map(|id|
                    {
                        self.$name[id].clone()
                    }),
                )+}
            }

            pub fn push_message(&mut self, info: EntityInfo) -> Message
            {
                let entity = self.push(info.clone());

                Message::EntitySet{entity, info}
            }

            pub fn remove_message(&mut self, entity: Entity) -> Message
            {
                self.remove(entity);

                Message::EntityDestroy{entity}
            }

            pub fn handle_message(&mut self, message: Message) -> Option<Message>
            {
                self.handle_message_common(message)
            }
        }
    }
}

define_entities!{
    (render, render_mut, SetRender, RenderType, RenderInfo),
    (transform, transform_mut, SetTransform, TransformType, Transform),
    (player, player_mut, SetPlayer, PlayerType, Player)
}
