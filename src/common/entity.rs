use std::{
    f32,
    mem,
    rc::Rc,
    fmt::{self, Debug},
    cell::{Ref, RefMut, RefCell}
};

use serde::{Serialize, Deserialize};

use yanyaengine::{game_object::*, Transform};

use crate::{
    debug_config::*,
    server,
    client,
    common::{
        some_or_return,
        write_log,
        insertion_sort_with,
        render_info::*,
        collider::*,
        watcher::*,
        lazy_transform::*,
        damaging::*,
        Door,
        Joint,
        Light,
        ClientLight,
        Outlineable,
        LazyMix,
        DataInfos,
        Occluder,
        ClientOccluder,
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
        Saveable,
        character::PartialCombinedInfo
    }
};

pub use crate::{iterate_components_with, for_each_component};


// too many macros, the syntax is horrible, why r they so limiting? wuts up with that?

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

macro_rules! component_index_with_enum
{
    ($this:expr, $entity:expr, $component:expr) =>
    {
        components!($this, $entity).borrow().get($entity.id)
            .and_then(|components| components[$component as usize])
    }
}

macro_rules! component_index
{
    ($this:expr, $entity:expr, $component:ident) =>
    {
        component_index_with_enum!($this, $entity, Component::$component)
    }
}

macro_rules! swap_indices_of
{
    ($this:expr, $component:ident, $a:expr, $b:expr) =>
    {
        let a = some_or_return!(component_index!($this, $a, $component));
        let b = some_or_return!(component_index!($this, $b, $component));

        $this.$component.swap(a, b);
    }
}

