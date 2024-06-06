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
        watcher::*,
        lazy_transform::*,
        EntityPasser,
        Inventory,
        Anatomy,
        Player,
        Enemy,
        EnemiesInfo,
        Physical
    }
};


macro_rules! components
{
    ($this:expr, $entity:expr) =>
    {
        if $entity.local
        {
            &$this.local_components
        } else
        {
            &$this.components
        }
    }
}

macro_rules! components_mut
{
    ($this:expr, $entity:expr) =>
    {
        if $entity.local
        {
            &mut $this.local_components
        } else
        {
            &mut $this.components
        }
    }
}

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
        get_component!(
            $this,
            components!($this, $entity)[$entity.id],
            $access_type,
            $component
        )
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
pub struct Entity
{
    local: bool,
    id: usize
}

impl Entity
{
    pub fn local(&self) -> bool
    {
        self.local
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
    FollowRotation,
    Inventory,
    String,
    Parent,
    Transform,
    Player,
    Collider,
    Physical,
    Watchers,
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

    pub fn entity(&self) -> Entity
    {
        self.entity
    }
}

pub type UiElementServer = ();

macro_rules! normal_define
{
    ($(($fn_ref:ident, $fn_mut:ident, $value:ident)),+) =>
    {
        $(
        fn $fn_ref(&self, entity: Entity) -> Option<Ref<$value>>;
        fn $fn_mut(&self, entity: Entity) -> Option<RefMut<$value>>;
        )+
    }
}

macro_rules! normal_forward_impl
{
    ($(($fn_ref:ident, $fn_mut:ident, $value:ident)),+) =>
    {
        $(
        fn $fn_ref(&self, entity: Entity) -> Option<Ref<$value>>
        {
            Self::$fn_ref(self, entity)
        }

        fn $fn_mut(&self, entity: Entity) -> Option<RefMut<$value>>
        {
            Self::$fn_mut(self, entity)
        }
        )+
    }
}

pub trait AnyEntities
{
    type CreateInfo<'a>;

    normal_define!{
        (transform, transform_mut, Transform),
        (parent, parent_mut, Parent),
        (physical, physical_mut, Physical),
        (player, player_mut, Player),
        (enemy, enemy_mut, Enemy),
        (named, named_mut, String),
        (collider, collider_mut, Collider)
    }

    fn lazy_target_ref(&self, entity: Entity) -> Option<Ref<Transform>>;
    fn lazy_target(&self, entity: Entity) -> Option<RefMut<Transform>>;

    fn is_visible(&self, entity: Entity) -> bool;
    fn visible_target(&self, entity: Entity) -> Option<RefMut<bool>>;

    fn remove(&mut self, entity: Entity);
    // i cant make remove the &mut cuz reborrowing would stop working :/
    fn push(
        &mut self,
        create_info: &mut Self::CreateInfo<'_>,
        local: bool,
        info: EntityInfo
    ) -> Entity;

    fn name(
        &self,
        enemies_info: &EnemiesInfo,
        entity: Entity
    ) -> Option<String>
    {
        self.player(entity).map(|player|
        {
            player.name.clone()
        }).or_else(||
        {
            self.enemy(entity).map(|enemy|
            {
                enemy.info(enemies_info).name.clone()
            })
        }).or_else(||
        {
            self.named(entity).as_deref().cloned()
        })
    }

    fn parent_transform(&self, entity: Entity) -> Option<Transform>
    {
        self.parent(entity).and_then(|parent|
        {
            self.transform(parent.entity).as_deref().cloned()
        })
    }

    fn target_ref(&self, entity: Entity) -> Option<Ref<Transform>>
    {
        self.lazy_target_ref(entity).or_else(||
        {
            self.transform(entity)
        })
    }

