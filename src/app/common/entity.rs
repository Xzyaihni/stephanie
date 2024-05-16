use serde::{Serialize, Deserialize};

use yanyaengine::game_object::*;

use crate::{
    server::ConnectionsHandler,
    client::ui_element::UiElement,
    common::{
        EntityPasser,
        Anatomy,
        Enemy,
        EnemiesInfo,
        Physical,
        RenderInfo,
        ClientRenderInfo,
        LazyTransform,
        LazyTransformServer,
        LazyTargettable
    }
};


macro_rules! get_component
{
    ($this:expr, $components:expr, $access_type:ident, $component:ident) =>
    {
        $components[Component::$component as usize]
            .map(|id|
            {
                $this.$component.$access_type(id).unwrap_or_else(||
                {
                    panic!("pointer to {} is out of bounds", stringify!($component))
                }).$access_type()
            })
    }
}

macro_rules! get_required_component
{
    ($this:expr, $components:expr, $access_type:ident, $component:ident) =>
    {
        get_component!($this, $components, $access_type, $component).unwrap_or_else(||
        {
            panic!("has no {} component", stringify!($component))
        })
    }
}

macro_rules! get_entity
{
    ($this:expr, $entity:expr, $access_type:ident, $component:ident) =>
    {
        get_component!($this, $this.components[$entity.0], $access_type, $component)
    }
}