macro_rules! swap_fully
{
    ($this:expr, $component:ident, $a:expr, $b:expr) =>
    {
        $this.swap_component_indices(Component::$component, $a, $b);
        swap_indices_of!($this, $component, $a, $b);
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

#[macro_export]
macro_rules! iterate_components_with
{
    ($this:expr, $component:ident, $iter_func:ident, $handler:expr) =>
    {
        $this.$component.iter().$iter_func(|(_, &$crate::common::entity::ComponentWrapper{
            entity,
            component: ref component
        })|
        {
            $handler(entity, component)
        })
    }
}

#[macro_export]
macro_rules! for_each_component
{
    ($this:expr, $component:ident, $handler:expr) =>
    {
        $crate::iterate_components_with!($this, $component, for_each, $handler)
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
        create_info: &mut UpdateBuffersInfo
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

type UnitType = ();

no_on_set!{
    ClientRenderInfo,
    RenderInfo,
    LazyMix,
    Outlineable,
    LazyTransform,
    FollowRotation,
    FollowPosition,
    Inventory,
    String,
    Parent,
    Transform,
    Enemy,
    Player,
    Anatomy,
    Collider,
    Physical,
    Door,
    Joint,
    Light,
    ClientLight,
    Damaging,
    Watchers,
    Occluder,
    ClientOccluder,
    UnitType
}

no_on_set_for!{ServerEntities, Character}

impl OnSet<ClientEntities> for Character
{
    fn on_set(previous: Option<Self>, entities: &ClientEntities, entity: Entity)
    {
        if let Some(previous) = previous
        {
            entities.character_mut_no_change(entity).unwrap().with_previous(previous);
        }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullEntityInfo
{
    pub children: Vec<FullEntityInfo>,
    pub info: EntityInfo
}

impl FullEntityInfo
{
    pub fn create(self, mut f: impl FnMut(EntityInfo) -> Entity) -> Entity
    {
        self.create_inner(&mut f)
    }

    fn create_inner(self, f: &mut impl FnMut(EntityInfo) -> Entity) -> Entity
    {
        let this = f(self.info);

        self.children.into_iter().for_each(|mut child|
        {
            child.info.parent = Some(Parent::new(this, true));

            child.create_inner(f);
        });

        this
    }
}

impl EntityInfo
{
    pub fn to_full(
        entities: &ServerEntities,
        this: Entity
    ) -> Option<FullEntityInfo>
    {
        debug_assert!(entities.saveable_exists(this));

        // this isnt the root node therefore skip
        if let Some(parent) = entities.parent(this)
        {
            debug_assert!(entities.saveable_exists(parent.entity()));

            return None;
        }

        Some(Self::to_full_always(entities, this))
    }

    fn to_full_always(entities: &ServerEntities, this: Entity) -> FullEntityInfo
    {
        let info = entities.info(this);

        let children: Vec<_> = entities.children_of(this).map(|child|
        {
            entities.set_parent(child, None);

            Self::to_full_always(entities, child)
        }).collect();

        FullEntityInfo{
            children,
            info
        }
    }
}

pub struct InFlightGetter<T>(T);

pub struct SetChanged<'a>(&'a ClientEntities);

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
    ($this_entity_info:ident, $(($name:ident, $set_func:ident, $component_type:ident)),+,) =>
    {
        fn push_inner(
            &mut self,
            local: bool,
            mut info: $this_entity_info
        ) -> Entity
        {
            let entity = self.push_empty(local, info.parent.as_ref().map(|x| x.entity));

            info.setup_components(self);

            self.set_each(entity, info);

            entity
        }

        fn handle_message_common(&mut self, message: Message) -> Option<Message>
        {
            match message
            {
                Message::SetTarget{entity, target} =>
                {
                    if let Some(mut x) = self.target(entity)
                    {
                        *x = *target;
                    }

                    None
                },
                Message::SyncPosition{entity, position} =>
                {
                    if let Some(mut transform) = self.target(entity)
                    {
                        transform.position = position;
                    }

                    None
                },
                Message::SyncPositionRotation{entity, position, rotation} =>
                {
                    if let Some(mut transform) = self.target(entity)
                    {
                        transform.position = position;
                        transform.rotation = rotation;
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

        pub fn update_lazy_mix(&mut self, dt: f32)
        {
            for_each_component!(self, lazy_mix, |entity, lazy_mix: &RefCell<LazyMix>|
            {
                if let Some(mut render) = self.render_mut_no_change(entity)
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

        fn for_every_child_inner(
            &self,
            entity: Entity,
            f: &mut impl FnMut(Entity)
        )
        {
            f(entity);
            self.children_of(entity).for_each(move |entity| self.for_every_child_inner(entity, f));
        }

        fn remove_queued(&mut self)
        {
            let queue = mem::take(self.remove_queue.get_mut());
            queue.into_iter().for_each(|entity|
            {
                self.remove(entity);
            });
        }

        fn create_queued_common(
            &mut self,
            mut f: impl FnMut(&mut Self, Entity, EntityInfo) -> $this_entity_info
        )
        {
            let queue = mem::take(self.create_queue.get_mut());
            queue.into_iter().for_each(|(entity, mut info)|
            {
                if self.exists(entity)
                {
                    info.setup_components(self);

                    let info = f(self, entity, info);

                    self.set_each(entity, info);
                }
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

        pub fn end_sync(&self, entity: Entity, f: impl FnOnce(RefMut<Transform>, Transform))
        {
            if let Some(transform) = self.transform_mut(entity)
            {
                if let Some(end) = self.lazy_target_end(entity)
                {
                    f(transform, end);
                }
            }
        }

        pub fn end_sync_full(&self, entity: Entity)
        {
            self.end_sync(entity, |mut current, target| *current = target);
        }
    }
}

macro_rules! entity_info_common
{
    () =>
    {
        pub fn setup_components(
            &mut self,
            entities: &impl AnyEntities
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
            } else
            {
                let must_have_lazy = self.follow_rotation.is_some() || self.follow_position.is_some();

                if must_have_lazy
                {
                    self.lazy_transform = Some(LazyTransformInfo::default().into());
                }
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

                self.lazy_transform.as_mut().unwrap().rotation = Rotation::Ignore;
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

                self.lazy_transform.as_mut().unwrap().connection = Connection::Ignore;
            }

            if let Some(lazy_mix) = self.lazy_mix.as_ref()
            {
                if let Some(render) = self.render.as_mut()
                {
                    render.mix = Some(lazy_mix.target);
                }
            }

            if self.anatomy.is_some() && self.watchers.is_none()
            {
                self.watchers = Some(Default::default());
            }

            if self.player.is_none() && self.inventory.is_some() && self.outlineable.is_none()
            {
                self.outlineable = Some(Outlineable::default());
            }

            if self.outlineable.is_some() && self.watchers.is_none()
            {
                self.watchers = Some(Watchers::default());
            }

            if self.character.is_some()
            {
                self.lazy_transform.as_mut().unwrap().deformation = Deformation::Stretch(
                    StretchDeformation{
                        animation: ValueAnimation::EaseOut(1.1),
                        limit: 1.3,
                        onset: 0.5,
                        strength: 0.2
                    }
                );
            }
        }
    }
}

macro_rules! common_trait_impl
{
    (
        ($(($fn_ref:ident, $fn_mut:ident, $value_type:ident)),+,),
        ($(($set_func:ident, $exists_name:ident, $shared_type:ident)),+,)
    ) =>
    {
        $(
            fn $fn_ref(&self, entity: Entity) -> Option<Ref<$value_type>>
            {
                Self::$fn_ref(self, entity)
            }

            fn $fn_mut(&self, entity: Entity) -> Option<RefMut<$value_type>>
            {
                Self::$fn_mut(self, entity)
            }
        )+

        $(
            fn $exists_name(&self, entity: Entity) -> bool
            {
                Self::$exists_name(self, entity)
            }

            fn $set_func(&self, entity: Entity, component: Option<$shared_type>)
            {
                self.lazy_setter.borrow_mut().$set_func(entity, component);
            }
        )+

        fn infos(&self) -> &DataInfos
        {
            self.infos.as_ref().unwrap()
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
            Self::lazy_transform_mut_no_change(self, entity).map(|lazy|
            {
                RefMut::map(lazy, |x| x.target())
            })
        }

        fn lazy_target_end(&self, entity: Entity) -> Option<Transform>
        {
            self.lazy_target_end(entity)
        }

        fn for_every_child(
            &self,
            entity: Entity,
            mut f: impl FnMut(Entity)
        )
        {
            self.for_every_child_inner(entity, &mut f);
        }

        fn z_level(&self, entity: Entity) -> Option<ZLevel>
        {
            self.render(entity).map(|x| x.z_level())
        }

        fn set_z_level(&self, entity: Entity, z_level: ZLevel)
        {
            self.render_mut(entity).map(|mut x| x.set_z_level(z_level));

            *self.z_changed.borrow_mut() = true;
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

        fn in_flight(&self) -> InFlightGetter<RefMut<SetterQueue<$($shared_type,)+>>>
        {
            InFlightGetter(self.lazy_setter.borrow_mut())
        }

        fn remove_deferred(&self, entity: Entity)
        {
            self.remove_queue.borrow_mut().push(entity);
        }

        fn remove(&mut self, entity: Entity)
        {
            Self::remove(self, entity);
        }

        fn check_guarantees(&mut self)
        {
            const PANIC_ON_FAIL: bool = false;
            let side = if Self::is_server() { "SERVER" } else { "CLIENT" };

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
                            if !(parent_id < child_id)
                            {
                                let body = format!("[{side} CHILD-PARENT FAILED] ({parent_id} ({parent:?}) < {child_id} ({entity:?}))",);

                                eprintln!("{body}");

                                write_log(format!(
                                    "{body} parent: {}, child: {}",
                                    self.info_ref(parent),
                                    self.info_ref(entity)
                                ));

                                if PANIC_ON_FAIL { panic!() }
                            }
                        }
                    }
                });
            };

            for_components(&self.components, false);
            for_components(&self.local_components, true);

            let reducer = |(before, before_z), x@(after, after_z)|
            {
                if !(before_z <= after_z)
                {
                    let body = format!("[{side} Z-ORDER FAILED] ({before_z:?} ({before:?}) <= {after_z:?} ({after:?}))");

                    eprintln!("{body}");

                    write_log(format!(
                        "{body} before: {}, after: {}",
                        self.info_ref(before),
                        self.info_ref(after)
                    ));

                    if PANIC_ON_FAIL { panic!() }
                }

                x
            };

            iterate_components_with!(self, render, map, |entity, _|
            {
                (entity, self.z_level(entity).unwrap())
            }).reduce(reducer);

            for_each_component!(self, saveable, |entity, _|
            {
                if let Some(parent) = self.parent(entity)
                {
                    let index = component_index!(self, entity, saveable)
                        .unwrap();

                    if let Some(parent_index) = component_index!(self, parent.entity(), saveable)
                    {
                        if !(parent_index < index)
                        {
                            let parent_entity = parent.entity();
                            let body = format!("[{side} SAVEABLE ORDER FAILED] ({parent_index:?} ({parent_entity:?}) < {index:?} ({entity:?}))");

                            eprintln!("{body}");

                            write_log(format!(
                                "{body} parent: {}, child: {}",
                                self.info_ref(parent_entity),
                                self.info_ref(entity)
                            ));

                            if PANIC_ON_FAIL { panic!() }
                        }
                    }
                }
            });
        }
    }
}

macro_rules! order_sensitives
{
    ($(($name:ident, $resort_name:ident)),+) =>
    {
        const fn order_sensitive(component: Component) -> bool
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
        $mut_func_no_change:ident,
        $set_func:ident,
        $set_func_no_change:ident,
        $on_name:ident,
        $resort_name:ident,
        $exists_name:ident,
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

        #[allow(dead_code)]
        impl SetChanged<'_>
        {
            $(
                pub fn $name(&self, entity: Entity)
                {
                    self.0.changed_entities.borrow_mut().$name.push(entity);
                }
            )+
        }

        pub const COMPONENTS_COUNT: usize = count_components();

        #[derive(Clone, Serialize, Deserialize)]
        pub struct EntityInfo<$($component_type=$default_type,)+>
        {
            $(pub $name: Option<$component_type>,)+
        }

        impl<$($component_type: Debug,)+> Debug for EntityInfo<$($component_type,)+>
        {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
            {
                let mut s = f.debug_struct("EntityInfo");

                $(if let Some(component) = self.$name.as_ref()
                {
                    s.field(stringify!($name), component);
                })+

                s.finish()
            }
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

            pub fn compact_format(&self) -> String
            {
                let mut components = String::new();
                $(
                    if self.$name.is_some()
                    {
                        if !components.is_empty()
                        {
                            components += ", ";
                        }

                        components += stringify!($name);
                    }
                )+

                format!("EntityInfo[{components}]")
            }
        }

        pub struct OnChangeInfo<'a>
        {
            pub entities: &'a mut ClientEntities,
            pub entity: Entity,
            pub index: usize,
            pub total: usize
        }

        pub type OnComponentChange = Box<dyn FnMut(OnChangeInfo)>;

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
            $($name: Vec<(Entity, Option<$component_type>, bool)>,)+
        }

        impl<$($component_type,)+> SetterQueue<$($component_type,)+>
        {
            $(
                pub fn $set_func(&mut self, entity: Entity, component: Option<$component_type>)
                {
                    self.changed = true;
                    self.$name.push((entity, component, true));
                }

                pub fn $set_func_no_change(&mut self, entity: Entity, component: Option<$component_type>)
                {
                    self.changed = true;
                    self.$name.push((entity, component, false));
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

        impl<$($component_type,)+> InFlightGetter<RefMut<'_, SetterQueue<$($component_type,)+>>>
        {
            $(
                pub fn $mut_func(&mut self, entity: Entity) -> Option<&mut $component_type>
                {
                    self.0.$name.iter_mut().find(|(e, _c, _)| *e == entity)
                        .and_then(|(_entity, component, _)| component.as_mut())
                }
            )+
        }


        pub struct Entities<$($component_type=$default_type,)+>
        {
            pub local_components: RefCell<ObjectsStore<ComponentsIndices>>,
            pub components: RefCell<ObjectsStore<ComponentsIndices>>,
            pub lazy_setter: RefCell<SetterQueue<$($default_type,)+>>,
            infos: Option<DataInfos>,
            z_changed: RefCell<bool>,
            remove_queue: RefCell<Vec<Entity>>,
            create_queue: RefCell<Vec<(Entity, EntityInfo)>>,
            create_render_queue: RefCell<Vec<(Entity, RenderComponent)>>,
            changed_entities: RefCell<ChangedEntities>,
            side_sync: RefCell<SideSyncEntities>,
            $($on_name: Rc<RefCell<Vec<OnComponentChange>>>,)+
            $(pub $name: ObjectsStore<ComponentWrapper<$component_type>>,)+
        }

        impl<$($component_type: OnSet<Self> + Debug,)+> Entities<$($component_type,)+>
        where
            Self: AnyEntities,
            for<'a> &'a ParentType: Into<&'a Parent>
        {
            pub fn new(infos: impl Into<Option<DataInfos>>) -> Self
            {
                let this = Self{
                    local_components: RefCell::new(ObjectsStore::new()),
                    components: RefCell::new(ObjectsStore::new()),
                    lazy_setter: RefCell::new(Default::default()),
                    infos: infos.into(),
                    z_changed: RefCell::new(false),
                    remove_queue: RefCell::new(Vec::new()),
                    create_queue: RefCell::new(Vec::new()),
                    create_render_queue: RefCell::new(Vec::new()),
                    changed_entities: RefCell::new(Default::default()),
                    side_sync: RefCell::new(Default::default()),
                    $($on_name: Rc::new(RefCell::new(Vec::new())),)+
                    $($name: ObjectsStore::new(),)+
                };

                this.on_render(Box::new(move |OnChangeInfo{entities, ..}|
                {
                    entities.resort_by_z();
                }));

                this.on_anatomy(Box::new(move |OnChangeInfo{entities, entity, ..}|
                {
                    if let Some(mut character) = entities.character_mut(entity)
                    {
                        let anatomy = entities.anatomy(entity).unwrap();

                        character.anatomy_changed(&anatomy);
                    }
                }));

                this
            }

            pub fn exists(&self, entity: Entity) -> bool
            {
                components!(self, entity).borrow().get(entity.id).is_some()
            }

            pub fn for_each_entity(
                &self,
                mut f: impl FnMut(Entity)
            )
            {
                self.try_for_each_entity(|x| -> Result<(), ()>
                {
                    f(x);

                    Ok(())
                }).unwrap();
            }

            pub fn try_for_each_entity<E>(
                &self,
                mut f: impl FnMut(Entity) -> Result<(), E>
            ) -> Result<(), E>
            {
                self.components.borrow().iter()
                    .map(|(id, _)| Entity{local: false, id})
                    .chain(self.local_components.borrow().iter().map(|(id, _)| Entity{local: true, id}))
                    .try_for_each(|entity|
                    {
                        f(entity)
                    })
            }

            pub fn component_info(&self, entity: Entity, name: &str) -> Option<String>
            {
                let name = name.replace(' ', "_").to_lowercase();
                match name.as_ref()
                {
                    $(stringify!($name) =>
                    {
                        self.$name(entity).map(|component|
                        {
                            format!("{component:#?}")
                        })
                    },)+
                    _ => None
                }
            }

            pub fn info_ref(&self, entity: Entity) -> String
            {
                if !self.exists(entity)
                {
                    return String::new();
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

                format!("{info:#?}")
            }

            fn set_each(&mut self, entity: Entity, info: EntityInfo<$($component_type,)+>)
            {
                $(
                    if info.$name.is_some()
                    {
                        self.$set_func_no_change(entity, info.$name);
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

            pub fn resort_queued(&mut self)
            {
                let changed = self.z_changed.get_mut();
                if *changed
                {
                    *changed = false;

                    self.resort_by_z_force();
                }
            }

            fn resort_by_z(&mut self)
            {
                *self.z_changed.get_mut() = true;
            }

            fn resort_by_z_force(&mut self)
            {
                // cycle sort has the least amount of swaps but i dunno
                // if its worth the increased amount of checks

                // maybe shellsort is better?

                let mut z_levels: Vec<_> = iterate_components_with!(self, render, map, |entity, _|
                {
                    (self.z_level(entity), entity)
                }).collect();

                insertion_sort_with(&mut z_levels, |(z_level, _)| *z_level, |&(_, before), &(_, after)|
                {
                    swap_fully!(self, render, before, after);
                });
            }

            $(
                pub fn $name(&self, entity: Entity) -> Option<Ref<$component_type>>
                {
                    get_entity!(self, entity, get, $name)
                }

                pub fn $mut_func(&self, entity: Entity) -> Option<RefMut<$component_type>>
                {
                    {
                        const NAME: &'static str = stringify!($name);
                        if (NAME != "transform") && (NAME != "watchers")
                        {
                            self.changed_entities.borrow_mut().$name.push(entity);
                        }
                    }

                    self.$mut_func_no_change(entity)
                }

                pub fn $mut_func_no_change(&self, entity: Entity) -> Option<RefMut<$component_type>>
                {
                    get_entity!(self, entity, get_mut, $name)
                }

                pub fn $exists_name(&self, entity: Entity) -> bool
                {
                    component_index!(self, entity, $name).is_some()
                }

                pub fn $set_func(&mut self, entity: Entity, component: Option<$component_type>)
                {
                    self.changed_entities.get_mut().$name.push(entity);

                    self.$set_func_no_change(entity, component)
                }

                pub fn $set_func_no_change(&mut self, entity: Entity, component: Option<$component_type>)
                {
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

                            let existed_before = slot.is_some();
                            let value = if let Some(id) = slot
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

                                None
                            };

                            drop(components);

                            if existed_before && parent_order_sensitive
                            {
                                self.$resort_name(entity);
                            }

                            if Component::$name == Component::parent
                            {
                                let parent_entity = self.parent(entity).map(|x|
                                {
                                    (&*x).into().entity()
                                }).unwrap();

                                self.resort_all(parent_entity);
                            } else if Component::$name == Component::render
                            {
                                self.resort_by_z();
                            }

                            value
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

                pub fn $on_name(&self, f: OnComponentChange)
                {
                    self.$on_name.borrow_mut().push(f);
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

            fn swap_component_indices(
                &mut self,
                component: Component,
                a: Entity,
                b: Entity
            )
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

            pub fn children_of(&self, parent_entity: Entity) -> impl Iterator<Item=Entity> + '_
            {
                self.parent.iter().filter_map(move |(_, &ComponentWrapper{
                    entity,
                    component: ref parent
                })|
                {
                    let parent = parent.borrow();

                    ((&*parent).into().entity() == parent_entity).then(||
                    {
                        entity
                    })
                })
            }

            pub fn remove_children(&mut self, parent_entity: Entity)
            {
                let remove_list: Vec<_> = self.children_of(parent_entity).collect();

                remove_list.into_iter().for_each(|entity|
                {
                    self.remove(entity);
                });
            }

            order_sensitives!(
                (parent, resort_parent),
                (lazy_transform, resort_lazy_transform),
                (follow_rotation, resort_follow_rotation),
                (follow_position, resort_follow_position),
                (saveable, resort_saveable)
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

            pub fn set_changed(&self) -> SetChanged
            {
                SetChanged(self)
            }

            #[allow(dead_code)]
            pub fn push_client_eager(
                &mut self,
                info: ClientEntityInfo
            ) -> Entity
            {
                self.push_inner(true, info)
            }

            impl_common_systems!{ClientEntityInfo, $(($name, $set_func, $component_type),)+}

            pub fn handle_on_change(&mut self)
            {
                $(
                    let changed_entities = self.changed_entities.get_mut();

                    let taken = mem::take(&mut changed_entities.$name);

                    if DebugConfig::is_enabled(DebugTool::PrintListenerUpdates)
                    {
                        if !taken.is_empty()
                        {
                            let count = taken.len();
                            eprintln!("updating {count} {} listeners", stringify!($on_name));
                        }
                    }

                    let total = taken.len();
                    taken.into_iter().enumerate().for_each(|(index, entity)|
                    {
                        let listeners = self.$on_name.clone();

                        let entities: &mut Self = self;
                        listeners.borrow_mut().iter_mut().for_each(move |on_change|
                        {
                            on_change(OnChangeInfo{entities, entity, index, total});
                        });
                    });
                )+
            }

            pub fn create_render_queued(&mut self, create_info: &mut UpdateBuffersInfo)
            {
                let render_queue = {
                    let mut queue = self.create_render_queue.borrow_mut();

                    mem::take(&mut *queue)
                };

                let mut needs_resort = false;

                fn constrain<F>(f: F) -> F
                where
                    F: FnMut(&mut ClientEntities, (Entity, RenderComponent)) -> (&mut ClientEntities, Option<(Entity, RenderComponent)>)
                {
                    f
                }

                let mut try_create = constrain(|
                    this,
                    (entity, render)
                |
                {
                    if this.exists(entity)
                    {
                        match render
                        {
                            RenderComponent::Full(_) =>
                            {
                                let transform = if let Some(x) = this.transform_clone(entity)
                                {
                                    x
                                } else
                                {
                                    return (this, Some((entity, render)));
                                };

                                if let RenderComponent::Full(render) = render
                                {
                                    let render = render.server_to_client(
                                        move ||
                                        {
                                            transform
                                        },
                                        create_info
                                    );

                                    this.set_render(entity, Some(render));
                                } else
                                {
                                    unreachable!()
                                }
                            },
                            RenderComponent::Object(object) =>
                            {
                                if let Some((transform, mut render)) = this.render_mut(entity).and_then(|render|
                                {
                                    this.transform_clone(entity).map(|transform| (transform, render))
                                })
                                {
                                    let object = object.into_client(transform.clone(), create_info);

                                    render.object = object;

                                    needs_resort = true;
                                }
                            },
                            RenderComponent::Scissor(scissor) =>
                            {
                                if let Some(mut render) = this.render_mut_no_change(entity)
                                {
                                    let size = create_info.partial.size;
                                    let scissor = scissor.into_global(size);

                                    render.scissor = Some(scissor);
                                }
                            }
                        }

                        return (this, None);
                    }

                    (this, Some((entity, render)))
                });

                render_queue.into_iter().for_each(|x|
                {
                    if let (this, Some(ignored)) = try_create(self, x)
                    {
                        this.create_render_queue.borrow_mut().push(ignored);
                    }
                });

                if needs_resort
                {
                    self.resort_by_z();
                }
            }

            pub fn create_queued(
                &mut self,
                create_info: &mut UpdateBuffersInfo
            )
            {
                crate::frame_time_this!{
                    lazy_set,
                    self.lazy_set_common(create_info)
                };

                crate::frame_time_this!{
                    create_queued_common,
                    self.create_queued_common(|this, entity, info|
                    {
                        ClientEntityInfo::from_server(
                            this,
                            entity,
                            create_info,
                            info
                        )
                    })
                };

                crate::frame_time_this!{
                    remove_queued,
                    self.remove_queued()
                };
            }

            pub fn update_children(&mut self)
            {
                for_each_component!(self, parent, |entity, parent: &RefCell<Parent>|
                {
                    let parent = parent.borrow();

                    if let Some(mut render) = self.render_mut_no_change(entity)
                    {
                        let parent_visible = self.render(parent.entity).map(|parent_render|
                        {
                            parent_render.visible
                        }).unwrap_or(true);

                        render.visible = parent.visible && parent_visible;
                    }
                });
            }

            pub fn is_lootable(&self, entity: Entity) -> bool
            {
                let is_player = self.player_exists(entity);
                let has_inventory = self.inventory(entity).map(|inventory|
                {
                    !inventory.is_empty()
                }).unwrap_or(false);

                let maybe_anatomy = if let Some(anatomy) = self.anatomy(entity)
                {
                    anatomy.speed() == 0.0
                } else
                {
                    true
                };

                !is_player && has_inventory && maybe_anatomy
            }

            pub fn within_interactable_distance(&self, a: Entity, b: Entity) -> bool
            {
                let interactable_distance = 0.3;

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

                let mouse_collided = some_or_return!(mouse_collided);

                if !self.within_interactable_distance(player, mouse_collided)
                {
                    return;
                }

                for_each_component!(self, outlineable, |entity, outlineable: &RefCell<Outlineable>|
                {
                    let overlapping = mouse_collided == entity;

                    if !overlapping || !self.is_lootable(entity)
                    {
                        return;
                    }

                    if let Some(mut watchers) = self.watchers_mut(entity)
                    {
                        outlineable.borrow_mut().enable();

                        let kind = WatcherType::Lifetime(0.1.into());
                        if let Some(found) = watchers.find(|watcher|
                        {
                            // comparison considered harmful
                            if let WatcherAction::OutlineableDisable = watcher.action
                            {
                                true
                            } else
                            {
                                false
                            }
                        })
                        {
                            found.kind = kind;
                        } else
                        {
                            watchers.push(Watcher{
                                kind,
                                action: WatcherAction::OutlineableDisable,
                                ..Default::default()
                            });
                        }
                    }
                });
            }

            pub fn update_outlineable(&mut self, dt: f32)
            {
                for_each_component!(self, outlineable, |_entity, outlineable: &RefCell<Outlineable>|
                {
                    outlineable.borrow_mut().update(dt);
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
                        transform.clone(),
                        target_global,
                        dt
                    );

                    if let Some(mut follow) = self.follow_rotation_mut_no_change(entity)
                    {
                        let current = &mut transform.rotation;

                        if let Some(target) = self.transform(follow.parent()).map(|x| x.rotation)
                        {
                            follow.next(current, target, dt);
                        }
                    }

                    if let Some(mut follow) = self.follow_position_mut_no_change(entity)
                    {
                        if let Some(target) = self.transform(follow.parent()).map(|x| x.position)
                        {
                            follow.next(&mut transform, target, dt);
                        }
                    }
                }
            }

            pub fn update_lazy(&mut self, dt: f32)
            {
                for_each_component!(self, lazy_transform, |entity, lazy: &RefCell<LazyTransform>|
                {
                    self.update_lazy_one(entity, lazy.borrow_mut(), dt);
                });
            }

            pub fn update_characters(
                &mut self,
                partial: PartialCombinedInfo,
                create_info: &mut UpdateBuffersInfo,
                dt: f32
            )
            {
                for_each_component!(self, character, |entity, character: &RefCell<Character>|
                {
                    let combined_info = partial.to_full(self);

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
                });
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

            pub fn try_info(&self, entity: Entity) -> Option<EntityInfo>
            {
                self.exists(entity).then(|| self.info(entity))
            }

            impl_common_systems!{EntityInfo, $(($name, $set_func, $component_type),)+}

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
                for_each_component!(self, character, |_entity, character: &RefCell<Character>|
                {
                    character.borrow_mut().update_common(characters_info, self);
                });
            }

            pub fn create_queued(
                &mut self,
                writer: &mut server::ConnectionsHandler
            )
            {
                self.lazy_set_common(&mut ());
                self.create_queued_common(|_this, entity, info|
                {
                    let message = Message::EntitySet{entity, info: Box::new(info.clone())};

                    writer.send_message(message);

                    info
                });

                self.remove_queued();

                self.create_render_queue.borrow_mut().clear();
            }

            pub fn push_message(&mut self, info: EntityInfo) -> (Message, Entity)
            {
                let entity = self.push_inner(false, info);

                (Message::EntitySet{entity, info: Box::new(self.info(entity))}, entity)
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
                        debug_assert!(!entity.local, "{entity:?} {component:#?} {:#?}", self.try_info(entity));
                        self.$set_func_no_change(entity, component.map(|x| *x));

                        None
                    },)+
                    x => Some(x)
                }
            }
        }
    }
}

macro_rules! implement_common_complex
{
    ($(($name:ident, $set_func:ident, $set_func_no_change:ident, $server_type:ident, $this_type:ident),)+) =>
    {
        fn lazy_set_common<C>(&mut self, t: &mut C)
        where
            $(Self: LazySideSyncable<$server_type>,)+
            $(C: ServerClientConverter<Self, $server_type, $this_type>,)+
        {
            let mut lazy_setter = self.lazy_setter.borrow_mut();
            if lazy_setter.changed
            {
                lazy_setter.changed = false;

                drop(lazy_setter);

                $(
                    let queue = mem::take(&mut self.lazy_setter.borrow_mut().$name);
                    queue.into_iter().for_each(|(entity, component, is_changed)|
                    {
                        if is_changed
                        {
                            self.side_sync(entity, &component);
                        }

                        let component = component.map(|x| t.convert(self, entity, x));

                        if is_changed
                        {
                            self.$set_func(entity, component);
                        } else
                        {
                            self.$set_func_no_change(entity, component);
                        }
                    });
                )+
            }
        }
    }
}

macro_rules! define_entities
{
    ((side_specific
        $(($side_name:ident,
            $side_mut_func:ident,
            $side_mut_func_no_change:ident,
            $side_set_func:ident,
            $side_set_func_no_change:ident,
            $side_on_name:ident,
            $side_resort_name:ident,
            $side_exists_name:ident,
            $side_message_name:ident,
            $side_component_type:ident,
            $side_default_type:ident,
            $client_type:ident
        )),+),
        $(($name:ident,
            $mut_func:ident,
            $mut_func_no_change:ident,
            $set_func:ident,
            $set_func_no_change:ident,
            $on_name:ident,
            $resort_name:ident,
            $exists_name:ident,
            $message_name:ident,
            $component_type:ident,
            $default_type:ident
        )),+
    ) =>
    {
        #[derive(Debug, Default)]
        struct SideSyncEntities
        {
            changed: bool,
            $($side_name: Vec<(Entity, Option<Box<$side_default_type>>)>,)+
        }

        define_entities_both!{
            $(($side_name, $side_mut_func, $side_mut_func_no_change, $side_set_func, $side_set_func_no_change, $side_on_name, $side_resort_name, $side_exists_name, $side_message_name, $side_component_type, $side_default_type),)+
            $(($name, $mut_func, $mut_func_no_change, $set_func, $set_func_no_change, $on_name, $resort_name, $exists_name, $message_name, $component_type, $default_type),)+
        }

        trait ServerClientConverter<E, T, U>
        {
            fn convert(&mut self, entities: &E, entity: Entity, value: T) -> U;
        }

        $(
            impl ServerClientConverter<ServerEntities, $default_type, $default_type> for ()
            {
                fn convert(
                    &mut self,
                    _entities: &ServerEntities,
                    _entity: Entity,
                    value: $default_type
                ) -> $default_type { value }
            }

            impl ServerClientConverter<ClientEntities, $default_type, $default_type> for UpdateBuffersInfo<'_>
            {
                fn convert(
                    &mut self,
                    _entities: &ClientEntities,
                    _entity: Entity,
                    value: $default_type
                ) -> $default_type { value }
            }
        )+

        $(
            impl ServerClientConverter<ServerEntities, $side_default_type, $side_default_type> for ()
            {
                fn convert(
                    &mut self,
                    _entities: &ServerEntities,
                    _entity: Entity,
                    value: $side_default_type
                ) -> $side_default_type { value }
            }

            impl ServerClientConverter<ClientEntities, $side_default_type, $client_type> for UpdateBuffersInfo<'_>
            {
                fn convert(
                    &mut self,
                    entities: &ClientEntities,
                    entity: Entity,
                    value: $side_default_type
                ) -> $client_type
                {
                    value.server_to_client(|| entities.transform(entity).as_deref().cloned().unwrap_or_default(), self)
                }
            }
        )+

        trait LazySideSyncable<T>
        {
            fn side_sync(&mut self, entity: Entity, value: &Option<T>);
        }

        $(
            impl LazySideSyncable<$default_type> for ServerEntities
            {
                fn side_sync(&mut self, _entity: Entity, _value: &Option<$default_type>) {}
            }

            impl LazySideSyncable<$default_type> for ClientEntities
            {
                fn side_sync(&mut self, _entity: Entity, _value: &Option<$default_type>) {}
            }
        )+

        $(
            impl LazySideSyncable<$side_default_type> for ServerEntities
            {
                fn side_sync(&mut self, _entity: Entity, _value: &Option<$side_default_type>) {}
            }

            impl LazySideSyncable<$side_default_type> for ClientEntities
            {
                fn side_sync(&mut self, entity: Entity, value: &Option<$side_default_type>)
                {
                    let mut side_sync = self.side_sync.borrow_mut();

                    side_sync.changed = true;
                    side_sync.$side_name.push((entity, value.clone().map(Box::new)));
                }
            }
        )+

        impl ClientEntities
        {
            implement_common_complex!{
                $(($name, $set_func, $set_func_no_change, $default_type, $default_type),)+
                $(($side_name, $side_set_func, $side_set_func_no_change, $side_default_type, $client_type),)+
            }
        }

        impl ServerEntities
        {
            implement_common_complex!{
                $(($name, $set_func, $set_func_no_change, $default_type, $default_type),)+
                $(($side_name, $side_set_func, $side_set_func_no_change, $side_default_type, $side_default_type),)+
            }
        }

        impl AnyEntities for ClientEntities
        {
            common_trait_impl!{
                ($(($name, $mut_func, $default_type),)+),
                ($(($side_set_func, $side_exists_name, $side_default_type),)+ $(($set_func, $exists_name, $default_type),)+)
            }

            fn is_server() -> bool { false }

            fn push_eager(
                &mut self,
                local: bool,
                mut info: EntityInfo
            ) -> Entity
            {
                // clients cant create global entities
                assert!(local);

                if DebugConfig::is_enabled(DebugTool::PrintPushEntity)
                {
                    eprintln!("eagerly pushing {}", info.compact_format());
                }

                let entity = self.push_inner(local, info.shared());

                self.create_queue.borrow_mut().push((entity, info));

                entity
            }

            fn push(&self, local: bool, info: EntityInfo) -> Entity
            {
                // clients cant create global entities
                assert!(local);

                if DebugConfig::is_enabled(DebugTool::PrintPushEntity)
                {
                    eprintln!("pushing {}", info.compact_format());
                }

                let entity = self.push_empty(local, info.parent.as_ref().map(|x| x.entity));

                self.create_queue.borrow_mut().push((entity, info));

                entity
            }
        }

        impl AnyEntities for ServerEntities
        {
            common_trait_impl!{
                ($(($name, $mut_func, $default_type),)+),
                ($(($side_set_func, $side_exists_name, $side_default_type),)+ $(($set_func, $exists_name, $default_type),)+)
            }

            fn is_server() -> bool { true }

            fn push_eager(&mut self, local: bool, info: EntityInfo) -> Entity
            {
                Self::push_inner(self, local, info)
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

            $(
                fn $set_func(&self, entity: Entity, component: Option<$default_type>);
                fn $exists_name(&self, entity: Entity) -> bool;
            )+

            $(
                fn $side_set_func(&self, entity: Entity, component: Option<$side_default_type>);
                fn $side_exists_name(&self, entity: Entity) -> bool;
            )+

            fn is_server() -> bool;

            fn infos(&self) -> &DataInfos;

            fn lazy_target_ref(&self, entity: Entity) -> Option<Ref<Transform>>;
            fn lazy_target(&self, entity: Entity) -> Option<RefMut<Transform>>;
            fn lazy_target_end(&self, entity: Entity) -> Option<Transform>;

            fn for_every_child(&self, entity: Entity, f: impl FnMut(Entity));

            fn z_level(&self, entity: Entity) -> Option<ZLevel>;
            fn set_z_level(&self, entity: Entity, z_level: ZLevel);
            fn is_visible(&self, entity: Entity) -> bool;
            fn visible_target(&self, entity: Entity) -> Option<RefMut<bool>>;
            fn mix_color_target(&self, entity: Entity) -> Option<RefMut<Option<MixColor>>>;

            fn exists(&self, entity: Entity) -> bool;

            fn remove_deferred(&self, entity: Entity);
            fn remove(&mut self, entity: Entity);

            fn in_flight(&self) -> InFlightGetter<RefMut<SetterQueue<$($side_default_type,)+ $($default_type,)+>>>;

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
                create_info: &mut UpdateBuffersInfo,
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
            pub fn sync_changed(&self, passer: &mut client::ConnectionsHandler)
            {
                {
                    let mut side_sync = self.side_sync.borrow_mut();
                    if side_sync.changed
                    {
                        side_sync.changed = false;

                        $(
                            mem::take(&mut side_sync.$side_name).into_iter().for_each(|(entity, component)|
                            {
                                passer.send_message(Message::$side_message_name{
                                    entity,
                                    component
                                });
                            });
                        )+
                    }
                }

                let changed_entities = self.changed_entities.borrow();

                $(
                    changed_entities.$name.iter().copied().for_each(|entity|
                    {
                        if entity.local
                        {
                            return;
                        }

                        passer.send_message(Message::$message_name{
                            entity,
                            component: get_entity!(self, entity, get, $name).map(|x| Box::new(x.clone()))
                        });
                    });
                )+
            }

            fn handle_entity_set(
                &mut self,
                create_info: &mut UpdateBuffersInfo,
                entity: Entity,
                info: EntityInfo
            )
            {
                debug_assert!(!entity.local, "{entity:?} {info:#?}");

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
                        self.$side_set_func_no_change(entity, component);
                    }
                })+

                $(
                    if info.$name.is_some()
                    {
                        self.$set_func_no_change(entity, info.$name);
                    }
                )+

                if let (
                    Some(end),
                    Some(mut transform)
                ) = (self.lazy_target_end(entity), self.transform_mut(entity))
                {
                    *transform = end;
                }
            }

            pub fn handle_message(
                &mut self,
                create_info: &mut UpdateBuffersInfo,
                message: Message
            ) -> Option<Message>
            {
                let message = self.handle_message_common(message)?;

                #[allow(unreachable_patterns)]
                match message
                {
                    Message::EntitySetMany{entities} =>
                    {
                        entities.into_iter().for_each(|(entity, info)|
                        {
                            self.handle_entity_set(create_info, entity, info);
                        });

                        None
                    },
                    Message::EntitySet{entity, info} =>
                    {
                        self.handle_entity_set(create_info, entity, *info);

                        None
                    },
                    $(Message::$side_message_name{entity, component} =>
                    {
                        debug_assert!(!entity.local);
                        let component = component.map(|x|
                        {
                            x.server_to_client(||
                            {
                                self.transform_clone(entity).unwrap_or_else(||
                                {
                                    panic!(
                                        "{} expected transform, got none",
                                        stringify!($side_message_name)
                                    )
                                })
                            }, create_info)
                        });

                        self.$side_set_func(entity, component);

                        None
                    },)+
                    $(Message::$message_name{entity, component} =>
                    {
                        debug_assert!(!entity.local);
                        self.$set_func_no_change(entity, component.map(|x| *x));

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
        (render, render_mut, render_mut_no_change, set_render, set_render_no_change, on_render, resort_render, render_exists, SetRender, RenderType, RenderInfo, ClientRenderInfo),
        (light, light_mut, light_mut_no_change, set_light, set_light_no_change, on_light, resort_light, light_exists, SetLight, LightType, Light, ClientLight),
        (occluder, occluder_mut, occluder_mut_no_change, set_occluder, set_occluder_no_change, on_occluder_mut, resort_occluder, occluder_exists, SetOccluder, OccluderType, Occluder, ClientOccluder)),
    (parent, parent_mut, parent_mut_no_change, set_parent, set_parent_no_change, on_parent, resort_parent, parent_exists, SetParent, ParentType, Parent),
    (lazy_mix, lazy_mix_mut, lazy_mix_mut_no_change, set_lazy_mix, set_lazy_mix_no_change, on_lazy_mix, resort_lazy_mix, lazy_mix_exists, SetLazyMix, LazyMixType, LazyMix),
    (outlineable, outlineable_mut, outlinable_mut_no_change, set_outlineable, set_outlineable_no_change, on_outlineable, resort_outlineable, outlineable_exists, SetOutlineable, OutlineableType, Outlineable),
    (lazy_transform, lazy_transform_mut, lazy_transform_mut_no_change, set_lazy_transform, set_lazy_transform_no_change, on_lazy_transform, resort_lazy_transform, lazy_transform_exists, SetLazyTransform, LazyTransformType, LazyTransform),
    (follow_rotation, follow_rotation_mut, follow_rotation_mut_no_change, set_follow_rotation, set_follow_rotation_no_change, on_follow_rotation, resort_follow_rotation, follow_rotation_exists, SetFollowRotation, FollowRotationType, FollowRotation),
    (follow_position, follow_position_mut, follow_position_mut_no_change, set_follow_position, set_follow_position_no_change, on_follow_position, resort_follow_position, follow_position_exists, SetFollowPosition, FollowPositionType, FollowPosition),
    (watchers, watchers_mut, watchers_mut_no_change, set_watchers, set_watchers_no_change, on_watchers, resort_watchers, watchers_exists, SetWatchers, WatchersType, Watchers),
    (damaging, damaging_mut, damaging_mut_no_change, set_damaging, set_damaging_no_change, on_damaging, resort_damaging, damaging_exists, SetDamaging, DamagingType, Damaging),
    (inventory, inventory_mut, inventory_mut_no_change, set_inventory, set_inventory_no_change, on_inventory, resort_inventory, inventory_exists, SetInventory, InventoryType, Inventory),
    (named, named_mut, named_mut_no_change, set_named, set_named_no_change, on_named, resort_named, named_exists, SetNamed, NamedType, String),
    (transform, transform_mut, transform_mut_no_change, set_transform, set_transform_no_change, on_transform, resort_transform, transform_exists, SetTransform, TransformType, Transform),
    (character, character_mut, character_mut_no_change, set_character, set_character_no_change, on_character, resort_character, character_exists, SetCharacter, CharacterType, Character),
    (enemy, enemy_mut, enemy_mut_no_change, set_enemy, set_enemy_no_change, on_enemy, resort_enemy, enemy_exists, SetEnemy, EnemyType, Enemy),
    (player, player_mut, player_mut_no_change, set_player, set_player_no_change, on_player, resort_player, player_exists, SetPlayer, PlayerType, Player),
    (collider, collider_mut, collider_mut_no_change, set_collider, set_collider_no_change, on_collider, resort_collider, collider_exists, SetCollider, ColliderType, Collider),
    (physical, physical_mut, physical_mut_no_change, set_physical, set_physical_no_change, on_physical, resort_physical, physical_exists, SetPhysical, PhysicalType, Physical),
    (anatomy, anatomy_mut, anatomy_mut_no_change, set_anatomy, set_anatomy_no_change, on_anatomy, resort_anatomy, anatomy_exists, SetAnatomy, AnatomyType, Anatomy),
    (door, door_mut, door_mut_no_change, set_door, set_door_no_change, on_door, resort_door, door_exists, SetDoor, DoorType, Door),
    (joint, joint_mut, joint_mut_no_change, set_joint, set_joint_no_change, on_joint, resort_joint, joint_exists, SetJoint, JointType, Joint),
    (saveable, saveable_mut, saveable_mut_no_change, set_saveable, set_saveable_no_change, on_saveable, resort_saveable, saveable_exists, SetNone, SaveableType, Saveable)
}
