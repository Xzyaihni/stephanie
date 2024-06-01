use std::cell::{Ref, RefMut, RefCell};

use serde::{Serialize, Deserialize};

use nalgebra::Vector2;

use yanyaengine::game_object::*;

use crate::{
    server::ConnectionsHandler,
    client::{
        UiElement,
        UiEvent
    },
    common::{
        render_info::*,
        collider::*,
        EntityPasser,
        Inventory,
        Anatomy,
        Player,
        Enemy,
        EnemiesInfo,
        Physical,
        LazyTransform,
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
                $this.$component.get(id).unwrap_or_else(||
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

pub trait ServerToClient<T>
{
    fn server_to_client(
        self,
        transform: impl FnOnce() -> Transform,
        create_info: &mut ObjectCreateInfo
    ) -> T;
}

impl<T> ServerToClient<T> for T
{
    fn server_to_client(
        self,
        _transform: impl FnOnce() -> Transform,
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
    pub fn from_raw(raw: usize) -> Entity
    {
        Entity(raw)
    }

    pub fn get_raw(&self) -> usize
    {
        self.0
    }
}

pub trait OnSet<EntitiesType>
where
    Self: Sized
{
    fn on_set(previous: Option<Self>, entities: &EntitiesType, entity: Entity);
}

macro_rules! no_on_set
{
    ($($name:ident),*) =>
    {
        $(impl<E> OnSet<E> for $name
        {
            fn on_set(_previous: Option<Self>, _entities: &E, _entity: Entity) {}
        })*
    }
}

macro_rules! no_on_set_for
{
    ($container:ident, $name:ident) =>
    {
        impl OnSet<$container> for $name
        {
            fn on_set(_previous: Option<Self>, _entities: &$container, _entity: Entity) {}
        }
    }
}

no_on_set!{
    ClientRenderInfo,
    RenderInfo,
    LazyTransform,
    Inventory,
    Parent,
    Transform,
    Player,
    Collider,
    Physical,
    UiElement,
    UiElementServer
}

no_on_set_for!{ServerEntities, Enemy}

impl OnSet<ClientEntities> for Enemy
{
    fn on_set(previous: Option<Self>, entities: &ClientEntities, entity: Entity)
    {
        if let Some(previous) = previous
        {
            let mut enemy = entities.enemy_mut(entity).unwrap();

            enemy.with_previous(previous);
        }
    }
}

no_on_set_for!{ServerEntities, Anatomy}

impl OnSet<ClientEntities> for Anatomy
{
    fn on_set(_previous: Option<Self>, entities: &ClientEntities, entity: Entity)
    {
        entities.anatomy_changed(entity);
    }
}

// parent must always come before child !! (index wise)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parent
{
    pub visible: bool,
    entity: Entity
}

impl Parent
{
    pub fn new(entity: Entity, visible: bool) -> Self
    {
        Self{visible, entity}
    }
}

type UiElementServer = ();

#[derive(Debug, Clone)]
pub struct ComponentWrapper<T>
{
    entity: Entity,
    component: RefCell<T>
}

impl<T> ComponentWrapper<T>
{
    pub fn get(&self) -> Ref<T>
    {
        self.component.borrow()
    }

    pub fn get_mut(&self) -> RefMut<T>
    {
        self.component.borrow_mut()
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
                for<'a> &'a mut PhysicalType: Into<&'a mut Physical>,
                for<'a> &'a mut TransformType: Into<&'a mut Transform>,
                LazyTransformType: LazyTargettable
            {
                self.physical.iter().for_each(|(_, ComponentWrapper{
                    entity,
                    component: physical
                })|
                {
                    if let Some(mut target) = self.transform_target(*entity)
                    {
                        let mut physical = physical.borrow_mut();
                        (&mut *physical).into().physics_update(&mut target, dt);
                    }
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
                self.enemy.iter().for_each(|(_, ComponentWrapper{
                    entity,
                    component: enemy
                })|
                {
                    let anatomy = self.anatomy(*entity).unwrap();
                    let mut lazy_transform = self.lazy_transform_mut(*entity).unwrap();
                    let mut physical = self.physical_mut(*entity).unwrap();

                    let mut enemy = enemy.borrow_mut();
                    let state_changed = (&mut *enemy).into().update(
                        (&*anatomy).into(),
                        lazy_transform.target(),
                        (&mut *physical).into(),
                        dt
                    );

                    if state_changed
                    {
                        on_state_change(
                            *entity,
                            &mut enemy,
                            &mut lazy_transform
                        )
                    }
                });
            }

            $(
                pub fn $name(&self, entity: Entity) -> Option<Ref<$component_type>>
                {
                    get_entity!(self, entity, get, $name)
                }

                pub fn $mut_func(&self, entity: Entity) -> Option<RefMut<$component_type>>
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

                        let component = ComponentWrapper{
                            entity,
                            component: RefCell::new(component)
                        };

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
                            previous.map(|x| x.component.into_inner()),
                            self,
                            entity
                        );
                    }
                }
            )+

            pub fn transform_target(&self, entity: Entity) -> Option<RefMut<Transform>>
            where
                for<'a> &'a mut TransformType: Into<&'a mut Transform>,
                LazyTransformType: LazyTargettable<Transform>
            {
                if let Some(lazy) = self.lazy_transform_mut(entity)
                {
                    Some(RefMut::map(lazy, |x| x.target()))
                } else if let Some(transform) = self.transform_mut(entity)
                {
                    Some(RefMut::map(transform, |x| x.into()))
                } else
                {
                    None
                }
            }

            pub fn push(&mut self, mut info: EntityInfo<$($component_type,)+>) -> Entity
            where
                for<'a> &'a ParentType: Into<&'a Parent>,
                TransformType: Clone,
                LazyTransformType: LazyTargettable<TransformType>
            {
                let id = if let Some(parent) = info.parent.as_ref()
                {
                    self.components.take_after_key(parent.into().entity.0)
                } else
                {
                    self.components.take_vacant_key()
                };

                let entity_id = Entity(id);

                if let Some(lazy_transform) = info.lazy_transform.as_ref()
                {
                    info.transform = Some(lazy_transform.target_ref().clone());
                }

                let indices = self.push_info_components(entity_id, info);

                self.components.insert(id, indices);

                entity_id
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

            fn push_info_components(
                &mut self,
                entity: Entity,
                info: EntityInfo<$($component_type,)+>
            ) -> Vec<Option<usize>>
            where
                for<'a> &'a ParentType: Into<&'a Parent>,
            {
                let parent = info.parent.as_ref().map(|x| x.into().entity.0);
                vec![
                    $({
                        info.$name.map(|component|
                        {
                            let wrapper = ComponentWrapper{
                                entity,
                                component: RefCell::new(component)
                            };

                            if let Some(parent) = parent
                            {
                                self.$name.push_after(parent, wrapper)
                            } else
                            {
                                self.$name.push(wrapper)
                            }
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
                for<'a> &'a mut AnatomyType: Into<&'a mut Anatomy>,
                for<'a> &'a mut TransformType: Into<&'a mut Transform>,
                LazyTransformType: LazyTargettable<Transform>
            {
                match message
                {
                    Message::EntityDamage{entity, damage} =>
                    {
                        if let Some(mut anatomy) = self.anatomy_mut(entity)
                        {
                            use crate::common::Damageable;

                            (&mut *anatomy).into().damage(damage);

                            AnatomyType::on_set(None, self, entity);
                        }

                        None
                    },
                    Message::SetTarget{entity, target} =>
                    {
                        if self.exists(entity)
                        {
                            if let Some(mut lazy) = self.lazy_transform_mut(entity)
                            {
                                *lazy.target() = target;
                            } else
                            {
                                let mut transform = self.transform_mut(entity).unwrap();
                                *(&mut *transform).into() = target;
                            }
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
                        let transform = self.transform_clone(entity)
                            .or_else(|| info.transform.clone());

                        $({
                            let component = info.$name.map(|x|
                            {
                                x.server_to_client(||
                                {
                                    transform.clone().expect("server to client expects transform")
                                }, create_info)
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
                                if let Some(parent) = self.transform(parent.entity)
                                {
                                    lazy.combine(&parent)
                                } else
                                {
                                    lazy.target_local.clone()
                                }
                            } else
                            {
                                lazy.target_local.clone()
                            };

                            let mut transform = get_required_component!(self, components, get_mut, transform);

                            *transform = new_transform;
                        }

                        None
                    },
                    $(Message::$message_name{entity, $name} =>
                    {
                        let component = $name.server_to_client(||
                        {
                            self.transform_clone(entity).expect("expects a transform")
                        }, create_info);

                        self.$set_func(entity, Some(component));

                        None
                    },)+
                    x => Some(x)
                }
            }

            fn transform_clone(&self, entity: Entity) -> Option<Transform>
            {
                (self.exists(entity))
                    .then(|| self.transform(entity).as_deref().cloned())
                    .flatten()
            }

            pub fn update_visibility(&mut self)
            {
                self.parent.iter().for_each(|(_, ComponentWrapper{
                    entity,
                    component: parent
                })|
                {
                    let parent = parent.borrow();

                    if let Some(mut render) = self.render_mut(*entity)
                    {
                        let parent_visible = self.render(parent.entity).map(|parent_render|
                        {
                            parent_render.visible
                        }).unwrap_or(true);

                        render.visible = parent.visible && parent_visible;
                    }
                });
            }

            pub fn update_render(&mut self)
            {
                self.render.iter().for_each(|(_, ComponentWrapper{
                    entity,
                    component: object
                })|
                {
                    let transform = self.transform(*entity).unwrap();

                    if let Some(object) = object.borrow_mut().object.as_mut()
                    {
                        object.set_transform(transform.clone());
                    }
                });
            }

            pub fn update_colliders_local<P>(
                &mut self,
                passer: &mut P,
                others: &Self
            )
            where
                P: EntityPasser
            {

                self.collider.iter().for_each(|(_, ComponentWrapper{
                    entity,
                    component: collider
                })|
                {
                    self.collider.iter().filter(|(_, x)|
                    {
                        x.entity != *entity
                    }).map(|(_, ComponentWrapper{
                        entity: other_entity,
                        component: other_collider
                    })|
                    {
                        (
                            None,
                            self.physical_mut(*other_entity),
                            self.transform_target(*other_entity).unwrap(),
                            other_collider.borrow().clone()
                        )
                    }).chain(others.collider.iter().map(|(_, ComponentWrapper{
                        entity: other_entity,
                        component: other_collider
                    })|
                    {
                        (
                            Some(|passer: &mut P, physical: Option<Physical>, transform: Transform|
                            {
                                passer.send_message(Message::SetTarget{
                                    entity: *other_entity,
                                    target: transform
                                });

                                if let Some(physical) = physical
                                {
                                    passer.send_message(Message::SetPhysical{
                                        entity: *other_entity,
                                        physical
                                    });
                                }
                            }),
                            others.physical_mut(*other_entity),
                            others.transform_target(*other_entity).unwrap(),
                            other_collider.borrow().clone()
                        )
                    })).for_each(|(
                        on_collision,
                        mut other_physical,
                        mut other_transform,
                        other_collider
                    )|
                    {
                        let mut physical = self.physical_mut(*entity);
                        let mut transform = self.transform_target(*entity).unwrap();

                        let this = CollidingInfo{
                            physical: physical.as_deref_mut(),
                            transform: &mut transform,
                            collider: collider.borrow().clone()
                        };

                        let collided = this.resolve(CollidingInfo{
                            physical: other_physical.as_deref_mut(),
                            transform: &mut other_transform,
                            collider: other_collider
                        });

                        if let (Some(on_collision), true) = (on_collision, collided)
                        {
                            on_collision(
                                passer,
                                other_physical.map(|x| x.clone()),
                                other_transform.clone()
                            );
                        }
                    });
                });
            }

            pub fn update_colliders(
                &mut self,
                passer: &mut impl EntityPasser
            )
            {
                self.collider.iter().for_each(|(_, ComponentWrapper{
                    entity: other_entity,
                    component: other_collider
                })|
                {
                    self.collider.iter().filter(|(_, x)|
                    {
                        x.entity != *other_entity
                    }).for_each(|(_, ComponentWrapper{
                        entity,
                        component: collider
                    })|
                    {
                        let mut physical = self.physical_mut(*entity);
                        let mut transform = self.transform_target(*entity).unwrap();

                        let this = CollidingInfo{
                            physical: physical.as_deref_mut(),
                            transform: &mut transform,
                            collider: collider.borrow().clone()
                        };

                        let mut other_physical = self.physical_mut(*other_entity);
                        let mut other_transform = self.transform_target(*other_entity).unwrap();
                        let collision = this.resolve(CollidingInfo{
                            physical: other_physical.as_deref_mut(),
                            transform: &mut other_transform,
                            collider: other_collider.borrow().clone()
                        });

                        if collision
                        {
                            passer.send_message(Message::SetTarget{
                                entity: *entity,
                                target: transform.clone()
                            });

                            if let Some(physical) = physical
                            {
                                passer.send_message(Message::SetPhysical{
                                    entity: *entity,
                                    physical: physical.clone()
                                });
                            }

                            passer.send_message(Message::SetTarget{
                                entity: *other_entity,
                                target: other_transform.clone()
                            });

                            if let Some(physical) = other_physical
                            {
                                passer.send_message(Message::SetPhysical{
                                    entity: *other_entity,
                                    physical: physical.clone()
                                });
                            }
                        }
                    });
                });
            }

            fn update_lazy_one(
                &self,
                entity: Entity,
                mut lazy: RefMut<LazyTransform>,
                dt: f32
            )
            {
                let parent = self.parent(entity);

                let target_global = parent.map(|parent|
                {
                    self.transform(parent.entity).as_deref().cloned()
                }).flatten();

                let mut transform = self.transform_mut(entity).unwrap();

                *transform = lazy.next(
                    self.physical(entity).as_deref(),
                    transform.clone(),
                    target_global,
                    dt
                );
            }

            pub fn update_lazy(&mut self, dt: f32)
            {
                self.lazy_transform.iter().for_each(|(_, ComponentWrapper{
                    entity,
                    component: lazy
                })|
                {
                    self.update_lazy_one(*entity, lazy.borrow_mut(), dt);
                });
            }

            pub fn update_enemy(&mut self, dt: f32 )
            {
                self.update_enemy_common(dt, |_, _, _| {});
            }

            pub fn update_ui(
                &mut self,
                camera_position: Vector2<f32>,
                event: UiEvent
            ) -> bool
            {
                let mut captured = false;
                // borrow checker more like goofy ahh
                // rev to early exit if child is captured
                self.ui_element.iter().rev().for_each(|(_, ComponentWrapper{
                    entity,
                    component: ui_element
                })|
                {
                    let is_visible = self.render(*entity).map(|x| x.visible).unwrap_or(false);
                    if is_visible
                    {
                        captured = ui_element.borrow_mut().update(
                            &*self,
                            *entity,
                            camera_position,
                            &event,
                            captured
                        ) || captured;
                    }
                });

                captured
            }

            pub fn update_sprites(
                &self,
                create_info: &mut ObjectCreateInfo,
                enemies_info: &EnemiesInfo
            )
            {
                self.enemy.iter().for_each(|(_, ComponentWrapper{
                    entity,
                    component: enemy
                })|
                {
                    let mut lazy = self.lazy_transform_mut(*entity).unwrap();
                    let changed = {
                        let mut render = self.render_mut(*entity).unwrap();
                        let mut transform = self.transform_mut(*entity).unwrap();

                        enemy.borrow_mut().update_sprite(
                            &mut *lazy,
                            enemies_info,
                            &mut render,
                            |render, texture|
                            {
                                render.set_sprite(create_info, Some(&mut transform), texture);
                            }
                        )
                    };

                    if changed
                    {
                        self.update_lazy_one(*entity, lazy, 0.0001);
                    }
                });
            }

            pub fn anatomy_changed(&self, entity: Entity)
            {
                if let Some(mut enemy) = self.enemy_mut(entity)
                {
                    let anatomy = self.anatomy(entity).unwrap();

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
                        self.$name[id].component.borrow().clone()
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
                self.lazy_transform.iter().for_each(|(_, ComponentWrapper{
                    entity,
                    component: lazy
                })|
                {
                    let parent = self.parent(*entity);

                    let target_global = parent.map(|parent|
                    {
                        self.transform(parent.entity).as_deref().cloned()
                    }).flatten();

                    let mut transform = self.transform_mut(*entity).unwrap();

                    *transform = lazy.borrow_mut().target_global(target_global.as_ref());
                });
            }

            pub fn update_sprites(
                &mut self,
                enemies_info: &EnemiesInfo
            )
            {
                self.enemy.iter().for_each(|(_, ComponentWrapper{
                    entity,
                    component: enemy
                })|
                {
                    let mut lazy = self.lazy_transform_mut(*entity).unwrap();

                    enemy.borrow_mut().update_sprite_common(&mut *lazy, enemies_info);
                });
            }

            pub fn push_message(&mut self, info: EntityInfo) -> Message
            {
                let entity = self.push(info);

                Message::EntitySet{entity, info: self.info(entity)}
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
    (lazy_transform, lazy_transform_mut, set_lazy_transform, SetLazyTransform, LazyTransformType, LazyTransform),
    (ui_element, ui_element_mut, set_ui_element, SetUiElement, UiElementType, UiElementServer),
    (inventory, inventory_mut, set_inventory, SetInventory, InventoryType, Inventory),
    (enemy, enemy_mut, set_enemy, SetEnemy, EnemyType, Enemy),
    (parent, parent_mut, set_parent, SetParent, ParentType, Parent),
    (transform, transform_mut, set_transform, SetTransform, TransformType, Transform),
    (player, player_mut, set_player, SetPlayer, PlayerType, Player),
    (collider, collider_mut, set_collider, SetCollider, ColliderType, Collider),
    (physical, physical_mut, set_physical, SetPhysical, PhysicalType, Physical),
    (anatomy, anatomy_mut, set_anatomy, SetAnatomy, AnatomyType, Anatomy)
}
