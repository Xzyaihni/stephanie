use std::{
    f32,
    mem,
    rc::Rc,
    fmt::Debug,
    cmp::Ordering,
    cell::{Ref, RefMut, RefCell}
};

use serde::{Serialize, Deserialize};

use nalgebra::{Vector2, Vector3, Unit};

use yanyaengine::{TextureId, Transform};

use crate::{
    server,
    client::{
        RenderCreateInfo,
        UiElement,
        UiEvent
    },
    common::{
        some_or_return,
        short_rotation,
        angle_between,
        ENTITY_SCALE,
        render_info::*,
        collider::*,
        watcher::*,
        lazy_transform::*,
        damaging::*,
        particle_creator::*,
        raycast::*,
        LazyMix,
        DataInfos,
        OccludingPlane,
        OccludingPlaneServer,
        Side2d,
        PhysicalProperties,
        Faction,
        DamagePartial,
        Damage,
        EntityPasser,
        Inventory,
        Anatomy,
        CharactersInfo,
        Character,
        Player,
        Enemy,
        Physical,
        ObjectsStore,
        Message,
        character::PartialCombinedInfo,
        world::World
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

macro_rules! component_index
{
    ($this:expr, $entity:expr, $component:ident) =>
    {
        components!($this, $entity).borrow().get($entity.id)
            .and_then(|components| components[Component::$component as usize])
    }
}

macro_rules! get_entity
{
    ($this:expr, $entity:expr, $access_type:ident, $component:ident) =>
    {
        component_index!($this, $entity, $component)
            .map(|id|
            {
                $this.$component.get(id).unwrap_or_else(||
                {
                    panic!("pointer to {} is out of bounds", stringify!($component))
                }).$access_type()
            })
    }
}

macro_rules! remove_component
{
    ($this:expr, $entity:expr, $component:ident) =>
    {
        let id = components!($this, $entity).borrow_mut()
            [$entity.id]
            [Component::$component as usize]
            .take();

        if let Some(id) = id
        {
            $this.$component.remove(id);
        }
    }
}

macro_rules! iterate_components_with
{
    ($this:expr, $component:ident, $iter_func:ident, $handler:expr) => 
    {
        $this.$component.iter().$iter_func(|(_, &ComponentWrapper{
            entity,
            component: ref component
        })|
        {
            $handler(entity, component)
        })
    }
}

macro_rules! for_each_component
{
    ($this:expr, $component:ident, $handler:expr) => 
    {
        iterate_components_with!($this, $component, for_each, $handler);
    }
}

pub trait ServerToClient<T>
{
    fn unchanged(self) -> Option<Self>
    where
        Self: Sized
    {
        None
    }

    fn server_to_client(
        self,
        transform: impl FnOnce() -> Transform,
        create_info: &mut RenderCreateInfo
    ) -> T;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Entity
{
    local: bool,
    id: usize
}

impl Entity
{
    pub fn from_raw(local: bool, id: usize) -> Entity
    {
        Self{local, id}
    }

    pub fn id(&self) -> usize
    {
        self.id
    }

    pub fn local(&self) -> bool
    {
        self.local
    }
}

pub trait OnSet<EntitiesType>: Sized + Debug
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
    LazyMix,
    LazyTransform,
    FollowRotation,
    FollowPosition,
    Inventory,
    String,
    Parent,
    Transform,
    Enemy,
    Player,
    Collider,
    Physical,
    Damaging,
    Watchers,
    OccludingPlane,
    UiElement,
    UiElementServer
}

no_on_set_for!{ServerEntities, Character}

impl OnSet<ClientEntities> for Character
{
    fn on_set(previous: Option<Self>, entities: &ClientEntities, entity: Entity)
    {
        if let Some(previous) = previous
        {
            entities.character_mut(entity).unwrap().with_previous(previous);
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

pub type Saveable = ();
pub type UiElementServer = ();

macro_rules! normal_forward_impl
{
    ($(($fn_ref:ident, $fn_mut:ident, $value:ident)),+,) =>
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

#[derive(Debug, Clone)]
pub struct ComponentWrapper<T>
{
    pub entity: Entity,
    pub component: RefCell<T>
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
    ($this_entity_info:ident, $(($name:ident, $set_func:ident)),+,) =>
    {
        fn push(
            &mut self,
            local: bool,
            mut info: $this_entity_info
        ) -> Entity
        {
            let entity = self.push_empty(local, info.parent.as_ref().map(|x| x.entity));

            info.setup_components(self, entity);

            self.set_each(entity, info);

            entity
        }

        fn handle_message_common(&mut self, message: Message) -> Option<Message>
        {
            match message
            {
                Message::EntityDamage{entity, faction, damage} =>
                {
                    self.damage_entity_common(entity, faction, damage);

                    None
                },
                Message::SetTarget{entity, target} =>
                {
                    if let Some(mut x) = self.target(entity)
                    {
                        *x = target;
                    }

                    None
                },
                Message::SetTargetPosition{entity, position} =>
                {
                    if let Some(mut x) = self.target(entity)
                    {
                        x.position = position;
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

        pub fn damage_entity_common(
            &self,
            entity: Entity,
            faction: Faction,
            damage: Damage
        ) -> bool
        {
            use crate::common::Damageable;

            if let Some(other) = self.faction(entity)
            {
                if !faction.aggressive(&other)
                {
                    return false;
                }
            } else
            {
                return false;
            }

            if self.anatomy(entity).is_some()
            {
                if let Some(mut mix_color) = self.mix_color_target(entity)
                {
                    *mix_color = Some(MixColor{color: [1.0; 3], amount: 0.8});
                }

                self.watchers_mut(entity).unwrap().push(
                    Watcher{
                        kind: WatcherType::Lifetime(0.2.into()),
                        action: WatcherAction::SetMixColor(None),
                        ..Default::default()
                    }
                );

                if let Some(mut anatomy) = self.anatomy_mut(entity)
                {
                    anatomy.damage(damage);
                }

                Anatomy::on_set(None, self, entity);

                return true;
            }

            false
        }

        pub fn faction(&self, entity: Entity) -> Option<Faction>
        {
            self.character(entity).map(|character|
            {
                character.faction
            })
        }

        pub fn update_lazy_mix(&mut self, dt: f32)
        {
            for_each_component!(self, lazy_mix, |entity, lazy_mix: &RefCell<LazyMix>|
            {
                if let Some(mut render) = self.render_mut(entity)
                {
                    let lazy_mix = lazy_mix.borrow();

                    render.mix = Some(if let Some(mix) = render.mix
                    {
                        lazy_mix.update(mix, dt)
                    } else
                    {
                        lazy_mix.target
                    });
                }
            });
        }

        fn create_queued_common(
            &mut self,
            mut f: impl FnMut(&mut Self, Entity, EntityInfo) -> $this_entity_info
        )
        {
            {
                let mut lazy_setter = self.lazy_setter.borrow_mut();
                if lazy_setter.changed
                {
                    lazy_setter.changed = false;

                    drop(lazy_setter);

                    $(
                        let queue = mem::take(&mut self.lazy_setter.borrow_mut().$name);
                        queue.into_iter().for_each(|(entity, component)|
                        {
                            self.$set_func(entity, component);
                        });
                    )+
                }
            }

            let queue = mem::take(self.create_queue.get_mut());
            queue.into_iter().for_each(|(entity, mut info)|
            {
                info.setup_components(self, entity);

                let info = f(self, entity, info);

                self.set_each(entity, info);
            });
        }

        pub fn lazy_target_end(&self, entity: Entity) -> Option<Transform>
        {
            self.lazy_transform(entity).map(|lazy|
            {
                let parent_transform = self.parent_transform(entity);

                lazy.target_global(parent_transform.as_ref())
            })
        }

        pub fn update_physical(&mut self, dt: f32)
        {
            for_each_component!(self, physical, |entity, physical: &RefCell<Physical>|
            {
                if let Some(mut target) = self.target(entity)
                {
                    let mut physical = physical.borrow_mut();
                    physical.physics_update(&mut target, dt);
                }
            });
        }

        pub fn update_follows(&mut self, dt: f32)
        {
            for_each_component!(self, follow_rotation, |entity, follow_rotation: &RefCell<FollowRotation>|
            {
                let mut follow_rotation = follow_rotation.borrow_mut();

                let mut transform = self.transform_mut(entity).unwrap();
                let current = &mut transform.rotation;
                let target = self.transform(follow_rotation.parent()).unwrap().rotation;

                follow_rotation.next(current, target, dt);
            });

            for_each_component!(self, follow_position, |entity, follow_position: &RefCell<FollowPosition>|
            {
                let mut follow_position = follow_position.borrow_mut();

                let mut transform = self.transform_mut(entity).unwrap();
                let target = self.transform(follow_position.parent()).unwrap().position;

                follow_position.next(&mut transform, target, dt);
            });
        }
    }
}

macro_rules! entity_info_common
{
    () =>
    {
        pub fn setup_components(
            &mut self,
            entities: &mut impl AnyEntities,
            entity: Entity
        )
        {
            if let Some(lazy) = self.lazy_transform.as_ref()
            {
                let parent_transform = self.parent.as_ref()
                    .and_then(|x|
                    {
                        entities.transform(x.entity).as_deref().cloned()
                    });

                let new_transform = lazy.target_global(parent_transform.as_ref());
                self.transform = Some(new_transform);
            }

            if let Some(follow_rotation) = self.follow_rotation.as_ref()
            {
                let transform = self.transform.as_mut().unwrap();

                let current = &mut transform.rotation;

                if let Some(parent_transform) = entities.transform(follow_rotation.parent())
                {
                    let target = parent_transform.rotation;

                    *current = target;
                }
            }

            if let Some(follow_position) = self.follow_position.as_ref()
            {
                let transform = self.transform.as_mut().unwrap();

                let current = &mut transform.position;

                if let Some(parent_transform) = entities.transform(follow_position.parent())
                {
                    let target = parent_transform.position;

                    *current = target;
                }
            }

            if self.anatomy.is_some() && self.watchers.is_none()
            {
                self.watchers = Some(Default::default());
            }

            if self.ui_element.is_some() && self.lazy_mix.is_none()
            {
                self.lazy_mix = Some(LazyMix::ui());
            }

            if let Some(lazy_mix) = self.lazy_mix.as_ref()
            {
                if let Some(render) = self.render.as_mut()
                {
                    render.mix = Some(lazy_mix.target);
                }
            }

            if let Some(character) = self.character.as_mut()
            {
                character.initialize(&entities.infos().characters_info, entity, |info|
                {
                    entities.push(entity.local(), info)
                });
            }
        }
    }
}

macro_rules! common_trait_impl
{
    ($(($fn_ref:ident, $fn_mut:ident, $value_type:ident)),+,) =>
    {
        normal_forward_impl!{
            $(($fn_ref, $fn_mut, $value_type),)+
        }

        fn infos(&self) -> &DataInfos
        {
            &self.infos
        }

        fn exists(&self, entity: Entity) -> bool
        {
            Self::exists(self, entity)
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

        fn mix_color_target(&self, entity: Entity) -> Option<RefMut<Option<MixColor>>>
        {
            self.render_mut(entity).map(|render|
            {
                RefMut::map(render, |x| &mut x.mix)
            })
        }

        fn remove(&mut self, entity: Entity)
        {
            Self::remove(self, entity);
        }

        fn check_guarantees(&mut self)
        {
            let for_components = |components: &RefCell<ObjectsStore<ComponentsIndices>>, local|
            {
                let components = components.borrow();

                components.iter().for_each(|(id, indices)|
                {
                    let entity = Entity{local, id};

                    if let Some(parent_component_id) = indices[Component::parent as usize]
                    {
                        let parent = self.parent[parent_component_id].component
                            .borrow()
                            .entity();

                        if let Some((parent_id, child_id)) = component_index!(
                            self,
                            parent,
                            lazy_transform
                        ).and_then(|parent|
                        {
                            component_index!(
                                self,
                                entity,
                                lazy_transform
                            ).map(|child| (parent, child))
                        })
                        {
                            assert!(
                                parent_id < child_id,
                                "({} ({parent:?}) < {} ({entity:?})), parent: {}, child: {}",
                                parent_id,
                                child_id,
                                self.info_ref(parent).unwrap_or_else(String::new),
                                self.info_ref(entity).unwrap_or_else(String::new)
                            );
                        }
                    }
                });
            };

            for_components(&self.components, false);
            for_components(&self.local_components, true);
        }
    }
}

macro_rules! order_sensitives
{
    ($(($name:ident, $resort_name:ident)),+) =>
    {
        fn order_sensitive(component: Component) -> bool
        {
            match component
            {
                $(
                    Component::$name => true,
                )+
                _ => false
            }
        }

        fn resort_all(&mut self, parent_entity: Entity)
        {
            $(
                self.$resort_name(parent_entity);
            )+
        }
    }
}

macro_rules! define_entities_both
{
    ($(($name:ident,
        $mut_func:ident,
        $set_func:ident,
        $on_name:ident,
        $resort_name:ident,
        $message_name:ident,
        $component_type:ident,
        $default_type:ident
    )),+,) =>
    {
        #[allow(non_camel_case_types)]
        #[derive(PartialEq, Eq)]
        pub enum Component
        {
            $($name,)+
        }

        const fn count_components() -> usize
        {
            0 $(+ {let _ = Component::$name; 1})+
        }

        pub const COMPONENTS_COUNT: usize = count_components();

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

        impl ClientEntityInfo
        {
            entity_info_common!{}
        }

        impl EntityInfo
        {
            entity_info_common!{}
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

        pub type OnComponentChange = Box<dyn FnMut(&mut ClientEntities, Entity)>;

        pub type ComponentsIndices = [Option<usize>; COMPONENTS_COUNT];

        #[derive(Debug, Default)]
        struct ChangedEntities
        {
            $($name: Vec<Entity>,)+
        }

        #[derive(Debug)]
        pub struct SetterQueue<$($component_type,)+>
        {
            changed: bool,
            $($name: Vec<(Entity, Option<$component_type>)>,)+
        }

        impl<$($component_type,)+> SetterQueue<$($component_type,)+>
        {
            $(
                pub fn $set_func(&mut self, entity: Entity, component: Option<$component_type>)
                {
                    self.changed = true;
                    self.$name.push((entity, component));
                }
            )+
        }

        impl<$($component_type,)+> Default for SetterQueue<$($component_type,)+>
        {
            fn default() -> Self
            {
                Self{
                    changed: false,
                    $($name: Vec::new(),)+
                }
            }
        }

        pub struct Entities<$($component_type=$default_type,)+>
        {
            pub local_components: RefCell<ObjectsStore<ComponentsIndices>>,
            pub components: RefCell<ObjectsStore<ComponentsIndices>>,
            pub lazy_setter: RefCell<SetterQueue<$($component_type,)+>>,
            infos: DataInfos,
            create_queue: RefCell<Vec<(Entity, EntityInfo)>>,
            create_render_queue: RefCell<Vec<(Entity, RenderComponent)>>,
            changed_entities: RefCell<ChangedEntities>,
            $($on_name: Rc<RefCell<Vec<OnComponentChange>>>,)+
            $(pub $name: ObjectsStore<ComponentWrapper<$component_type>>,)+
        }

        impl<$($component_type: OnSet<Self>,)+> Entities<$($component_type,)+>
        where
            Self: AnyEntities,
            for<'a> &'a ParentType: Into<&'a Parent>
        {
            pub fn new(infos: DataInfos) -> Self
            {
                Self{
                    local_components: RefCell::new(ObjectsStore::new()),
                    components: RefCell::new(ObjectsStore::new()),
                    lazy_setter: RefCell::new(Default::default()),
                    infos,
                    create_queue: RefCell::new(Vec::new()),
                    create_render_queue: RefCell::new(Vec::new()),
                    changed_entities: RefCell::new(Default::default()),
                    $($on_name: Rc::new(RefCell::new(Vec::new())),)+
                    $($name: ObjectsStore::new(),)+
                }
            }

            pub fn exists(&self, entity: Entity) -> bool
            {
                components!(self, entity).borrow().get(entity.id).is_some()
            }

            pub fn try_for_each_entity<E>(
                &self,
                mut f: impl FnMut(Entity) -> Result<(), E>
            ) -> Result<(), E>
            {
                self.components.borrow().iter()
                    .map(|(id, _)| Entity{local: false, id})
                    .try_for_each(|entity|
                    {
                        f(entity)
                    })
            }

            pub fn info_ref(&self, entity: Entity) -> Option<String>
            {
                if !self.exists(entity)
                {
                    return None;
                }

                let components = &components!(self, entity).borrow()[entity.id];

                let info = EntityInfo{$(
                    $name: {
                        components[Component::$name as usize].map(|id|
                        {
                            self.$name[id].get()
                        })
                    },
                )+};

                Some(format!("{info:#?}"))
            }

            fn set_each(&mut self, entity: Entity, info: EntityInfo<$($component_type,)+>)
            {
                $(
                    if info.$name.is_some()
                    {
                        self.$set_func(entity, info.$name);
                    }
                )+
            }

            fn push_empty(&self, local: bool, parent_entity: Option<Entity>) -> Entity
            {
                let components = if local
                {
                    &self.local_components
                } else
                {
                    &self.components
                };

                let mut components = components.borrow_mut();

                let id = if let Some(parent) = parent_entity
                {
                    components.take_after_key(parent.id)
                } else
                {
                    components.take_vacant_key()
                };

                components.insert(id, Self::empty_components());

                Entity{local, id}
            }

            $(
                pub fn $name(&self, entity: Entity) -> Option<Ref<$component_type>>
                {
                    get_entity!(self, entity, get, $name)
                }

                pub fn $mut_func(&self, entity: Entity) -> Option<RefMut<$component_type>>
                {
                    self.changed_entities.borrow_mut().$name.push(entity);

                    get_entity!(self, entity, get_mut, $name)
                }

                pub fn $set_func(&mut self, entity: Entity, component: Option<$component_type>)
                {
                    self.changed_entities.get_mut().$name.push(entity);

                    let parent_order_sensitive = Self::order_sensitive(Component::$name);

                    if !self.exists(entity)
                    {
                        components!(self, entity)
                            .borrow_mut()
                            .insert(entity.id, Self::empty_components());
                    }

                    if let Some(component) = component
                    {
                        let previous = {
                            let parent = parent_order_sensitive.then(||
                            {
                                self.parent(entity).map(|x|
                                {
                                    (&*x).into().entity()
                                }).and_then(|parent_entity|
                                {
                                    component_index!(self, parent_entity, $name)
                                })
                            }).flatten();

                            let mut components = components!(self, entity).borrow_mut();

                            let slot = &mut components[entity.id][Component::$name as usize];

                            let component = ComponentWrapper{
                                entity,
                                component: RefCell::new(component)
                            };

                            if let Some(id) = slot
                            {
                                self.$name.insert(*id, component)
                            } else
                            {
                                let id = if let Some(id) = parent
                                {
                                    self.$name.push_after(id, component)
                                } else
                                {
                                    self.$name.push(component)
                                };
                                
                                *slot = Some(id);

                                drop(components);

                                if parent_order_sensitive
                                {
                                    self.$resort_name(entity);
                                } else if Component::$name == Component::parent
                                {
                                    let parent_entity = self.parent(entity).map(|x|
                                    {
                                        (&*x).into().entity()
                                    }).unwrap();

                                    self.resort_all(parent_entity);
                                }

                                None
                            }
                        };

                        $component_type::on_set(
                            previous.map(|x| x.component.into_inner()),
                            self,
                            entity
                        );
                    } else
                    {
                        remove_component!(self, entity, $name);
                    }
                }

                fn $resort_name(
                    &mut self,
                    parent_entity: Entity
                )
                {
                    let child = self.$name.iter().find_map(|(component_id, &ComponentWrapper{
                        entity,
                        ..
                    })|
                    {
                        self.parent(entity).and_then(|parent|
                        {
                            ((&*parent).into().entity() == parent_entity).then(||
                            {
                                (component_id, entity)
                            })
                        })
                    });

                    let (child_component, child) = some_or_return!(child);

                    let parent_component = some_or_return!(
                        component_index!(self, parent_entity, $name)
                    );

                    if parent_component < child_component
                    {
                        return;
                    }

                    // swap contents
                    self.$name.swap(child_component, parent_component);

                    self.swap_component_indices(Component::$name, child, parent_entity);

                    if let Some(entity) = self.parent(parent_entity)
                        .map(|parent| (&*parent).into().entity())
                    {
                        self.$resort_name(entity);
                    }

                    self.$resort_name(child);
                    self.$resort_name(parent_entity);
                }
            )+

            pub fn update_watchers(
                &mut self,
                dt: f32
            )
            where
                for<'a> &'a mut WatchersType: Into<&'a mut Watchers>
            {
                // the borrow checker forcing me to collect into vectors cuz why not!
                let pairs: Vec<_> = iterate_components_with!(self, watchers, map, |entity, watchers: &RefCell<WatchersType>|
                {
                    let actions = (&mut *watchers.borrow_mut()).into().execute(self, entity, dt);

                    (entity, actions)
                }).collect();

                pairs.into_iter().for_each(|(entity, actions)|
                {
                    actions.into_iter().for_each(|action|
                    {
                        action.execute(self, entity);
                    });
                });
            }

            pub fn set_deferred_render(&self, entity: Entity, render: RenderInfo)
            {
                self.create_render_queue.borrow_mut()
                    .push((entity, RenderComponent::Full(render)));
            }

            pub fn set_deferred_render_object(&self, entity: Entity, object: RenderObject)
            {
                self.create_render_queue.borrow_mut()
                    .push((entity, RenderComponent::Object(object)));
            }

            pub fn set_deferred_render_scissor(&self, entity: Entity, scissor: Scissor)
            {
                self.create_render_queue.borrow_mut()
                    .push((entity, RenderComponent::Scissor(scissor)));
            }

            fn swap_component_indices(&mut self, component: Component, a: Entity, b: Entity)
            {
                let component_id = component as usize;

                let components_a = components!(self, a);
                let mut components_a = components_a.borrow_mut();

                if a.local() == b.local()
                {
                    let b_i = components_a.get(b.id).unwrap()[component_id];

                    let a_i = &mut components_a.get_mut(a.id).unwrap()[component_id];
                    let temp = *a_i;

                    *a_i = b_i;

                    components_a.get_mut(b.id).unwrap()[component_id] = temp;
                } else
                {
                    let components_b = components!(self, b);
                    let mut components_b = components_b.borrow_mut();

                    let a = &mut components_a.get_mut(a.id).unwrap()[component_id];
                    let b = &mut components_b.get_mut(b.id).unwrap()[component_id];

                    mem::swap(a, b);
                }
            }

            pub fn remove(&mut self, entity: Entity)
            {
                if !self.exists(entity)
                {
                    return;
                }

                {
                    let components = &components!(self, entity).borrow()[entity.id];

                    $(if let Some(id) = components[Component::$name as usize]
                    {
                        self.$name.remove(id);
                    })+
                }

                let components = components!(self, entity);
                components.borrow_mut().remove(entity.id);

                self.remove_children(entity);
            }

            pub fn remove_children(&mut self, parent_entity: Entity)
            {
                let remove_list: Vec<_> = iterate_components_with!(self, parent, filter_map, |entity, parent: &RefCell<ParentType>|
                {
                    let parent = parent.borrow();

                    ((&*parent).into().entity() == parent_entity).then(||
                    {
                        entity
                    })
                }).collect();

                remove_list.into_iter().for_each(|entity|
                {
                    self.remove(entity);
                });
            }

            order_sensitives!(
                (lazy_transform, resort_lazy_transform),
                (follow_rotation, resort_follow_rotation),
                (follow_position, resort_follow_position),
                (parent, resort_parent)
            );

            fn empty_components() -> ComponentsIndices
            {
                [$(
                    {
                        let _ = Component::$name;
                        None
                    }
                ,)+]
            }
        }

        impl ClientEntities
        {
            fn transform_clone(&self, entity: Entity) -> Option<Transform>
            {
                self.transform(entity).as_deref().cloned()
            }

            pub fn push_client(
                &mut self,
                local: bool,
                info: ClientEntityInfo
            ) -> Entity
            {
                self.push(local, info)
            }

            impl_common_systems!{ClientEntityInfo, $(($name, $set_func),)+}

            $(
                pub fn $on_name(&self, f: OnComponentChange)
                {
                    self.$on_name.borrow_mut().push(f);
                }
            )+

            pub fn handle_on_change(&mut self)
            {
                $(
                    let changed_entities = self.changed_entities.get_mut();

                    let taken = mem::take(&mut changed_entities.$name);
                    taken.into_iter().for_each(|entity|
                    {
                        let listeners = self.$on_name.clone();

                        listeners.borrow_mut().iter_mut().for_each(|on_change|
                        {
                            on_change(self, entity);
                        });
                    });
                )+
            }

            pub fn damage_entity(
                &self,
                passer: &mut impl EntityPasser,
                blood_texture: TextureId,
                angle: f32,
                entity: Entity,
                faction: Faction,
                damage: DamagePartial
            )
            {
                let entity_rotation = if let Some(transform) = self.transform(entity)
                {
                    transform.rotation
                } else
                {
                    return;
                };

                let relative_rotation = angle - (-entity_rotation);
                let damage = damage.with_direction(Side2d::from_angle(relative_rotation));

                let damaged = self.damage_entity_common(entity, faction, damage.clone());

                if damaged
                {
                    let direction = Unit::new_normalize(
                        Vector3::new(-angle.cos(), angle.sin(), 0.0)
                    );

                    passer.send_message(Message::EntityDamage{entity, faction, damage});

                    let scale = Vector3::repeat(ENTITY_SCALE * 0.1)
                        .component_mul(&Vector3::new(4.0, 1.0, 1.0));

                    self.watchers_mut(entity).unwrap().push(Watcher{
                        kind: WatcherType::Instant,
                        action: WatcherAction::Explode(Box::new(ExplodeInfo{
                            keep: true,
                            info: ParticlesInfo{
                                amount: 2..4,
                                speed: ParticleSpeed::DirectionSpread{
                                    direction,
                                    speed: 1.7..=2.0,
                                    spread: 0.2
                                },
                                decay: ParticleDecay::Random(7.0..=10.0),
                                position: ParticlePosition::Spread(0.1),
                                rotation: ParticleRotation::Exact(f32::consts::PI - angle),
                                scale: ParticleScale::Spread{scale, variation: 0.1},
                                min_scale: ENTITY_SCALE * 0.15
                            },
                            prototype: EntityInfo{
                                physical: Some(PhysicalProperties{
                                    mass: 0.05,
                                    friction: 0.05,
                                    floating: true
                                }.into()),
                                render: Some(RenderInfo{
                                    object: Some(RenderObjectKind::TextureId{
                                        id: blood_texture
                                    }.into()),
                                    z_level: ZLevel::Knee,
                                    ..Default::default()
                                }),
                                ..Default::default()
                            }
                        })),
                        ..Default::default()
                    });
                }
            }

            fn raycast_entity(
                start: &Vector3<f32>,
                direction: &Unit<Vector3<f32>>,
                transform: &Transform
            ) -> Option<RaycastResult>
            {
                let radius = transform.max_scale() / 2.0;

                let position = transform.position;

                let offset = start - position;

                let left = direction.dot(&offset).powi(2);
                let right = offset.magnitude_squared() - radius.powi(2);

                // math ppl keep making fake letters
                let nabla = left - right;

                if nabla < 0.0
                {
                    None
                } else
                {
                    let sqrt_nabla = nabla.sqrt();
                    let left = -(direction.dot(&offset));

                    let first = left - sqrt_nabla;
                    let second = left + sqrt_nabla;

                    let close = first.min(second);
                    let far = first.max(second);

                    let pierce = far - close;

                    Some(RaycastResult{distance: close, pierce})
                }
            }

            pub fn raycast(
                &self,
                info: RaycastInfo,
                start: &Vector3<f32>,
                end: &Vector3<f32>
            ) -> RaycastHits
            {
                let direction = end - start;

                let max_distance = direction.magnitude();
                let direction = Unit::new_normalize(direction);

                let mut hits: Vec<_> = iterate_components_with!(
                    self,
                    collider,
                    filter_map,
                    |entity, collider: &RefCell<Collider>|
                    {
                        let collides = collider.borrow().layer.collides(&info.layer);

                        (collides && !collider.borrow().ghost).then_some(entity)
                    })
                    .filter_map(|entity|
                    {
                        let transform = self.transform(entity);

                        transform.and_then(|transform|
                        {
                            if let Some(ignore_entity) = info.ignore_entity
                            {
                                (entity != ignore_entity).then_some((entity, transform))
                            } else
                            {
                                Some((entity, transform))
                            }
                        })
                    })
                    .filter_map(|(entity, transform)|
                    {
                        Self::raycast_entity(start, &direction, &transform).and_then(|hit|
                        {
                            let backwards = (hit.distance + hit.pierce) < 0.0;
                            let past_end = (hit.distance > max_distance) && !info.ignore_end;

                            if backwards || past_end
                            {
                                None
                            } else
                            {
                                let id = RaycastHitId::Entity(entity);
                                Some(RaycastHit{id, distance: hit.distance, width: hit.pierce})
                            }
                        })
                    })
                    .collect();

                hits.sort_unstable_by(|a, b|
                {
                    a.distance.partial_cmp(&b.distance).unwrap_or(Ordering::Equal)
                });

                let hits = if let Some(mut pierce) = info.pierce
                {
                    hits.into_iter().take_while(|x|
                    {
                        if pierce > 0.0
                        {
                            pierce -= x.width;

                            true
                        } else
                        {
                            false
                        }
                    }).collect()
                } else
                {
                    let first = hits.into_iter().next();

                    first.map(|x| vec![x]).unwrap_or_default()
                };

                RaycastHits{start: *start, direction, hits}
            }

            pub fn create_queued(
                &mut self,
                create_info: &mut RenderCreateInfo
            )
            {
                self.create_queued_common(|this, entity, info|
                {
                    ClientEntityInfo::from_server(
                        this,
                        entity,
                        create_info,
                        info
                    )
                });

                let render_queue = {
                    let mut render_queue = self.create_render_queue.borrow_mut();

                    mem::take(&mut *render_queue)
                };

                render_queue.into_iter().for_each(|(entity, render)|
                {
                    let transform = ||
                    {
                        self.transform_clone(entity).unwrap_or_else(||
                        {
                            panic!("deferred render expected transform, got none")
                        })
                    };

                    match render
                    {
                        RenderComponent::Full(render) =>
                        {
                            let render = render.server_to_client(
                                transform,
                                create_info
                            );

                            self.set_render(entity, Some(render));
                        },
                        RenderComponent::Object(object) =>
                        {
                            if let Some(mut render) = self.render_mut(entity)
                            {
                                let object = object.into_client(transform(), create_info);

                                render.object = object;
                            }
                        },
                        RenderComponent::Scissor(scissor) =>
                        {
                            if let Some(mut render) = self.render_mut(entity)
                            {
                                let size = create_info.object_info.partial.size;
                                let scissor = scissor.into_global(size);

                                render.scissor = Some(scissor);
                            }
                        }
                    }
                });
            }

            pub fn update_damaging(
                &mut self,
                passer: &mut impl EntityPasser,
                blood_texture: TextureId
            )
            {
                struct DamagingResult
                {
                    collided: Entity,
                    angle: f32,
                    faction: Faction,
                    damage: DamagePartial
                }

                // "zero" "cost" "abstractions" "borrow" "checker"
                let damage_entities = iterate_components_with!(self, damaging, flat_map, |entity, damaging: &RefCell<Damaging>|
                {
                    let collider = self.collider(entity).unwrap();

                    collider.collided().iter().copied().filter_map(|collided|
                    {
                        let mut damaging = damaging.borrow_mut();

                        let parent_angle_between = ||
                        {
                            let parent = self.parent(entity).unwrap().entity;

                            let parent_transform = self.transform(parent).unwrap();
                            let collided_transform = self.transform(collided).unwrap();

                            let angle = angle_between(
                                parent_transform.position,
                                collided_transform.position
                            );

                            let parent_angle = -parent_transform.rotation;
                            let relative_angle = angle + (f32::consts::PI - parent_angle);

                            short_rotation(relative_angle)
                        };

                        if damaging.can_damage(collided)
                            && damaging.predicate.meets(parent_angle_between)
                        {
                            damaging.damaged(collided);

                            let collision_info = || -> Option<_>
                            {
                                let source_entity = if let Some(other) = damaging.source
                                {
                                    other
                                } else
                                {
                                    entity
                                };

                                let this_transform = self.transform(source_entity)?;
                                let collided_transform = self.transform(collided)?;

                                let this_physical = self.physical(entity);
                                let collided_physical = self.physical(collided);

                                Some(CollisionInfo::new(
                                    &this_transform,
                                    &collided_transform,
                                    this_physical.as_deref(),
                                    collided_physical.as_deref()
                                ))
                            };

                            return damaging.damage.as_damage(collision_info).map(|(angle, damage)|
                            {
                                DamagingResult{collided, angle, faction: damaging.faction, damage}
                            });
                        }

                        None
                    }).collect::<Vec<_>>()
                }).collect::<Vec<_>>();

                damage_entities.into_iter().for_each(|DamagingResult{
                    collided,
                    angle,
                    faction,
                    damage
                }|
                {
                    self.damage_entity(passer, blood_texture, angle, collided, faction, damage);
                });
            }

            pub fn update_children(&mut self)
            {
                for_each_component!(self, parent, |entity, parent: &RefCell<Parent>|
                {
                    let parent = parent.borrow();

                    if let Some(mut render) = self.render_mut(entity)
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
                for_each_component!(self, render, |entity, render: &RefCell<ClientRenderInfo>|
                {
                    let transform = self.transform(entity).unwrap();

                    if let Some(object) = render.borrow_mut().object.as_mut()
                    {
                        object.set_transform(transform.clone());
                    }
                });

                for_each_component!(self, occluding_plane, |entity, occluding_plane: &RefCell<OccludingPlane>|
                {
                    let transform = self.transform(entity).unwrap();

                    occluding_plane.borrow_mut().set_transform(transform.clone());
                });
            }

            pub fn is_lootable(&self, entity: Entity) -> bool
            {
                let is_player = self.player(entity).is_some();
                let has_inventory = self.inventory(entity).is_some();

                let maybe_anatomy = if let Some(anatomy) = self.anatomy(entity)
                {
                    anatomy.speed().is_none()
                } else
                {
                    true
                };

                !is_player && has_inventory && maybe_anatomy
            }

            pub fn within_interactable_distance(&self, a: Entity, b: Entity) -> bool
            {
                let interactable_distance = 0.5;

                let a = if let Some(x) = self.transform(a)
                {
                    x.position
                } else
                {
                    return false;
                };

                let b = if let Some(x) = self.transform(b)
                {
                    x.position
                } else
                {
                    return false;
                };

                a.metric_distance(&b) <= interactable_distance
            }

            pub fn update_mouse_highlight(&mut self, player: Entity, mouse: Entity)
            {
                let mouse_collider = self.collider(mouse).unwrap();
                let mouse_collided = mouse_collider.collided().first().copied();

                // i thought about doing it with watchers or something but
                // that would create so many of them and this thing only runs once
                // per frame so i dunno if its worth?
                let unoutline_all = ||
                {
                    for_each_component!(self, collider, |entity, _collider|
                    {
                        if let Some(mut render) = self.render_mut(entity)
                        {
                            render.set_outlined(false);
                        }
                    });
                };

                let mouse_collided = if let Some(x) = mouse_collided
                {
                    x
                } else
                {
                    unoutline_all();
                    return;
                };

                if !self.within_interactable_distance(player, mouse_collided)
                {
                    unoutline_all();
                    return;
                }

                for_each_component!(self, collider, |entity, _collider|
                {
                    if let Some(mut render) = self.render_mut(entity)
                    {
                        let overlapping = mouse_collided == entity;

                        let outline = overlapping && self.is_lootable(entity);
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
                world: &World,
                mut passer: Option<&mut impl EntityPasser>
            )
            {
                macro_rules! colliding_info
                {
                    ($result_variable:expr, $physical:expr, $collider:expr, $entity:expr) =>
                    {
                        let mut collider: RefMut<Collider> = $collider.borrow_mut();
                        {
                            let transform = self.transform($entity).unwrap().clone();

                            $result_variable = CollidingInfo{
                                entity: Some($entity),
                                physical: $physical.as_deref_mut(),
                                transform,
                                target: |mut offset: Vector3<f32>|
                                {
                                    let mut target = self.target($entity).unwrap();

                                    if let Some(parent) = self.parent($entity)
                                    {
                                        let parent_scale = self.transform(parent.entity)
                                            .unwrap()
                                            .scale;

                                        offset = offset.component_div(&parent_scale);
                                    }

                                    target.position += offset;

                                    target.position
                                },
                                collider: &mut collider
                            };
                        }
                    }
                }

                let mut on_collision = |
                    entity,
                    physical: Option<RefMut<Physical>>
                |
                {
                    if let Some(passer) = passer.as_mut()
                    {
                        passer.send_message(Message::SetTargetPosition{
                            entity,
                            position: self.target_ref(entity).unwrap().position
                        });

                        if let Some(physical) = physical
                        {
                            passer.send_message(Message::SetPhysical{
                                entity,
                                component: physical.clone()
                            });
                        }
                    }
                };

                for_each_component!(self, collider, |_, collider: &RefCell<Collider>|
                {
                    collider.borrow_mut().reset_frame();
                });

                let mut pairs_fn = |&ComponentWrapper{
                    entity,
                    component: ref collider
                }, &ComponentWrapper{
                    entity: other_entity,
                    component: ref other_collider
                }|
                {
                    let mut physical = self.physical_mut(entity);
                    let mut this;
                    colliding_info!{this, physical, collider, entity};

                    let mut other_physical = self.physical_mut(other_entity);
                    let other;
                    colliding_info!{other, other_physical, other_collider, other_entity};

                    if this.resolve(other)
                    {
                        on_collision(entity, physical);
                        on_collision(other_entity, other_physical);
                    }
                };

                {
                    let mut colliders = self.collider.iter().map(|(_, x)| x);

                    // calls the function for each unique combination (excluding (self, self) pairs)
                    colliders.clone().for_each(|a|
                    {
                        colliders.by_ref().next();
                        colliders.clone().for_each(|b| pairs_fn(a, b));
                    });
                }

                for_each_component!(self, collider, |entity, collider: &RefCell<_>|
                {
                    let mut physical = self.physical_mut(entity);
                    let mut this;
                    colliding_info!{this, physical, collider, entity};

                    if this.resolve_with_world(world)
                    {
                        on_collision(entity, physical);
                    }
                });

                self.update_colliders_previous();
            }

            fn update_colliders_previous(&mut self)
            {
                for_each_component!(self, collider, |entity, collider: &RefCell<Collider>|
                {
                    if let Some(transform) = self.transform(entity)
                    {
                        collider.borrow_mut().save_previous(transform.position);
                    }
                });
            }

            pub fn update_lazy_one(
                &self,
                entity: Entity,
                mut lazy: RefMut<LazyTransform>,
                dt: f32
            )
            {
                if let Some(mut transform) = self.transform_mut(entity)
                {
                    let target_global = self.parent_transform(entity);

                    *transform = lazy.next(
                        self.physical(entity).as_deref(),
                        transform.clone(),
                        target_global,
                        dt
                    );
                }
            }

            pub fn update_lazy(&mut self, dt: f32)
            {
                for_each_component!(self, lazy_transform, |entity, lazy: &RefCell<LazyTransform>|
                {
                    self.update_lazy_one(entity, lazy.borrow_mut(), dt);
                });
            }

            pub fn update_enemy(&mut self, passer: &mut impl EntityPasser, dt: f32)
            {
                let mut on_state_change = |entity|
                {
                    let enemy = self.enemy(entity).unwrap().clone();
                    let target = self.target_ref(entity).unwrap().clone();

                    passer.send_message(Message::SetEnemy{
                        entity,
                        component: enemy
                    });

                    passer.send_message(Message::SetTarget{
                        entity,
                        target
                    });
                };

                for_each_component!(self, enemy, |entity, enemy: &RefCell<Enemy>|
                {
                    if enemy.borrow().check_hostiles()
                    {
                        let character = self.character_mut(entity).unwrap();
                        self.character.iter()
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

                                let anatomy = self.anatomy(entity).unwrap();

                                let transform = self.transform(entity).unwrap();
                                let other_transform = self.transform(other_entity).unwrap();

                                anatomy.sees(&transform.position, &other_transform.position)
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
                        self,
                        entity,
                        dt
                    );

                    if state_changed
                    {
                        on_state_change(entity);
                    }
                });
            }

            pub fn update_ui_aspect(
                &mut self,
                aspect: f32
            )
            {
                iterate_components_with!(self, ui_element, filter_map, |entity, ui_element|
                {
                    self.is_visible(entity).then(|| (entity, ui_element))
                }).rev().for_each(|(entity, ui_element): (_, &RefCell<UiElement>)|
                {
                    let mut target = self.target(entity).unwrap();
                    let mut render = self.render_mut(entity).unwrap();
                    ui_element.borrow_mut().update_aspect(&mut target, &mut render, aspect);
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
                iterate_components_with!(self, ui_element, filter_map, |entity, ui_element|
                {
                    self.is_visible(entity).then(|| (entity, ui_element))
                }).rev().for_each(|(entity, ui_element): (_, &RefCell<UiElement>)|
                {
                    captured = ui_element.borrow_mut().update(
                        &*self,
                        entity,
                        camera_position,
                        &event,
                        captured
                    ) || captured;
                });

                captured
            }

            pub fn update_characters(
                &mut self,
                partial: PartialCombinedInfo,
                create_info: &mut RenderCreateInfo,
                dt: f32
            )
            {
                let assets = create_info.object_info.partial.assets.clone();
                for_each_component!(self, character, |entity, character: &RefCell<Character>|
                {
                    let changed = {
                        let combined_info = partial.to_full(
                            self,
                            &assets
                        );

                        character.borrow_mut().update(
                            combined_info,
                            entity,
                            dt,
                            |texture|
                            {
                                let mut render = self.render_mut(entity).unwrap();
                                let transform = self.target_ref(entity).unwrap();

                                render.set_sprite(create_info, Some(&transform), texture);
                            }
                        )
                    };

                    if changed
                    {
                        if let Some(end) = self.lazy_target_end(entity)
                        {
                            let mut transform = self.transform_mut(entity).unwrap();

                            transform.scale = end.scale;
                        }
                    }
                });
            }

            pub fn anatomy_changed(&self, entity: Entity)
            {
                if let Some(mut character) = self.character_mut(entity)
                {
                    let anatomy = self.anatomy(entity).unwrap();

                    character.anatomy_changed(&anatomy);
                }
            }
        }

        impl ServerEntities
        {
            pub fn info(&self, entity: Entity) -> EntityInfo
            {
                let components = &components!(self, entity).borrow()[entity.id];

                EntityInfo{$(
                    $name: components[Component::$name as usize].map(|id|
                    {
                        self.$name[id].get().clone()
                    }),
                )+}
            }

            impl_common_systems!{EntityInfo, $(($name, $set_func),)+}

            pub fn update_lazy(&mut self)
            {
                for_each_component!(self, lazy_transform, |entity, _lazy_transform|
                {
                    if let (
                        Some(end), Some(mut transform)
                    ) = (self.lazy_target_end(entity), self.transform_mut(entity))
                    {
                        *transform = end;
                    }
                });
            }

            pub fn update_sprites(
                &mut self,
                characters_info: &CharactersInfo
            )
            {
                for_each_component!(self, character, |entity, character: &RefCell<Character>|
                {
                    let mut target = self.target(entity).unwrap();

                    let changed = character.borrow_mut()
                        .update_common(characters_info, &mut target);

                    if changed
                    {
                        drop(target);
                        if let Some(end) = self.lazy_target_end(entity)
                        {
                            let mut transform = self.transform_mut(entity).unwrap();

                            transform.scale = end.scale;
                        }
                    }
                });
            }

            pub fn create_queued(
                &mut self,
                writer: &mut server::ConnectionsHandler
            )
            {
                self.create_queued_common(|_this, entity, info|
                {
                    let message = Message::EntitySet{entity, info: info.clone()};

                    writer.send_message(message);

                    info
                });

                self.create_render_queue.borrow_mut().clear();
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

                #[allow(unreachable_patterns)]
                match message
                {
                    $(Message::$message_name{entity, component} =>
                    {
                        debug_assert!(!entity.local);
                        self.$set_func(entity, Some(component));

                        None
                    },)+
                    x => Some(x)
                }
            }
        }
    }
}

macro_rules! define_entities
{
    ((side_specific
        $(($side_name:ident,
            $side_mut_func:ident,
            $side_set_func:ident,
            $side_on_name:ident,
            $side_resort_name:ident,
            $side_message_name:ident,
            $side_component_type:ident,
            $side_default_type:ident,
            $client_type:ident
        )),+),
        $(($name:ident,
            $mut_func:ident,
            $set_func:ident,
            $on_name:ident,
            $resort_name:ident,
            $message_name:ident,
            $component_type:ident,
            $default_type:ident
        )),+
    ) =>
    {
        define_entities_both!{
            $(($side_name, $side_mut_func, $side_set_func, $side_on_name, $side_resort_name, $side_message_name, $side_component_type, $side_default_type),)+
            $(($name, $mut_func, $set_func, $on_name, $resort_name, $message_name, $component_type, $default_type),)+
        }

        impl AnyEntities for ClientEntities
        {
            common_trait_impl!{$(($name, $mut_func, $default_type),)+}

            fn push_eager(
                &mut self,
                local: bool,
                mut info: EntityInfo
            ) -> Entity
            {
                // clients cant create global entities
                assert!(local);

                let entity = self.push(local, info.shared());

                info.setup_components(self, entity);

                self.create_queue.borrow_mut().push((entity, info));

                entity
            }

            fn push(&self, local: bool, info: EntityInfo) -> Entity
            {
                // clients cant create global entities
                assert!(local);

                let entity = self.push_empty(local, info.parent.as_ref().map(|x| x.entity));

                self.create_queue.borrow_mut().push((entity, info));

                entity
            }
        }

        impl AnyEntities for ServerEntities
        {
            common_trait_impl!{$(($name, $mut_func, $default_type),)+}

            fn push_eager(&mut self, local: bool, info: EntityInfo) -> Entity
            {
                Self::push(self, local, info)
            }

            fn push(&self, local: bool, info: EntityInfo) -> Entity
            {
                let entity = self.push_empty(local, info.parent.as_ref().map(|x| x.entity));

                self.create_queue.borrow_mut().push((entity, info));

                entity
            }
        }

        pub trait AnyEntities
        {
            $(
                fn $name(&self, entity: Entity) -> Option<Ref<$default_type>>;
                fn $mut_func(&self, entity: Entity) -> Option<RefMut<$default_type>>;
            )+

            fn infos(&self) -> &DataInfos;

            fn lazy_target_ref(&self, entity: Entity) -> Option<Ref<Transform>>;
            fn lazy_target(&self, entity: Entity) -> Option<RefMut<Transform>>;

            fn is_visible(&self, entity: Entity) -> bool;
            fn visible_target(&self, entity: Entity) -> Option<RefMut<bool>>;
            fn mix_color_target(&self, entity: Entity) -> Option<RefMut<Option<MixColor>>>;

            fn exists(&self, entity: Entity) -> bool;

            fn remove(&mut self, entity: Entity);

            fn push_eager(
                &mut self,
                local: bool,
                info: EntityInfo
            ) -> Entity;

            fn push(
                &self,
                local: bool,
                info: EntityInfo
            ) -> Entity;

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

            fn check_guarantees(&mut self);
        }

        pub type ClientEntityInfo = EntityInfo<$($client_type,)+>;
        pub type ClientEntities = Entities<$($client_type,)+>;
        pub type ServerEntities = Entities;

        impl ClientEntityInfo
        {
            pub fn from_server(
                entities: &ClientEntities,
                entity: Entity,
                create_info: &mut RenderCreateInfo,
                info: EntityInfo
            ) -> Self
            {
                let transform = info.target_ref().cloned().or_else(||
                {
                    entities.transform_clone(entity)
                });

                Self{
                    $($side_name: info.$side_name.map(|x|
                    {
                        x.server_to_client(||
                        {
                            transform.clone().unwrap_or_else(||
                            {
                                panic!("{} expected transform, got none", stringify!($side_name))
                            })
                        }, create_info)
                    }),)+
                    $($name: info.$name,)+
                }
            }
        }

        impl EntityInfo
        {
            pub fn shared(&mut self) -> ClientEntityInfo
            {
                ClientEntityInfo{
                    $($side_name: None,)+
                    $($name: self.$name.take(),)+
                }
            }
        }

        impl ClientEntities
        {
            pub fn handle_message(
                &mut self,
                create_info: &mut RenderCreateInfo,
                message: Message
            ) -> Option<Message>
            {
                let message = self.handle_message_common(message)?;

                #[allow(unreachable_patterns)]
                match message
                {
                    Message::EntitySet{entity, info} =>
                    {
                        let transform = info.transform.clone()
                            .or_else(||self.transform_clone(entity));

                        $({
                            let component = info.$side_name.map(|x|
                            {
                                x.server_to_client(||
                                {
                                    transform.clone().unwrap_or_else(||
                                    {
                                        panic!(
                                            "{} expected transform, got none",
                                            stringify!($side_name)
                                        )
                                    })
                                }, create_info)
                            });

                            if component.is_some()
                            {
                                self.$side_set_func(entity, component);
                            }
                        })+

                        $(
                            if info.$name.is_some()
                            {
                                self.$set_func(entity, info.$name);
                            }
                        )+

                        debug_assert!(!entity.local);

                        if let (
                            Some(end),
                            Some(mut transform)
                        ) = (self.lazy_target_end(entity), self.transform_mut(entity))
                        {
                            *transform = end;
                        }

                        None
                    },
                    $(Message::$side_message_name{entity, component} =>
                    {
                        debug_assert!(!entity.local);
                        let component = component.server_to_client(||
                        {
                            self.transform_clone(entity).unwrap_or_else(||
                            {
                                panic!(
                                    "{} expected transform, got none",
                                    stringify!($side_message_name)
                                )
                            })
                        }, create_info);

                        self.$side_set_func(entity, Some(component));

                        None
                    },)+
                    $(Message::$message_name{entity, component} =>
                    {
                        debug_assert!(!entity.local);
                        self.$set_func(entity, Some(component));

                        None
                    },)+
                    x => Some(x)
                }
            }
        }
    }
}

// macros still cant be used in ident positions :)
// this is not pain :)
// im okay :)
define_entities!{
    (side_specific
        (render, render_mut, set_render, on_render, resort_render, SetRender, RenderType, RenderInfo, ClientRenderInfo),
        (occluding_plane, occluding_plane_mut, set_occluding_plane, on_plane_mut, resort_occluding_plane, SetNone, OccludingPlaneType, OccludingPlaneServer, OccludingPlane),
        (ui_element, ui_element_mut, set_ui_element, on_ui_element, resort_ui_element, SetNone, UiElementType, UiElementServer, UiElement)),
    (parent, parent_mut, set_parent, on_parent, resort_parent, SetParent, ParentType, Parent),
    (lazy_mix, lazy_mix_mut, set_lazy_mix, on_lazy_mix, resort_lazy_mix, SetLazyMix, LazyMixType, LazyMix),
    (lazy_transform, lazy_transform_mut, set_lazy_transform, on_lazy_transform, resort_lazy_transform, SetLazyTransform, LazyTransformType, LazyTransform),
    (follow_rotation, follow_rotation_mut, set_follow_rotation, on_follow_rotation, resort_follow_rotation, SetFollowRotation, FollowRotationType, FollowRotation),
    (follow_position, follow_position_mut, set_follow_position, on_follow_position, resort_follow_position, SetFollowPosition, FollowPositionType, FollowPosition),
    (watchers, watchers_mut, set_watchers, on_watchers, resort_watchers, SetWatchers, WatchersType, Watchers),
    (damaging, damaging_mut, set_damaging, on_damaging, resort_damaging, SetDamaging, DamagingType, Damaging),
    (inventory, inventory_mut, set_inventory, on_inventory, resort_inventory, SetInventory, InventoryType, Inventory),
    (named, named_mut, set_named, on_named, resort_named, SetNamed, NamedType, String),
    (transform, transform_mut, set_transform, on_transform, resort_transform, SetTransform, TransformType, Transform),
    (character, character_mut, set_character, on_character, resort_character, SetCharacter, CharacterType, Character),
    (enemy, enemy_mut, set_enemy, on_enemy, resort_enemy, SetEnemy, EnemyType, Enemy),
    (player, player_mut, set_player, on_player, resort_player, SetPlayer, PlayerType, Player),
    (collider, collider_mut, set_collider, on_collider, resort_collider, SetCollider, ColliderType, Collider),
    (physical, physical_mut, set_physical, on_physical, resort_physical, SetPhysical, PhysicalType, Physical),
    (anatomy, anatomy_mut, set_anatomy, on_anatomy, resort_anatomy, SetAnatomy, AnatomyType, Anatomy),
    (saveable, saveable_mut, set_saveable, on_saveable, resort_saveable, SetNone, SaveableType, Saveable)
}
