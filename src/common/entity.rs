use std::{
    f32,
    mem,
    rc::Rc,
    ops::{ControlFlow, Range},
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
        render_info::*,
        collider::*,
        watcher::*,
        lazy_transform::*,
        damaging::*,
        Door,
        Joint,
        Light,
        ClientLight,
        LazyMix,
        DataInfos,
        Occluder,
        ClientOccluder,
        EntityPasser,
        Inventory,
        Anatomy,
        Character,
        Player,
        Enemy,
        Physical,
        ObjectsStore,
        Message,
        Saveable,
        EntitiesSaver,
        FurnitureId,
        Item,
        furniture_creator,
        characters_info::CHARACTER_DEFORMATION,
        character::PartialCombinedInfo
    }
};

pub use crate::{iterate_components_with, iterate_components_many_with, for_each_component};


// max amount of character initializations per frame
const CHARACTERS_INITIALIZATIONS_MAX: usize = 10;

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
            .and_then(|components| components[const { $component as usize }])
    }
}

macro_rules! check_seed
{
    ($this:expr, $entity:expr, $component:ident) =>
    {
        {
            if cfg!(debug_assertions) && DebugConfig::is_disabled(DebugTool::NoSeedChecks)
            {
                if let Some(component) = component_index_with_enum!($this, $entity, Component::$component).map(|id|
                {
                    $this.$component.get(id).unwrap_or_else(||
                    {
                        panic!("pointer to {} is out of bounds", stringify!($component))
                    })
                })
                {
                    use $crate::debug_config::*;

                    if let Some(check_seed) = $entity.seed()
                    {
                        if component.entity.seed().map(|x| check_seed != x).unwrap_or(false)
                        {
                            let message = format!("{:?} {} {component:#?}", $entity, stringify!($component));
                            if DebugConfig::is_disabled(DebugTool::AllowSeedMismatch)
                            {
                                panic!("{message}");
                            } else
                            {
                                eprintln!("seed mismatch: {message}");
                            }
                        }
                    }
                }
            }
        }
    }
}

macro_rules! component_index
{
    ($this:expr, $entity:expr, $component:ident) =>
    {
        {
            check_seed!($this, $entity, $component);
            component_index!(no_check $this, $entity, $component)
        }
    };
    (no_check $this:expr, $entity:expr, $component:ident) =>
    {
        component_index_with_enum!($this, $entity, Component::$component)
    };
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
            [const { Component::$component as usize }]
            .take();

        if let Some(id) = id
        {
            $this.$component.remove(id);
        }
    }
}