#[allow(unused_macros)]
macro_rules! get_required_entity
{
    ($this:expr, $entity:expr, $access_type:ident, $component:ident) =>
    {
        get_required_component!($this, $this.components[$entity.0], $access_type, $component)
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Entity(usize);

impl Entity
{
    pub fn get_raw(&self) -> usize
    {
        self.0
    }
}

pub trait OnSet<EntitiesType>
where
    Self: Sized
{
    fn on_set(previous: Option<Self>, entities: &mut EntitiesType, entity: Entity);
}

macro_rules! no_on_set
{
    ($($name:ident),*) =>
    {
        $(impl<E> OnSet<E> for $name
        {
            fn on_set(_previous: Option<Self>, _entities: &mut E, _entity: Entity) {}
        })*
    }
}

macro_rules! no_on_set_for
{
    ($container:ident, $name:ident) =>
    {
        impl OnSet<$container> for $name
        {
            fn on_set(_previous: Option<Self>, _entities: &mut $container, _entity: Entity) {}
        }
    }
}

no_on_set!{
    ClientRenderInfo,
    RenderInfo,
    LazyTransform,
    LazyTransformServer,
    Parent,
    Transform,
    Player,
    Physical,
    UiElement,
    UiElementServer
}

no_on_set_for!{ServerEntities, Enemy}

impl OnSet<ClientEntities> for Enemy
{
    fn on_set(previous: Option<Self>, entities: &mut ClientEntities, entity: Entity)
    {
        if let Some(previous) = previous
        {
            let enemy = entities.enemy_mut(entity).unwrap();

            enemy.with_previous(previous);
        }
    }
}

no_on_set_for!{ServerEntities, Anatomy}

impl OnSet<ClientEntities> for Anatomy
{
    fn on_set(_previous: Option<Self>, entities: &mut ClientEntities, entity: Entity)
    {
        entities.anatomy_changed(entity);
    }
}

// parent must always come before child !! (index wise)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parent
{
    parent: Entity
}

impl Parent
{
    pub fn new(parent: Entity) -> Self
    {
        Self{parent}
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player
{
    pub name: String
}

type UiElementServer = ();

#[derive(Debug, Clone)]
pub struct ComponentWrapper<T>
{
    entity: Entity,
    component: T
}

impl<T> ComponentWrapper<T>
{
    pub fn get(&self) -> &T
    {
        &self.component
    }

    pub fn get_mut(&mut self) -> &mut T
    {
        &mut self.component
    }
}

macro_rules! define_entities
{
    ($(($name:ident,
        $mut_func:ident,
        $set_func:ident,
        $message_name:ident,
        $component_type:ident,
        $default_type:ident
    )),+) =>
    {
        use yanyaengine::Transform;

        use crate::common::{ObjectsStore, Message};


        #[allow(non_camel_case_types)]
        pub enum Component
        {
            $($name,)+
        }

        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct EntityInfo<$($component_type=$default_type,)+>
        {
            $(pub $name: Option<$component_type>,)+
        }

        impl<$($component_type,)+> Default for EntityInfo<$($component_type,)+>
        {
            fn default() -> Self
            {
                Self{
                    $($name: None,)+
                }
            }
        }

        pub type ClientEntityInfo = EntityInfo<ClientRenderInfo, LazyTransform, UiElement>;
        pub type ClientEntities = Entities<ClientRenderInfo, LazyTransform, UiElement>;
        pub type ServerEntities = Entities;

        pub struct Entities<$($component_type=$default_type,)+>
        {
            pub components: ObjectsStore<Vec<Option<usize>>>,
            $(pub $name: ObjectsStore<ComponentWrapper<$component_type>>,)+
        }

        impl<$($component_type: OnSet<Self>,)+> Entities<$($component_type,)+>
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
                self.components.get(entity.0).is_some()
            }

            pub fn entities_iter(&self) -> impl Iterator<Item=Entity> + '_
            {
                self.components.iter().map(|(index, _)|
                {
                    Entity(index)
                })
            }

            // i hate rust generics
            pub fn update_physical(&mut self, dt: f32)
            where
                for<'a> &'a mut TransformType: Into<&'a mut Transform>,
                for<'a> &'a mut PhysicalType: Into<&'a mut Physical>,
                LazyTransformType: LazyTargettable
            {
                self.physical.iter_mut().for_each(|(_, ComponentWrapper{
                    entity,
                    component: physical
                })|
                {
                    let target = if let Some(lazy) = get_entity!(self, entity, get_mut, lazy_transform)
                    {
                        lazy.target()
                    } else
                    {
                        let transform = get_required_entity!(self, entity, get_mut, transform);

                        transform.into()
                    };

                    physical.into().physics_update(target, dt);
                });
            }

            fn update_enemy_common<F>(
                &mut self,
                dt: f32,
                mut on_state_change: F
            )
            where
                F: FnMut(Entity, &mut EnemyType, &mut LazyTransformType),
                for<'a> &'a mut EnemyType: Into<&'a mut Enemy>,
                for<'a> &'a AnatomyType: Into<&'a Anatomy>,
                for<'a> &'a mut PhysicalType: Into<&'a mut Physical>,
                LazyTransformType: LazyTargettable
            {
                self.enemy.iter_mut().for_each(|(_, ComponentWrapper{
                    entity,
                    component: enemy
                })|
                {
                    let anatomy = get_required_entity!(self, entity, get, anatomy);
                    let lazy_transform = get_required_entity!(self, entity, get_mut, lazy_transform);
                    let physical = get_required_entity!(self, entity, get_mut, physical);

                    let state_changed = enemy.into().update(
                        anatomy.into(),
                        lazy_transform.target(),
                        physical.into(),
                        dt
                    );

                    if state_changed
                    {
                        on_state_change(
                            *entity,
                            enemy,
                            lazy_transform
                        )
                    }
                });
            }

            $(
                pub fn $name(&self, entity: Entity) -> Option<&$component_type>
                {
                    get_entity!(self, entity, get, $name)
                }

                pub fn $mut_func(&mut self, entity: Entity) -> Option<&mut $component_type>
                {
                    get_entity!(self, entity, get_mut, $name)
                }

                pub fn $set_func(&mut self, entity: Entity, component: Option<$component_type>)
                {
                    if !self.exists(entity)
                    {
                        self.components.insert(entity.0, Self::empty_components());
                    }

                    if let Some(component) = component
                    {
                        let slot = &mut self.components
                            [entity.0]
                            [Component::$name as usize];

                        let component = ComponentWrapper{entity, component};

                        let previous = if let Some(id) = slot
                        {
                            self.$name.insert(*id, component)
                        } else
                        {
                            let id = self.$name.push(component);
                            
                            *slot = Some(id);

                            None
                        };

                        $component_type::on_set(
                            previous.map(|x| x.component),
                            self,
                            entity
                        );
                    }
                }
            )+

            pub fn push(&mut self, info: EntityInfo<$($component_type,)+>) -> Entity
            {
                let is_child = info.parent.is_some();

                let id = if is_child
                {
                    self.components.last_key()
                } else
                {
                    self.components.vacant_key()
                };

                let id = Entity(id);

                let indices = self.info_components(id, info);

                if is_child
                {
                    self.components.push_last(indices);
                } else
                {
                    self.components.push(indices);
                }

                id
            }

            pub fn remove(&mut self, entity: Entity)
            {
                if !self.exists(entity)
                {
                    return;
                }

                let components = &self.components[entity.0];

                $(if let Some(id) = components[Component::$name as usize]
                {
                    self.$name.remove(id);
                })+

                self.components.remove(entity.0);
            }

            fn info_components(
                &mut self,
                entity: Entity,
                info: EntityInfo<$($component_type,)+>
            ) -> Vec<Option<usize>>
            {
                vec![
                    $({
                        info.$name.map(|component|
                        {
                            self.$name.push(ComponentWrapper{entity, component})
                        })
                    },)+
                ]
            }

            fn empty_components() -> Vec<Option<usize>>
            {
                vec![$(
                    {
                        let _ = Component::$name;
                        None
                    }
                ,)+]
            }

            fn handle_message_common(&mut self, message: Message) -> Option<Message>
            where
                for<'a> &'a mut AnatomyType: Into<&'a mut Anatomy>
            {
                match message
                {
                    Message::EntityDamage{entity, damage} =>
                    {
                        if let Some(anatomy) = get_entity!(self, entity, get_mut, anatomy)
                        {
                            use crate::common::Damageable;

                            anatomy.into().damage(damage);

                            AnatomyType::on_set(None, self, entity);
                        }

                        None
                    },
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
                        let transform = info.transform.clone();
                        $({
                            let component = info.$name.map(|x|
                            {
                                x.server_to_client(transform.clone(), create_info)
                            });

                            self.$set_func(entity, component);
                        })+

                        let components = &self.components[entity.0];

                        let lazy = get_component!(self, components, get_mut, lazy_transform);

                        if let Some(lazy) = lazy
                        {
                            let parent = get_component!(self, components, get, parent);
                            let new_transform = if let Some(parent) = parent
                            {
                                if let Some(parent) = get_entity!(self, parent.parent, get, transform)
                                {
                                    lazy.combine(parent)
                                } else
                                {
                                    lazy.target_local.clone()
                                }
                            } else
                            {
                                lazy.target_local.clone()
                            };

                            lazy.reset_current(new_transform.clone());

                            let transform = get_required_component!(self, components, get_mut, transform);

                            *transform = new_transform;
                        }

                        None
                    },
                    $(Message::$message_name{entity, $name} =>
                    {
                        let transform = (self.exists(entity))
                            .then(|| self.transform(entity).cloned())
                            .flatten();

                        let component = $name.server_to_client(transform, create_info);

                        self.$set_func(entity, Some(component));

                        None
                    },)+
                    x => Some(x)
                }
            }

            pub fn update_render(&mut self)
            {
                self.render.iter_mut().for_each(|(_, ComponentWrapper{
                    entity,
                    component: object
                })|
                {
                    let transform = get_required_entity!(self, entity, get, transform);

                    use yanyaengine::TransformContainer;

                    if let Some(object) = object.object.as_mut()
                    {
                        object.set_transform(transform.clone());
                    }
                });
            }

            pub fn update_lazy(&mut self, dt: f32)
            {
                self.lazy_transform.iter_mut().for_each(|(_, ComponentWrapper{
                    entity,
                    component: lazy
                })|
                {
                    let parent = get_entity!(self, entity, get, parent);

                    let target_global = parent.map(|parent|
                    {
                        get_entity!(self, parent.parent, get, transform).cloned()
                    }).flatten();

                    let transform = get_required_entity!(self, entity, get_mut, transform);
                    *transform = lazy.next(target_global, dt);
                });
            }

            pub fn update_enemy(&mut self, dt: f32 )
            {
                self.update_enemy_common(dt, |_, _, _| {});
            }

            pub fn update_sprites(
                &mut self,
                create_info: &mut ObjectCreateInfo,
                enemies_info: &EnemiesInfo
            )
            {
                self.enemy.iter_mut().for_each(|(_, ComponentWrapper{
                    entity,
                    component: enemy
                })|
                {
                    let lazy = get_required_entity!(self, entity, get_mut, lazy_transform);
                    let render = get_required_entity!(self, entity, get_mut, render);
                    let transform = get_required_entity!(self, entity, get_mut, transform);

                    let updated = enemy.update_sprite(
                        lazy,
                        enemies_info,
                        render,
                        |render, texture|
                        {
                            render.set_sprite(create_info, Some(transform), texture);
                        }
                    );

                    if updated
                    {
                        *transform = lazy.target_local.clone();
                    }
                });
            }

            pub fn anatomy_changed(&mut self, entity: Entity)
            {
                if let Some(enemy) = get_entity!(self, entity, get_mut, enemy)
                {
                    let anatomy = get_required_entity!(self, entity, get, anatomy);

                    let can_move = anatomy.speed().is_some();

                    use crate::common::enemy::SpriteState;

                    let state = if can_move
                    {
                        SpriteState::Normal
                    } else
                    {
                        SpriteState::Lying
                    };

                    enemy.set_sprite(state);
                }
            }
        }

        impl ServerEntities
        {
            pub fn info(&self, entity: Entity) -> EntityInfo
            {
                let components = &self.components[entity.0];

                EntityInfo{$(
                    $name: components[Component::$name as usize].map(|id|
                    {
                        self.$name[id].component.clone()
                    }),
                )+}
            }

            pub fn update_enemy(&mut self, messager: &mut ConnectionsHandler, dt: f32)
            {
                self.update_enemy_common(dt, |entity, enemy, lazy_transform|
                {
                    messager.send_message(Message::SetEnemy{
                        entity,
                        enemy: enemy.clone()
                    });

                    messager.send_message(Message::SetLazyTransform{
                        entity,
                        lazy_transform: lazy_transform.clone()
                    });
                });
            }

            pub fn update_lazy(&mut self)
            {
                self.lazy_transform.iter_mut().for_each(|(_, ComponentWrapper{
                    entity,
                    component: lazy
                })|
                {
                    let parent = get_entity!(self, entity, get, parent);

                    let target_global = parent.map(|parent|
                    {
                        get_entity!(self, parent.parent, get, transform).cloned()
                    }).flatten();

                    let transform = get_required_entity!(self, entity, get_mut, transform);

                    *transform = lazy.target_global(target_global.as_ref());
                });
            }

            pub fn update_sprites(
                &mut self,
                enemies_info: &EnemiesInfo
            )
            {
                self.enemy.iter_mut().for_each(|(_, ComponentWrapper{
                    entity,
                    component: enemy
                })|
                {
                    let lazy = get_required_entity!(self, entity, get_mut, lazy_transform);

                    enemy.update_sprite_common(lazy, enemies_info);
                });
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
                let message = self.handle_message_common(message)?;

                match message
                {
                    $(Message::$message_name{entity, $name} =>
                    {
                        self.$set_func(entity, Some($name));

                        None
                    },)+
                    x => Some(x)
                }
            }
        }
    }
}

define_entities!{
    (render, render_mut, set_render, SetRender, RenderType, RenderInfo),
    (lazy_transform, lazy_transform_mut, set_lazy_transform, SetLazyTransform, LazyTransformType, LazyTransformServer),
    (ui_element, ui_element_mut, set_ui_element, SetUiElement, UiElementType, UiElementServer),
    (enemy, enemy_mut, set_enemy, SetEnemy, EnemyType, Enemy),
    (parent, parent_mut, set_parent, SetParent, ParentType, Parent),
    (transform, transform_mut, set_transform, SetTransform, TransformType, Transform),
    (player, player_mut, set_player, SetPlayer, PlayerType, Player),
    (physical, physical_mut, set_physical, SetPhysical, PhysicalType, Physical),
    (anatomy, anatomy_mut, set_anatomy, SetAnatomy, AnatomyType, Anatomy)
}