    fn target(&self, entity: Entity) -> Option<RefMut<Transform>>
    {
        self.lazy_target(entity).or_else(||
        {
            self.transform_mut(entity)
        })
    }
}

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

macro_rules! impl_common_systems
{
    () =>
    {
        pub fn update_physical(&mut self, dt: f32)
        {
            self.physical.iter().for_each(|(_, ComponentWrapper{
                entity,
                component: physical
            })|
            {
                if let Some(mut target) = self.target(*entity)
                {
                    let mut physical = physical.borrow_mut();
                    physical.physics_update(&mut target, dt);
                }
            });
        }

        pub fn update_follows(&mut self, dt: f32)
        {
            self.follow_rotation.iter().for_each(|(_, ComponentWrapper{
                entity,
                component: follow_rotation
            })|
            {
                let mut follow_rotation = follow_rotation.borrow_mut();

                let mut transform = self.transform_mut(*entity).unwrap();
                let current = &mut transform.rotation;
                let target = self.transform(follow_rotation.parent()).unwrap().rotation;

                follow_rotation.next(current, target, dt);
            });
        }
    }
}

macro_rules! common_trait_impl
{
    () =>
    {
        normal_forward_impl!{
            (transform, transform_mut, Transform),
            (parent, parent_mut, Parent),
            (physical, physical_mut, Physical),
            (player, player_mut, Player),
            (enemy, enemy_mut, Enemy),
            (named, named_mut, String),
            (collider, collider_mut, Collider)
        }

        fn lazy_target_ref(&self, entity: Entity) -> Option<Ref<Transform>>
        {
            Self::lazy_transform(self, entity).map(|lazy|
            {
                Ref::map(lazy, |x| x.target_ref())
            })
        }

        fn lazy_target(&self, entity: Entity) -> Option<RefMut<Transform>>
        {
            Self::lazy_transform_mut(self, entity).map(|lazy|
            {
                RefMut::map(lazy, |x| x.target())
            })
        }

        fn is_visible(&self, entity: Entity) -> bool
        {
            self.render(entity).map(|x| x.visible).unwrap_or(false)
        }

        fn visible_target(&self, entity: Entity) -> Option<RefMut<bool>>
        {
            self.parent_mut(entity).map(|parent|
            {
                RefMut::map(parent, |x| &mut x.visible)
            }).or_else(||
            {
                self.render_mut(entity).map(|render|
                {
                    RefMut::map(render, |x| &mut x.visible)
                })
            })
        }

        fn remove(&mut self, entity: Entity)
        {
            Self::remove(self, entity);
        }
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

        impl EntityInfo
        {
            pub fn target_ref(&self) -> Option<&Transform>
            {
                self.lazy_transform.as_ref()
                    .map(|lazy| lazy.target_ref())
                    .or_else(|| self.transform.as_ref())
            }

            pub fn target(&mut self) -> Option<&mut Transform>
            {
                self.lazy_transform.as_mut()
                    .map(|lazy| lazy.target())
                    .or_else(|| self.transform.as_mut())
            }
        }

        pub type ClientEntityInfo = EntityInfo<ClientRenderInfo, UiElement>;
        pub type ClientEntities = Entities<ClientRenderInfo, UiElement>;
        pub type ServerEntities = Entities;

        impl ClientEntityInfo
        {
            pub fn from_server(
                create_info: &mut ObjectCreateInfo,
                info: EntityInfo
            ) -> Self
            {
                let transform = info.target_ref().cloned();

                Self{
                    $($name: info.$name.map(|x|
                    {
                        x.server_to_client(|| transform.clone().unwrap(), create_info)
                    }),)+
                }
            }
        }

        impl AnyEntities for ClientEntities
        {
            type CreateInfo<'a> = ObjectCreateInfo<'a>;

            common_trait_impl!{}

            fn push(
                &mut self,
                create_info: &mut Self::CreateInfo<'_>,
                local: bool,
                info: EntityInfo
            ) -> Entity
            {
                let info = ClientEntityInfo::from_server(create_info, info);

                Self::push(self, local, info)
            }
        }

        impl AnyEntities for ServerEntities
        {
            type CreateInfo<'a> = ();

            common_trait_impl!{}

            fn push(&mut self, _create_info: &mut (), local: bool, info: EntityInfo) -> Entity
            {
                Self::push(self, local, info)
            }
        }

        pub struct Entities<$($component_type=$default_type,)+>
        {
            pub local_components: ObjectsStore<Vec<Option<usize>>>,
            pub components: ObjectsStore<Vec<Option<usize>>>,
            $(pub $name: ObjectsStore<ComponentWrapper<$component_type>>,)+
        }

        impl<$($component_type: OnSet<Self>,)+> Entities<$($component_type,)+>
        where
            Self: AnyEntities
        {
            pub fn new() -> Self
            {
                Self{
                    local_components: ObjectsStore::new(),
                    components: ObjectsStore::new(),
                    $($name: ObjectsStore::new(),)+
                }
            }

            pub fn exists(&self, entity: Entity) -> bool
            {
                components!(self, entity).get(entity.id).is_some()
            }

            pub fn entities_iter(&self) -> impl Iterator<Item=Entity> + '_
            {
                self.components.iter().map(|(id, _)|
                {
                    Entity{local: false, id}
                }).chain(self.local_components.iter().map(|(id, _)|
                {
                    Entity{local: true, id}
                }))
            }

            pub fn info_ref(&self, entity: Entity) -> EntityInfo<$(Ref<$component_type>,)+>
            {
                let components = &components!(self, entity)[entity.id];

                EntityInfo{$(
                    $name: components[Component::$name as usize].map(|id|
                    {
                        self.$name[id].component.borrow()
                    }),
                )+}
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

            pub fn update_watchers(
                &mut self,
                create_info: &mut <Self as AnyEntities>::CreateInfo<'_>,
                dt: f32
            )
            where
                for<'a> &'a mut WatchersType: Into<&'a mut Watchers>
            {
                // the borrow checker forcing me to collect into vectors cuz why not!
                let pairs: Vec<_> = self.watchers.iter().map(|(_, ComponentWrapper{
                    entity,
                    component: watchers
                })|
                {
                    let actions = (&mut *watchers.borrow_mut()).into().execute(self, *entity, dt);

                    (*entity, actions)
                }).collect();

                pairs.into_iter().for_each(|(entity, actions)|
                {
                    actions.into_iter().for_each(|action|
                    {
                        action.execute(create_info, self, entity);
                    });
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
                        components_mut!(self, entity)
                            .insert(entity.id, Self::empty_components());
                    }

                    if let Some(component) = component
                    {
                        let slot = &mut components_mut!(self, entity)
                            [entity.id]
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

            // when r TransformType = Transform constraints coming RUST?!?!? 
            pub fn push(
                &mut self,
                local: bool,
                mut info: EntityInfo<$($component_type,)+>
            ) -> Entity
            where
                for<'a> &'a ParentType: Into<&'a Parent>,
                for<'a> &'a LazyTransformType: Into<&'a LazyTransform>,
                for<'a> &'a FollowRotationType: Into<&'a FollowRotation>,
                for<'a> &'a TransformType: Into<&'a Transform>,
                for<'a> &'a mut TransformType: Into<&'a mut Transform>,
                for<'a> &'a mut Option<TransformType>: Into<&'a mut Option<Transform>>,
                TransformType: Clone,
                LazyTransformType: LazyTargettable<TransformType>
            {
                let components = if local
                {
                    &mut self.local_components
                } else
                {
                    &mut self.components
                };

                let id = if let Some(parent) = info.parent.as_ref()
                {
                    components.take_after_key(parent.into().entity.id)
                } else
                {
                    components.take_vacant_key()
                };

                let entity_id = Entity{local, id};

                if let Some(lazy) = info.lazy_transform.as_ref()
                {
                    let parent_transform = info.parent.as_ref().and_then(|x|
                    {
                        self.transform((&*x).into().entity).as_deref().map(|x| x.into()).cloned()
                    });

                    let new_transform = (&*lazy).into().target_global(parent_transform.as_ref());
                    *(&mut info.transform).into() = Some(new_transform);
                }

                if let Some(follow_rotation) = info.follow_rotation.as_ref()
                {
                    let transform = info.transform.as_mut().unwrap();
                    let transform: &mut Transform = transform.into();

                    let current = &mut transform.rotation;

                    let follow_rotation = follow_rotation.into();

                    let parent_transform = self.transform(follow_rotation.parent()).unwrap();
                    let target = (&*parent_transform).into().rotation;

                    *current = target;
                }

                let indices = self.push_info_components(entity_id, info);

                components_mut!(self, entity_id).insert(id, indices);

                entity_id
            }

            pub fn remove(&mut self, entity: Entity)
            {
                if !self.exists(entity)
                {
                    return;
                }

                let components = &components!(self, entity)[entity.id];

                $(if let Some(id) = components[Component::$name as usize]
                {
                    self.$name.remove(id);
                })+

                components_mut!(self, entity).remove(entity.id);
            }

            fn push_info_components(
                &mut self,
                entity: Entity,
                info: EntityInfo<$($component_type,)+>
            ) -> Vec<Option<usize>>
            where
                for<'a> &'a ParentType: Into<&'a Parent>,
            {
                let parent = info.parent.as_ref().map(|x| x.into().entity.id);
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
                            if let Some(mut x) = self.target(entity)
                            {
                                *x = target;
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

                        debug_assert!(!entity.local);
                        let components = &self.components[entity.id];

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
                        debug_assert!(!entity.local);
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

            impl_common_systems!{}

            pub fn update_children(&mut self)
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

            pub fn is_lootable(&self, entity: Entity) -> bool
            {
                let is_player = self.player(entity).is_some();
                let has_inventory = self.inventory(entity).is_some();

                !is_player && has_inventory
            }

            pub fn update_mouse_highlight(&mut self, mouse: Entity)
            {
                self.collider.iter().for_each(|(_, ComponentWrapper{
                    entity,
                    component: collider
                })|
                {
                    if let Some(mut render) = self.render_mut(*entity)
                    {
                        let overlapping = *collider.borrow().collided() == Some(mouse);

                        let outline = overlapping && self.is_lootable(*entity);
                        if outline
                        {
                            render.set_outlined(true);
                        } else
                        {
                            render.set_outlined(false);
                        }
                    }
                });
            }

            pub fn update_colliders(
                &mut self,
                passer: &mut impl EntityPasser
            )
            {
                self.collider.iter().for_each(|(_, ComponentWrapper{
                    component: collider,
                    ..
                })|
                {
                    collider.borrow_mut().reset_frame();
                });

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
                        let mut transform = self.target(*entity).unwrap();

                        let this = CollidingInfo{
                            physical: physical.as_deref_mut(),
                            transform: &mut transform,
                            collider: collider.borrow().clone()
                        };

                        let mut other_physical = self.physical_mut(*other_entity);
                        let mut other_transform = self.target(*other_entity).unwrap();
                        let collision = this.resolve(CollidingInfo{
                            physical: other_physical.as_deref_mut(),
                            transform: &mut other_transform,
                            collider: other_collider.borrow().clone()
                        });

                        if collision
                        {
                            collider.borrow_mut().set_collided(*other_entity);
                            other_collider.borrow_mut().set_collided(*entity);

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

            pub fn update_lazy_one(
                &self,
                entity: Entity,
                mut lazy: RefMut<LazyTransform>,
                dt: f32
            )
            {
                let target_global = self.parent_transform(entity);

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

            pub fn update_ui_aspect(
                &mut self,
                aspect: f32
            )
            {
                self.ui_element.iter().rev().for_each(|(_, ComponentWrapper{
                    entity,
                    component: ui_element
                })|
                {
                    if self.is_visible(*entity)
                    {
                        let mut target = self.target(*entity).unwrap();
                        let mut render = self.render_mut(*entity).unwrap();
                        ui_element.borrow_mut().update_aspect(&mut target, &mut render, aspect);
                    }
                });
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
                    if self.is_visible(*entity)
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
                    let mut render = self.render_mut(*entity).unwrap();
                    let mut transform = self.transform_mut(*entity).unwrap();
                    let changed = enemy.borrow_mut().update_sprite(
                        &mut *lazy,
                        enemies_info,
                        &mut render,
                        |render, texture|
                        {
                            render.set_sprite(create_info, Some(&mut transform), texture);
                        }
                    );

                    if changed
                    {
                        let parent_transform = self.parent_transform(*entity);
                        transform.scale = lazy.target_global(parent_transform.as_ref()).scale;
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
                let components = &components!(self, entity)[entity.id];

                EntityInfo{$(
                    $name: components[Component::$name as usize].map(|id|
                    {
                        self.$name[id].component.borrow().clone()
                    }),
                )+}
            }

            impl_common_systems!{}

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
                let entity = self.push(false, info);

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
                        debug_assert!(!entity.local);
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
    (ui_element, ui_element_mut, set_ui_element, SetUiElement, UiElementType, UiElementServer),
    (lazy_transform, lazy_transform_mut, set_lazy_transform, SetLazyTransform, LazyTransformType, LazyTransform),
    (follow_rotation, follow_rotation_mut, set_follow_rotation, SetFollowRotation, FollowRotationType, FollowRotation),
    (watchers, watchers_mut, set_watchers, SetWatchers, WatchersType, Watchers),
    (inventory, inventory_mut, set_inventory, SetInventory, InventoryType, Inventory),
    (enemy, enemy_mut, set_enemy, SetEnemy, EnemyType, Enemy),
    (named, named_mut, set_named, SetNamed, NamedType, String),
    (parent, parent_mut, set_parent, SetParent, ParentType, Parent),
    (transform, transform_mut, set_transform, SetTransform, TransformType, Transform),
    (player, player_mut, set_player, SetPlayer, PlayerType, Player),
    (collider, collider_mut, set_collider, SetCollider, ColliderType, Collider),
    (physical, physical_mut, set_physical, SetPhysical, PhysicalType, Physical),
    (anatomy, anatomy_mut, set_anatomy, SetAnatomy, AnatomyType, Anatomy)
}