#[macro_export]
macro_rules! iterate_components_many_with
{
    ($this:expr, [$first_component:ident, $($component:ident),+], $iter_func:ident, $handler:expr $(, with_ref_early_exit, $early_exit:expr)? ) =>
    {
        $this.$first_component.iter().$iter_func(|(_, &$crate::common::entity::ComponentWrapper{
            entity,
            component: ref component
        })|
        {
            $(
                let component_ref = component.borrow();

                if $early_exit(&*component_ref)
                {
                    return;
                }
            )?

            let contents = &(if entity.local
            {
                &$this.local_components
            } else
            {
                &$this.components
            }).borrow()[entity.id];

            $(
                let $component = if let Some(x) = contents[const { $crate::common::entity::Component::$component as usize }]
                {
                    x
                } else
                {
                    return;
                };
            )+

            $handler(
                entity,
                $({ let _ = stringify!($early_exit); component_ref },)?
                component,
                $(
                    &$this.$component[$component].component,
                )+
            )
        })
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
    };
    ($this:expr, $component:ident, $iter_func:ident, move_outer, $handler:expr) =>
    {
        $this.$component.iter().$iter_func(move |(_, &$crate::common::entity::ComponentWrapper{
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

pub trait ClientEntitiesPush
{
    fn entities_ref(&self) -> &ClientEntities;

    fn push(&mut self, info: EntityInfo) -> Entity;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Entity
{
    pub local: bool,
    pub id: usize,
    #[cfg(debug_assertions)]
    seed: Option<u32>
}

impl Entity
{
    pub fn from_raw(local: bool, id: usize) -> Entity
    {
        Self{
            local,
            id,
            #[cfg(debug_assertions)]
            seed: DebugConfig::is_disabled(DebugTool::NoSeedChecks).then(|| fastrand::u32(0..u32::MAX))
        }
    }

    pub fn id(&self) -> usize
    {
        self.id
    }

    pub fn local(&self) -> bool
    {
        self.local
    }

    #[cfg(debug_assertions)]
    pub fn seed(&self) -> Option<u32>
    {
        self.seed
    }

    #[cfg(debug_assertions)]
    pub fn seed_mut(&mut self) -> &mut Option<u32>
    {
        &mut self.seed
    }

    #[cfg(not(debug_assertions))]
    pub fn seed(&self) -> Option<u32> { unreachable!() }

    #[cfg(not(debug_assertions))]
    pub fn seed_mut(&mut self) -> &mut Option<u32> { unreachable!() }

    pub fn no_seed(self) -> Self
    {
        Self{
            #[cfg(debug_assertions)]
            seed: None,
            ..self
        }
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
    ($container:ident, $($name:ident),*) =>
    {
        $(impl OnSet<$container> for $name
        {
            fn on_set(_previous: Option<Self>, _entities: &$container, _entity: Entity) {}
        })*
    }
}

type UnitType = ();

no_on_set!{
    ClientRenderInfo,
    RenderInfo,
    LazyMix,
    LazyTransform,
    FollowRotation,
    FollowPosition,
    Inventory,
    String,
    Entity,
    Item,
    f32,
    Transform,
    Enemy,
    Player,
    Anatomy,
    Collider,
    Physical,
    Joint,
    Light,
    ClientLight,
    Damaging,
    Occluder,
    ClientOccluder,
    UnitType
}

no_on_set_for!{ServerEntities, Character, Door, FurnitureId}

impl OnSet<ClientEntities> for Parent
{
    fn on_set(_previous: Option<Self>, entities: &ClientEntities, entity: Entity)
    {
        debug_assert!(
            entities.exists(entities.parent(entity).unwrap().entity()),
            "{}",
            entities.info_ref(entity).map(|x| format!("{x:#?}")).unwrap_or_default()
        );
    }
}

impl OnSet<ServerEntities> for Parent
{
    fn on_set(_previous: Option<Self>, entities: &ServerEntities, entity: Entity)
    {
        debug_assert!(
            entities.exists(entities.parent(entity).unwrap().entity()),
            "{}",
            entities.info_ref(entity).map(|x| format!("{x:#?}")).unwrap_or_default()
        );
    }
}

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

impl OnSet<ClientEntities> for Door
{
    fn on_set(_previous: Option<Self>, entities: &ClientEntities, entity: Entity)
    {
        Door::update_visible(entities, entity);
    }
}

impl OnSet<ClientEntities> for FurnitureId
{
    fn on_set(_previous: Option<Self>, entities: &ClientEntities, entity: Entity)
    {
        furniture_creator::update_furniture(entities, entity);
    }
}

// parent must always come before child !! (index wise)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parent(Entity);

impl Parent
{
    pub fn new(entity: Entity) -> Self
    {
        Self(entity)
    }

    pub fn entity(&self) -> Entity
    {
        self.0
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
            child.info.parent = Some(Parent::new(this));

            child.create_inner(f);
        });

        this
    }
}

impl EntityInfo
{
    pub fn try_to_full<const ONLY_SAVEABLE: bool>(
        entities: &ServerEntities,
        this: Entity
    ) -> Option<FullEntityInfo>
    {
        if ONLY_SAVEABLE && !entities.saveable_exists(this) { return None; }

        Self::to_full_inner::<ONLY_SAVEABLE, false>(entities, this)
    }

    pub fn to_full(
        entities: &ServerEntities,
        this: Entity
    ) -> Option<FullEntityInfo>
    {
        debug_assert!(entities.saveable_exists(this));

        Self::to_full_inner::<true, true>(entities, this)
    }

    fn to_full_inner<const ONLY_SAVEABLE: bool, const EXPECT_SAVEABLE: bool>(
        entities: &ServerEntities,
        this: Entity
    ) -> Option<FullEntityInfo>
    {
        // this isnt the root node therefore skip
        if let Some(parent) = entities.parent(this)
        {
            if EXPECT_SAVEABLE && ONLY_SAVEABLE { debug_assert!(entities.saveable_exists(parent.entity())); }

            return None;
        }

        Some(Self::to_full_always::<ONLY_SAVEABLE, EXPECT_SAVEABLE>(entities, this))
    }

    fn to_full_always<const ONLY_SAVEABLE: bool, const EXPECT_SAVEABLE: bool>(
        entities: &ServerEntities,
        this: Entity
    ) -> FullEntityInfo
    {
        let info = entities.info(this);

        let children: Vec<_> = entities.children_of(this).filter_map(|child|
        {
            if !EXPECT_SAVEABLE && ONLY_SAVEABLE
            {
                if !entities.saveable_exists(child) { return None; }
            }

            Some(Self::to_full_always::<ONLY_SAVEABLE, EXPECT_SAVEABLE>(entities, child))
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
    pub fn get(&self) -> Ref<'_, T>
    {
        self.component.borrow()
    }

    pub fn get_mut(&self) -> RefMut<'_, T>
    {
        self.component.borrow_mut()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityRemove(Entity);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityRemoveMany(Vec<Entity>);

impl EntityRemoveMany
{
    pub fn into_inner(self) -> Vec<Entity>
    {
        self.0
    }
}

impl ClientEntitiesPush for &ClientEntities
{
    fn entities_ref(&self) -> &ClientEntities { self }

    fn push(&mut self, info: EntityInfo) -> Entity { <ClientEntities as AnyEntities>::push(self, true, info) }
}

impl ClientEntitiesPush for &mut ClientEntities
{
    fn entities_ref(&self) -> &ClientEntities { self }

    fn push(&mut self, info: EntityInfo) -> Entity { <ClientEntities as AnyEntities>::push_eager(self, true, info) }
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
            let entity = self.push_empty(local);

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

                        if let Some(mut character) = self.character_mut_no_change(entity)
                        {
                            if let Some(character_rotation) = character.rotation_mut()
                            {
                                *character_rotation = rotation;
                            }
                        } else
                        {
                            transform.rotation = rotation;
                        }
                    }

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

                    self.set_each_existing(entity, info);
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
                if self.transform.is_none()
                {
                    let parent_transform = self.parent.as_ref()
                        .and_then(|x|
                        {
                            entities.transform(x.0).as_deref().cloned()
                        });

                    let new_transform = lazy.target_global(parent_transform.as_ref());
                    self.transform = Some(new_transform);
                }
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

            if self.character.is_some()
            {
                self.lazy_transform.as_mut().unwrap().deformation = CHARACTER_DEFORMATION;
            }
        }
    }
}

macro_rules! common_trait_impl
{
    (
        ($(($fn_ref:ident, $fn_mut:ident, $value_type:ident)),+,),
        ($(($set_func:ident, $set_func_no_change:ident, $exists_name:ident, $shared_type:ident)),+,)
    ) =>
    {
        $(
            fn $fn_ref(&self, entity: Entity) -> Option<Ref<'_, $value_type>>
            {
                Self::$fn_ref(self, entity)
            }

            fn $fn_mut(&self, entity: Entity) -> Option<RefMut<'_, $value_type>>
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

            fn $set_func_no_change(&self, entity: Entity, component: Option<$shared_type>)
            {
                self.lazy_setter.borrow_mut().$set_func_no_change(entity, component);
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

        fn lazy_target_ref(&self, entity: Entity) -> Option<Ref<'_, Transform>>
        {
            Self::lazy_transform(self, entity).map(|lazy|
            {
                Ref::map(lazy, |x| x.target_ref())
            })
        }

        fn lazy_target(&self, entity: Entity) -> Option<RefMut<'_, Transform>>
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

        fn sibling_first(&self, entity: Entity) -> Option<Entity>
        {
            self.with_sibling_of(entity).next()
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
        }

        fn set_outlined(&self, entity: Entity, value: bool)
        {
            if let Some(mut render) = self.render_mut(entity)
            {
                render.outlined = value;
            }
        }

        fn is_visible(&self, entity: Entity) -> bool
        {
            self.render(entity).map(|x| x.visible).unwrap_or(false)
        }

        fn visible_target(&self, entity: Entity) -> Option<RefMut<'_, bool>>
        {
            self.render_mut(entity).map(|render|
            {
                RefMut::map(render, |x| &mut x.visible)
            })
        }

        fn mix_color_target(&self, entity: Entity) -> Option<RefMut<'_, Option<MixColor>>>
        {
            self.render_mut(entity).map(|render|
            {
                RefMut::map(render, |x| &mut x.mix)
            })
        }

        fn in_flight(&self) -> InFlightGetter<Ref<'_, SetterQueue<$($shared_type,)+>>>
        {
            InFlightGetter(self.lazy_setter.borrow())
        }

        fn in_flight_mut(&self) -> InFlightGetter<RefMut<'_, SetterQueue<$($shared_type,)+>>>
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
            let side = if Self::IS_SERVER { "SERVER" } else { "CLIENT" };

            let for_components = |components: &RefCell<ObjectsStore<ComponentsIndices>>, local|
            {
                let components = components.borrow();

                components.iter().for_each(|(id, indices)|
                {
                    let entity = Entity{
                        local,
                        id,
                        #[cfg(debug_assertions)]
                        seed: None
                    };

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
                                    self.info_ref(parent).map(|x| format!("{x:#?}")).unwrap_or_default(),
                                    self.info_ref(entity).map(|x| format!("{x:#?}")).unwrap_or_default()
                                ));

                                if PANIC_ON_FAIL { panic!() }
                            }
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
    ($($name:ident),+) =>
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

        fn resort_all(&mut self, entity: Entity)
        {
            $(
                if const { !(matches!(Component::$name, Component::parent)) }
                {
                    Resorters(self).$name(entity);
                }
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

            pub fn position_rotation(&self, entity: Entity)
            {
                if entity.local
                {
                    return;
                }

                self.0.changed_entities.borrow_mut().position_rotation.push(entity);
            }
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

        impl<$($component_type,)+> EntityInfo<$($component_type,)+>
        {
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

        fn empty_components() -> ComponentsIndices
        {
            [$(
                {
                    let _ = Component::$name;
                    None
                }
            ,)+]
        }

        #[derive(Debug, Default)]
        struct ChangedEntities
        {
            position_rotation: Vec<Entity>,
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

        impl<$($component_type,)+> InFlightGetter<Ref<'_, SetterQueue<$($component_type,)+>>>
        {
            $(
                pub fn $exists_name(&self, entity: Entity) -> bool
                {
                    self.0.$name.iter().any(|(e, c, _)| *e == entity && c.is_some())
                }
            )+
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

        struct Resorters<'a, $($component_type,)+>(&'a mut Entities<$($component_type,)+>);

        impl<'a, $($component_type: Debug,)+> Resorters<'a, $($component_type,)+>
        where
            Entities<$($component_type,)+>: AnyEntities,
            for<'b> &'b ParentType: Into<&'b Parent>
        {
            $(
                // resort grandparent if needed
                fn $set_func_no_change(&mut self, parent_component: usize, parent_entity: Entity)
                {
                    if let Some(grand_parent_entity) = self.0.parent(parent_entity).map(|grand_parent|
                    {
                        let grand_parent: &Parent = (&*grand_parent).into();

                        grand_parent.entity()
                    })
                    {
                        let grand_parent_component = some_or_return!(
                            component_index!(self.0, grand_parent_entity, $name)
                        );

                        if grand_parent_component < parent_component
                        {
                            return;
                        }

                        self.$set_func(grand_parent_component, grand_parent_entity, parent_component, parent_entity);
                    }
                }

                // resort this entity fully
                fn $name(&mut self, parent_entity: Entity)
                {
                    let parent_component = some_or_return!(
                        component_index!(self.0, parent_entity, $name)
                    );

                    // try resorting the grandparent
                    self.$set_func_no_change(parent_component, parent_entity);

                    let child = self.0.$name.iter().find_map(|(component_id, &ComponentWrapper{
                        entity,
                        ..
                    })|
                    {
                        if parent_component < component_id
                        {
                            return None;
                        }

                        self.0.parent(entity).and_then(|parent|
                        {
                            let parent: &Parent = (&*parent).into();
                            (parent.entity() == parent_entity).then(||
                            {
                                (component_id, entity)
                            })
                        })
                    });

                    let (child_component, child) = some_or_return!(child);

                    self.$set_func(parent_component, parent_entity, child_component, child)
                }

                // resort this entity with known child
                fn $set_func(
                    &mut self,
                    parent_component: usize,
                    parent_entity: Entity,
                    child_component: usize,
                    child: Entity
                )
                {
                    // swap contents
                    self.0.$name.swap(child_component, parent_component);

                    self.0.swap_component_indices(Component::$name, child, parent_entity);

                    // try resorting the grandparent
                    self.$set_func_no_change(parent_component, parent_entity);

                    self.$name(child);
                    self.$name(parent_entity);
                }
            )+
        }

        pub struct IterEntities<'a, $($component_type,)+>
        {
            entities: &'a Entities<$($component_type,)+>,
            global: bool,
            local_components_indices: Range<usize>,
            local_components: Ref<'a, ObjectsStore<ComponentsIndices>>,
            components_indices: Range<usize>,
            components: Ref<'a, ObjectsStore<ComponentsIndices>>
        }

        impl<'a, $($component_type: Debug,)+> Iterator for IterEntities<'a, $($component_type,)+>
        {
            type Item = Entity;

            fn next(&mut self) -> Option<Self::Item>
            {
                fn get_next(indices: &mut Range<usize>, components: &ObjectsStore<ComponentsIndices>) -> Option<usize>
                {
                    indices.find(|x| components.get(*x).is_some())
                }

                let entity = if self.global
                {
                    if let Some(id) = get_next(&mut self.components_indices, &self.components)
                    {
                        Entity{
                            local: false,
                            id,
                            #[cfg(debug_assertions)]
                            seed: None
                        }
                    } else
                    {
                        self.global = false;

                        return self.next();
                    }
                } else
                {
                    let id = get_next(&mut self.local_components_indices, &self.local_components)?;

                    Entity{
                        local: true,
                        id,
                        #[cfg(debug_assertions)]
                        seed: None
                    }
                };

                Some(self.entities.with_seed(entity))
            }
        }

        pub struct Entities<$($component_type=$default_type,)+>
        {
            pub local_components: RefCell<ObjectsStore<ComponentsIndices>>,
            pub components: RefCell<ObjectsStore<ComponentsIndices>>,
            pub lazy_setter: RefCell<SetterQueue<$($default_type,)+>>,
            remove_awaiting: Vec<(FullEntityInfo, usize)>,
            infos: Option<DataInfos>,
            remove_queue: RefCell<Vec<Entity>>,
            create_queue: RefCell<Vec<(Entity, EntityInfo)>>,
            create_render_queue: RefCell<Vec<(Entity, RenderComponent)>>,
            changed_entities: RefCell<ChangedEntities>,
            side_sync: RefCell<SideSyncEntities>,
            removed_sync: Vec<Entity>,
            watchers: RefCell<Vec<(Entity, Watcher)>>,
            on_remove: Rc<RefCell<Vec<Box<dyn FnMut(&mut Self, Entity)>>>>,
            $($on_name: Rc<RefCell<Vec<OnComponentChange>>>,)+
            $(pub $name: ObjectsStore<ComponentWrapper<$component_type>>,)+
        }

        impl<$($component_type: Debug,)+> Entities<$($component_type,)+>
        {
            #[allow(unused_mut)]
            pub fn with_seed(&self, mut entity: Entity) -> Entity
            {
                #[cfg(debug_assertions)]
                {
                    if DebugConfig::is_enabled(DebugTool::NoSeedChecks)
                    {
                        return entity;
                    }

                    let mut seed = None;

                    $(
                        if seed.is_none()
                        {
                            if let Some(index) = component_index!(no_check self, entity, $name)
                            {
                                seed = self.$name[index].entity.seed();
                            }
                        }
                    )+

                    if let Some(seed) = seed
                    {
                        *entity.seed_mut() = Some(seed);
                    }
                }

                entity
            }
        }

        impl<$($component_type: Debug,)+> Entities<$($component_type,)+>
        {
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
        }

        impl<$($component_type: OnSet<Self> + Debug,)+> Entities<$($component_type,)+>
        where
            Self: AnyEntities,
            RenderType: RenderInfoTrait,
            for<'a> &'a ParentType: Into<&'a Parent>,
            for<'a> &'a SiblingType: Into<&'a Entity>
        {
            pub fn new(infos: impl Into<Option<DataInfos>>) -> Self
            {
                let this = Self{
                    local_components: RefCell::new(ObjectsStore::new()),
                    components: RefCell::new(ObjectsStore::new()),
                    lazy_setter: RefCell::new(Default::default()),
                    remove_awaiting: Vec::new(),
                    infos: infos.into(),
                    remove_queue: RefCell::new(Vec::new()),
                    create_queue: RefCell::new(Vec::new()),
                    create_render_queue: RefCell::new(Vec::new()),
                    changed_entities: RefCell::new(Default::default()),
                    side_sync: RefCell::new(Default::default()),
                    removed_sync: Vec::new(),
                    watchers: RefCell::new(Vec::new()),
                    on_remove: Rc::new(RefCell::new(Vec::new())),
                    $($on_name: Rc::new(RefCell::new(Vec::new())),)+
                    $($name: ObjectsStore::new(),)+
                };

                this.on_anatomy(Box::new(move |OnChangeInfo{entities, entity, ..}|
                {
                    if let Some(mut character) = entities.character_mut(entity)
                    {
                        let anatomy = entities.anatomy(entity).unwrap();

                        character.anatomy_changed(entities, &anatomy);
                    }
                }));

                this
            }

            pub fn exists(&self, entity: Entity) -> bool
            {
                components!(self, entity).borrow().get(entity.id).is_some()
            }

            pub fn iter_entities(&self) -> IterEntities<'_, $($component_type,)+>
            {
                let local_components = self.local_components.borrow();
                let components = self.components.borrow();

                IterEntities{
                    entities: self,
                    global: true,
                    local_components_indices: local_components.index_range(),
                    local_components,
                    components_indices: components.index_range(),
                    components
                }
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

            pub fn info_ref(&self, entity: Entity) -> Option<EntityInfo<$(Ref<'_, $component_type>,)+>>
            {
                if !self.exists(entity)
                {
                    return None;
                }

                let components = &components!(self, entity).borrow()[entity.id];

                Some(EntityInfo{$(
                    $name: {
                        components[Component::$name as usize].map(|id|
                        {
                            self.$name[id].get()
                        })
                    },
                )+})
            }

            fn set_each(&mut self, entity: Entity, info: EntityInfo<$($component_type,)+>)
            {
                $(
                    self.$set_func_no_change(entity, info.$name);
                )+
            }

            fn set_each_existing(&mut self, entity: Entity, info: EntityInfo<$($component_type,)+>)
            {
                $(
                    if info.$name.is_some()
                    {
                        self.$set_func_no_change(entity, info.$name);
                    }
                )+
            }

            fn push_empty(&self, local: bool) -> Entity
            {
                let components = if local
                {
                    &self.local_components
                } else
                {
                    &self.components
                };

                let mut components = components.borrow_mut();

                let id = components.take_vacant_key();

                components.insert(id, empty_components());

                Entity::from_raw(local, id)
            }

            fn check_all_seeds(&self, entity: Entity)
            {
                $(
                    check_seed!(self, entity, $name);
                )+
            }

            $(
                pub fn $name(&self, entity: Entity) -> Option<Ref<'_, $component_type>>
                {
                    get_entity!(self, entity, get, $name)
                }

                pub fn $mut_func(&self, entity: Entity) -> Option<RefMut<'_, $component_type>>
                {
                    if const { !matches!(Component::$name, Component::transform) }
                    {
                        let mut entities = self.changed_entities.borrow_mut();

                        if !entities.$name.contains(&entity)
                        {
                            entities.$name.push(entity);
                        }
                    }

                    self.$mut_func_no_change(entity)
                }

                pub fn $mut_func_no_change(&self, entity: Entity) -> Option<RefMut<'_, $component_type>>
                {
                    get_entity!(self, entity, get_mut, $name)
                }

                pub fn $exists_name(&self, entity: Entity) -> bool
                {
                    component_index!(self, entity, $name).is_some()
                }

                pub fn $set_func(&mut self, entity: Entity, component: Option<$component_type>)
                {
                    {
                        let entities = self.changed_entities.get_mut();

                        if !entities.$name.contains(&entity)
                        {
                            entities.$name.push(entity);
                        }
                    }

                    self.$set_func_no_change(entity, component)
                }

                pub fn $set_func_no_change(&mut self, entity: Entity, component: Option<$component_type>)
                {
                    if DebugConfig::is_disabled(DebugTool::NoSeedChecks)
                    {
                        debug_assert!(entity.seed().is_some(), "{entity:?} {component:#?} {:#?}", self.info_ref(entity));
                    }

                    if Self::IS_SERVER
                    {
                        // it simply discards the set which might not be the best?
                        if self.remove_awaiting.iter().any(|x| x.1 == entity.id) { return; }
                    }

                    let parent_order_sensitive = const { Self::order_sensitive(Component::$name) };

                    if !self.exists(entity)
                    {
                        components!(self, entity)
                            .borrow_mut()
                            .insert(entity.id, empty_components());
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

                            if const { matches!(Component::$name, Component::parent) }
                            {
                                self.resort_all(entity);
                            } else if parent_order_sensitive
                            {
                                // even if it didnt exist before it might be a parent and its quicker to check this way
                                Resorters(self).$name(entity);
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

                    self.check_all_seeds(entity);
                }

                pub fn $on_name(&self, f: OnComponentChange)
                {
                    self.$on_name.borrow_mut().push(f);
                }
            )+

            pub fn on_remove(&self, f: Box<dyn FnMut(&mut Self, Entity)>)
            {
                self.on_remove.borrow_mut().push(f);
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

            fn clear_components_inner(&mut self, entity: Entity)
            {
                let components = &components!(self, entity).borrow()[entity.id];

                $(if let Some(id) = components[Component::$name as usize]
                {
                    self.$name.remove(id);
                })+
            }

            fn clear_components(&mut self, entity: Entity)
            {
                self.remove_lazy_components(entity);
                self.remove_from_remove_queue(entity);
                self.remove_from_create_queue(entity);
                self.remove_from_watchers(entity);

                self.remove_children(entity);
                self.try_remove_sibling(entity);

                self.clear_components_inner(entity);

                let components = components!(self, entity);
                components.borrow_mut()[entity.id] = empty_components();
            }

            fn remove_lazy_components(&mut self, entity: Entity)
            {
                let setter = self.lazy_setter.get_mut();

                if !setter.changed
                {
                    return;
                }

                $(
                    setter.$name.retain(|x| x.0 != entity);
                )+
            }

            fn remove_from_remove_queue(&mut self, entity: Entity)
            {
                self.remove_queue.get_mut().retain(|x| *x != entity);
            }

            fn remove_from_create_queue(&mut self, entity: Entity)
            {
                self.create_queue.get_mut().retain(|x| x.0 != entity);
                self.create_render_queue.get_mut().retain(|x| x.0 != entity);
            }

            fn remove_from_watchers(&mut self, entity: Entity)
            {
                self.watchers.borrow_mut().retain(|(check, _)| *check != entity);
            }

            pub fn remove(&mut self, entity: Entity)
            {
                if !Self::IS_SERVER && !entity.local
                {
                    self.removed_sync.push(entity);
                }

                self.remove_inner(entity);
            }

            pub fn remove_inner(&mut self, entity: Entity)
            {
                if !self.exists(entity)
                {
                    return;
                }

                self.on_remove.clone().borrow_mut().iter_mut().for_each(|x| x(self, entity));

                self.remove_lazy_components(entity);
                self.remove_from_remove_queue(entity);
                self.remove_from_create_queue(entity);
                self.remove_from_watchers(entity);

                self.remove_children(entity);
                self.try_remove_sibling(entity);

                self.clear_components_inner(entity);

                let components = components!(self, entity);
                components.borrow_mut().remove(entity.id);
            }

            pub fn with_sibling_of(&self, sibling_entity: Entity) -> impl Iterator<Item=Entity> + '_
            {
                self.sibling.iter().filter_map(move |(_, &ComponentWrapper{
                    entity,
                    component: ref sibling
                })|
                {
                    let sibling = sibling.borrow();

                    (*(&*sibling).into() == sibling_entity).then_some(entity)
                })
            }

            pub fn children_of(&self, parent_entity: Entity) -> impl Iterator<Item=Entity> + '_
            {
                self.parent.iter().filter_map(move |(_, &ComponentWrapper{
                    entity,
                    component: ref parent
                })|
                {
                    let parent = parent.borrow();

                    ((&*parent).into().entity() == parent_entity).then_some(entity)
                })
            }

            pub fn try_remove_sibling(&mut self, entity: Entity)
            {
                let sibling = *some_or_return!(<Self as AnyEntities>::sibling(self, entity));

                self.remove_inner(sibling);
            }

            pub fn remove_children(&mut self, parent_entity: Entity)
            {
                let mut remove_list = self.parent.iter().filter_map(move |(_, &ComponentWrapper{
                    entity,
                    component: ref parent
                })|
                {
                    let parent = parent.borrow();

                    ((&*parent).into().entity() == parent_entity).then_some(entity)
                }).collect::<Vec<_>>();

                self.create_queue.get_mut().retain(|(entity, info)|
                {
                    let remove_this = info.parent.as_ref().map(|parent| parent.entity() == parent_entity).unwrap_or(false);

                    if remove_this
                    {
                        remove_list.push(*entity);
                    }

                    !remove_this
                });

                {
                    let setter = self.lazy_setter.get_mut();

                    if setter.changed
                    {
                        remove_list.extend(setter.parent.iter().filter_map(|(entity, component, _)|
                        {
                            component.as_ref().and_then(|x| (x.entity() == parent_entity).then_some(*entity))
                        }));
                    }
                }

                remove_list.into_iter().for_each(|entity|
                {
                    self.remove_inner(entity);
                });
            }

            order_sensitives!(
                parent,
                lazy_transform,
                follow_rotation,
                follow_position
            );
        }

        impl ClientEntities
        {
            fn transform_clone(&self, entity: Entity) -> Option<Transform>
            {
                self.transform(entity).as_deref().cloned()
            }

            pub fn set_changed(&self) -> SetChanged<'_>
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
                self.changed_entities.get_mut().position_rotation.clear();

                let changed_entities = self.changed_entities.get_mut();

                $(
                    let $name = mem::take(&mut changed_entities.$name);
                )+

                $(
                    let taken = $name;

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
            }

            pub fn create_queued(
                &mut self,
                create_info: &mut UpdateBuffersInfo
            )
            {
                crate::frame_time_this!{
                    [update, game_state_update, create_queued] -> lazy_set,
                    self.lazy_set_common(create_info)
                };

                crate::frame_time_this!{
                    [update, game_state_update, create_queued] -> common,
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
                    [update, game_state_update, create_queued] -> remove,
                    self.remove_queued()
                };
            }

            pub fn add_watcher(&self, entity: Entity, watcher: Watcher)
            {
                self.watchers.borrow_mut().push((entity, watcher));
            }

            pub fn replace_watcher(&self, check_entity: Entity, watcher: Watcher)
            {
                debug_assert!(watcher.id.is_some());

                let check_id = some_or_return!(watcher.id);

                let mut watchers = self.watchers.borrow_mut();
                if let Some(found) = watchers.iter_mut().find(|(entity, watcher)|
                {
                    *entity == check_entity && watcher.id.map(|id| id == check_id).unwrap_or(false)
                })
                {
                    found.1 = watcher;
                } else
                {
                    watchers.push((check_entity, watcher));
                }
            }

            pub fn update_watchers(
                &mut self,
                dt: f32
            )
            {
                let mut actions = Vec::new();
                self.watchers.borrow_mut().retain_mut(|(entity, watcher)|
                {
                    let meets = watcher.kind.meets(self, *entity, dt);

                    if meets
                    {
                        actions.push((*entity, mem::replace(&mut watcher.action, Box::new(|_, _| {}))));
                    }

                    !meets
                });

                actions.into_iter().for_each(|(entity, action)|
                {
                    action(self, entity);
                });
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
                let mut initialized_count = 0;

                let combined_info = partial.to_full(self);

                let _ = iterate_components_with!(self, character, try_for_each, |entity, character: &RefCell<Character>|
                {
                    let mut character = character.borrow_mut();

                    let initialized = character.try_initialize(self, entity);

                    if let Some(true) = initialized
                    {
                        if initialized_count > CHARACTERS_INITIALIZATIONS_MAX
                        {
                            return ControlFlow::Break(());
                        } else
                        {
                            initialized_count += 1;
                        }
                    }

                    if initialized.is_some()
                    {
                        character.update(
                            combined_info,
                            entity,
                            dt,
                            |entity, texture|
                            {
                                if let Some(mut target) = self.target(entity)
                                {
                                    if let Some(mut render) = self.render_mut(entity)
                                    {
                                        target.scale.x = texture.scale.x;
                                        target.scale.y = texture.scale.y;

                                        render.set_sprite(create_info, Some(&target), texture.id);
                                    }
                                }

                                self.end_sync(entity, |mut transform, end| transform.scale = end.scale);
                            }
                        );
                    }

                    ControlFlow::Continue(())
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

            fn handle_send_remove<const ONLY_SAVEABLE: bool>(&mut self, entity: Entity)
            {
                debug_assert!(!entity.local);

                if let Some(info) = EntityInfo::try_to_full::<ONLY_SAVEABLE>(self, entity)
                {
                    self.remove_awaiting.push((info, entity.id));
                }

                self.clear_components(entity);
            }

            pub fn send_remove(&mut self, entity: Entity) -> EntityRemove
            {
                self.handle_send_remove::<false>(entity);

                EntityRemove(entity)
            }

            pub fn send_remove_many<const ONLY_SAVEABLE: bool>(&mut self, entities: Vec<Entity>) -> EntityRemoveMany
            {
                entities.iter().for_each(|entity| self.handle_send_remove::<ONLY_SAVEABLE>(*entity));

                EntityRemoveMany(entities)
            }

            pub fn push_message(&mut self, info: EntityInfo) -> (Message, Entity)
            {
                let entity = self.push_inner(false, info);

                (Message::EntitySet{entity, info: Box::new(self.info(entity))}, entity)
            }

            pub fn take_remove_awaiting(&mut self) -> Vec<(FullEntityInfo, usize)>
            {
                mem::take(&mut self.remove_awaiting)
            }

            pub fn get_remove_awaiting(&self) -> &[(FullEntityInfo, usize)]
            {
                &self.remove_awaiting
            }

            fn remove_awaiting_entity(&mut self, entity: Entity) -> Option<FullEntityInfo>
            {
                debug_assert!(self.remove_awaiting.iter().any(|x| x.1 == entity.id), "{entity:?} {:#?}", self.info_ref(entity));

                self.remove_awaiting.iter().position(|x| x.1 == entity.id).map(|index|
                {
                    self.remove_awaiting.swap_remove(index).0
                })
            }

            fn handle_entity_remove_finished(
                &mut self,
                entity: Entity
            )
            {
                self.remove_awaiting_entity(entity);

                self.remove_inner(entity);
            }

            pub fn handle_message(
                &mut self,
                passer: &mut server::ConnectionsHandler,
                saver: &mut EntitiesSaver,
                message: Message
            ) -> Option<Message>
            {
                let message = self.handle_message_common(message)?;

                #[allow(unreachable_patterns)]
                match message
                {
                    $(Message::$message_name{entity, component} =>
                    {
                        debug_assert!(!entity.local, "{} {entity:?} {component:#?} {:#?}", stringify!($message_name), self.try_info(entity));
                        self.$set_func_no_change(entity, component.map(|x| *x));

                        None
                    },)+
                    Message::EntityRemoveFinished{entity} =>
                    {
                        self.handle_entity_remove_finished(entity);

                        None
                    },
                    Message::EntityRemoveManyFinished{entities} =>
                    {
                        entities.into_iter().for_each(|entity|
                        {
                            self.handle_entity_remove_finished(entity);
                        });

                        None
                    },
                    Message::EntityRemoveManyRequest(entities) =>
                    {
                        passer.send_message(Message::EntityRemoveMany(self.send_remove_many::<false>(entities)));

                        None
                    },
                    Message::EntityRemoveChunkFinished{pos, entities} =>
                    {
                        {
                            let infos: Vec<_> = entities.iter().filter_map(|entity|
                            {
                                self.remove_awaiting_entity(*entity)
                            }).collect();

                            saver.save_append(pos, infos);
                        }

                        entities.into_iter().for_each(|entity|
                        {
                            self.remove_inner(entity);
                        });

                        None
                    },
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
                        if is_changed && !entity.local
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
            $(($side_name, $side_mut_func, $side_mut_func_no_change, $side_set_func, $side_set_func_no_change, $side_on_name, $side_exists_name, $side_message_name, $side_component_type, $side_default_type),)+
            $(($name, $mut_func, $mut_func_no_change, $set_func, $set_func_no_change, $on_name, $exists_name, $message_name, $component_type, $default_type),)+
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

            pub fn sync_all_shared(&self, entity: Entity, mut f: impl FnMut(Message))
            {
                $(
                    if self.$exists_name(entity)
                    {
                        f(Message::$message_name{
                            entity,
                            component: self.$name(entity).map(|x| Box::new(x.clone()))
                        });
                    }
                )+
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
            const IS_SERVER: bool = false;

            common_trait_impl!{
                ($(($name, $mut_func, $default_type),)+),
                ($(($side_set_func, $side_set_func_no_change, $side_exists_name, $side_default_type),)+ $(($set_func, $set_func_no_change, $exists_name, $default_type),)+)
            }

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

                self.create_queue.get_mut().push((entity, info));

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

                let entity = self.push_empty(local);

                self.create_queue.borrow_mut().push((entity, info));

                entity
            }
        }

        impl AnyEntities for ServerEntities
        {
            const IS_SERVER: bool = true;

            common_trait_impl!{
                ($(($name, $mut_func, $default_type),)+),
                ($(($side_set_func, $side_set_func_no_change, $side_exists_name, $side_default_type),)+ $(($set_func, $set_func_no_change, $exists_name, $default_type),)+)
            }

            fn push_eager(&mut self, local: bool, info: EntityInfo) -> Entity
            {
                Self::push_inner(self, local, info)
            }

            fn push(&self, local: bool, info: EntityInfo) -> Entity
            {
                let entity = self.push_empty(local);

                self.create_queue.borrow_mut().push((entity, info));

                entity
            }
        }

        pub trait AnyEntities
        {
            const IS_SERVER: bool;

            $(
                fn $name(&self, entity: Entity) -> Option<Ref<'_, $default_type>>;
                fn $mut_func(&self, entity: Entity) -> Option<RefMut<'_, $default_type>>;
            )+

            $(
                fn $set_func(&self, entity: Entity, component: Option<$default_type>);
                fn $set_func_no_change(&self, entity: Entity, component: Option<$default_type>);
                fn $exists_name(&self, entity: Entity) -> bool;
            )+

            $(
                fn $side_set_func(&self, entity: Entity, component: Option<$side_default_type>);
                fn $side_set_func_no_change(&self, entity: Entity, component: Option<$side_default_type>);
                fn $side_exists_name(&self, entity: Entity) -> bool;
            )+

            fn infos(&self) -> &DataInfos;

            fn lazy_target_ref(&self, entity: Entity) -> Option<Ref<'_, Transform>>;
            fn lazy_target(&self, entity: Entity) -> Option<RefMut<'_, Transform>>;
            fn lazy_target_end(&self, entity: Entity) -> Option<Transform>;

            fn sibling_first(&self, entity: Entity) -> Option<Entity>;

            fn for_every_child(&self, entity: Entity, f: impl FnMut(Entity));

            fn z_level(&self, entity: Entity) -> Option<ZLevel>;
            fn set_z_level(&self, entity: Entity, z_level: ZLevel);

            fn set_outlined(&self, entity: Entity, value: bool);

            fn is_visible(&self, entity: Entity) -> bool;
            fn visible_target(&self, entity: Entity) -> Option<RefMut<'_, bool>>;

            fn mix_color_target(&self, entity: Entity) -> Option<RefMut<'_, Option<MixColor>>>;

            fn exists(&self, entity: Entity) -> bool;

            fn remove_deferred(&self, entity: Entity);
            fn remove(&mut self, entity: Entity);

            fn in_flight(&self) -> InFlightGetter<Ref<'_, SetterQueue<$($side_default_type,)+ $($default_type,)+>>>;
            fn in_flight_mut(&self) -> InFlightGetter<RefMut<'_, SetterQueue<$($side_default_type,)+ $($default_type,)+>>>;

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
                    self.transform(parent.0).as_deref().cloned()
                })
            }

            fn target_ref(&self, entity: Entity) -> Option<Ref<'_, Transform>>
            {
                self.lazy_target_ref(entity).or_else(||
                {
                    self.transform(entity)
                })
            }

            fn target(&self, entity: Entity) -> Option<RefMut<'_, Transform>>
            {
                self.lazy_target(entity).or_else(||
                {
                    self.transform_mut(entity)
                })
            }

            fn is_server(&self) -> bool { Self::IS_SERVER }

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
            pub fn sync_changed(&mut self, passer: &mut client::ConnectionsHandler)
            {
                {
                    let side_sync = self.side_sync.get_mut();
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

                {
                    let removed = mem::take(&mut self.removed_sync);
                    if !removed.is_empty()
                    {
                        passer.send_message(Message::EntityRemoveManyRequest(removed));
                    }
                }

                let changed_entities = self.changed_entities.borrow();

                changed_entities.position_rotation.iter().copied().for_each(|entity|
                {
                    debug_assert!(!entity.local);

                    let target = some_or_return!(self.target_ref(entity));

                    passer.send_message(Message::SyncPositionRotation{entity, position: target.position, rotation: target.rotation});
                });

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
                mut info: EntityInfo
            )
            {
                debug_assert!(!entity.local, "{entity:?} {info:#?}");

                self.remove_inner(entity.no_seed());

                info.setup_components(self);

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

                    self.$side_set_func_no_change(entity, component);
                })+

                $(
                    self.$set_func_no_change(entity, info.$name);
                )+

                if let (
                    Some(end),
                    Some(mut transform)
                ) = (self.lazy_target_end(entity), self.transform_mut(entity))
                {
                    *transform = end;
                }
            }

            pub fn handle_entity_remove_many(&mut self, entities: &[Entity])
            {
                crate::frame_time_this!{
                    [update, game_state_update, process_messages] -> entity_remove_many_raw,
                    {
                        entities.iter().for_each(|entity|
                        {
                            self.remove_inner(*entity);
                        });
                    }
                };
            }

            pub fn handle_message(
                &mut self,
                passer: &mut client::ConnectionsHandler,
                create_info: &mut UpdateBuffersInfo,
                message: Message,
                is_trusted: bool
            ) -> Option<Message>
            {
                let message = crate::frame_time_this!{
                    [update, game_state_update, process_messages] -> handle_message_common,
                    self.handle_message_common(message)?
                };

                #[allow(unreachable_patterns)]
                match message
                {
                    Message::EntitySetMany{entities} =>
                    {
                        crate::frame_time_this!{
                            [update, game_state_update, process_messages] -> entity_set_many,
                            {
                                if DebugConfig::is_enabled(DebugTool::DebugTimings)
                                {
                                    eprint!("with {} entities ", entities.len());
                                }

                                $crate::debug_time_this!(
                                    "entity-set-many",
                                    entities.into_iter().for_each(|(entity, info)|
                                    {
                                        self.handle_entity_set(create_info, entity, info)
                                    })
                                );
                            }
                        };

                        None
                    },
                    Message::EntitySet{entity, info} =>
                    {
                        crate::frame_time_this!{
                            [update, game_state_update, process_messages] -> entity_set,
                            self.handle_entity_set(create_info, entity, *info)
                        };

                        None
                    },
                    Message::EntityRemove(EntityRemove(entity)) =>
                    {
                        crate::frame_time_this!{
                            [update, game_state_update, process_messages] -> entity_remove,
                            {
                                self.remove_inner(entity);

                                if is_trusted { passer.send_message(Message::EntityRemoveFinished{entity}); }
                            }
                        };

                        None
                    },
                    Message::EntityRemoveManyRaw(entities) =>
                    {
                        self.handle_entity_remove_many(&entities);

                        None
                    },
                    Message::EntityRemoveMany(EntityRemoveMany(entities)) =>
                    {
                        self.handle_entity_remove_many(&entities);

                        if is_trusted { passer.send_message(Message::EntityRemoveManyFinished{entities}); }

                        None
                    },
                    $(Message::$side_message_name{entity, component} =>
                    {
                        crate::frame_time_this!{
                            [update, game_state_update, process_messages] -> $side_name,
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
                            }
                        };

                        None
                    },)+
                    $(Message::$message_name{entity, component} =>
                    {
                        crate::frame_time_this!{
                            [update, game_state_update, process_messages] -> $name,
                            {
                                debug_assert!(!entity.local);
                                self.$set_func_no_change(entity, component.map(|x| *x));
                            }
                        };

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
        (render, render_mut, render_mut_no_change, set_render, set_render_no_change, on_render, render_exists, SetRender, RenderType, RenderInfo, ClientRenderInfo),
        (light, light_mut, light_mut_no_change, set_light, set_light_no_change, on_light, light_exists, SetLight, LightType, Light, ClientLight),
        (occluder, occluder_mut, occluder_mut_no_change, set_occluder, set_occluder_no_change, on_occluder_mut, occluder_exists, SetOccluder, OccluderType, Occluder, ClientOccluder)),
    (parent, parent_mut, parent_mut_no_change, set_parent, set_parent_no_change, on_parent, parent_exists, SetParent, ParentType, Parent),
    (sibling, sibling_mut, sibling_mut_no_change, set_sibling, set_sibling_no_change, on_sibling, sibling_exists, SetSibling, SiblingType, Entity),
    (furniture, furniture_mut, furniture_mut_no_change, set_furniture, set_furniture_no_change, on_furniture, furniture_exists, SetFurniture, FurnitureType, FurnitureId),
    (item, item_mut, item_mut_no_change, set_item, set_item_no_change, on_item, item_exists, SetItem, ItemType, Item),
    (health, health_mut, health_mut_no_change, set_health, set_health_no_change, on_health, health_exists, SetHealth, HealthType, f32),
    (lazy_mix, lazy_mix_mut, lazy_mix_mut_no_change, set_lazy_mix, set_lazy_mix_no_change, on_lazy_mix, lazy_mix_exists, SetLazyMix, LazyMixType, LazyMix),
    (lazy_transform, lazy_transform_mut, lazy_transform_mut_no_change, set_lazy_transform, set_lazy_transform_no_change, on_lazy_transform, lazy_transform_exists, SetLazyTransform, LazyTransformType, LazyTransform),
    (follow_rotation, follow_rotation_mut, follow_rotation_mut_no_change, set_follow_rotation, set_follow_rotation_no_change, on_follow_rotation, follow_rotation_exists, SetFollowRotation, FollowRotationType, FollowRotation),
    (follow_position, follow_position_mut, follow_position_mut_no_change, set_follow_position, set_follow_position_no_change, on_follow_position, follow_position_exists, SetFollowPosition, FollowPositionType, FollowPosition),
    (damaging, damaging_mut, damaging_mut_no_change, set_damaging, set_damaging_no_change, on_damaging, damaging_exists, SetDamaging, DamagingType, Damaging),
    (inventory, inventory_mut, inventory_mut_no_change, set_inventory, set_inventory_no_change, on_inventory, inventory_exists, SetInventory, InventoryType, Inventory),
    (named, named_mut, named_mut_no_change, set_named, set_named_no_change, on_named, named_exists, SetNamed, NamedType, String),
    (transform, transform_mut, transform_mut_no_change, set_transform, set_transform_no_change, on_transform, transform_exists, SetTransform, TransformType, Transform),
    (character, character_mut, character_mut_no_change, set_character, set_character_no_change, on_character, character_exists, SetCharacter, CharacterType, Character),
    (enemy, enemy_mut, enemy_mut_no_change, set_enemy, set_enemy_no_change, on_enemy, enemy_exists, SetEnemy, EnemyType, Enemy),
    (player, player_mut, player_mut_no_change, set_player, set_player_no_change, on_player, player_exists, SetPlayer, PlayerType, Player),
    (collider, collider_mut, collider_mut_no_change, set_collider, set_collider_no_change, on_collider, collider_exists, SetCollider, ColliderType, Collider),
    (physical, physical_mut, physical_mut_no_change, set_physical, set_physical_no_change, on_physical, physical_exists, SetPhysical, PhysicalType, Physical),
    (anatomy, anatomy_mut, anatomy_mut_no_change, set_anatomy, set_anatomy_no_change, on_anatomy, anatomy_exists, SetAnatomy, AnatomyType, Anatomy),
    (door, door_mut, door_mut_no_change, set_door, set_door_no_change, on_door, door_exists, SetDoor, DoorType, Door),
    (joint, joint_mut, joint_mut_no_change, set_joint, set_joint_no_change, on_joint, joint_exists, SetJoint, JointType, Joint),
    (saveable, saveable_mut, saveable_mut_no_change, set_saveable, set_saveable_no_change, on_saveable, saveable_exists, SetNone, SaveableType, Saveable)
}
