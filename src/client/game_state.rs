use std::{
    f32,
    mem,
    env,
    thread::JoinHandle,
    cell::{Ref, RefCell},
    rc::Rc,
    ops::ControlFlow,
    collections::{VecDeque, BTreeMap, btree_map::Entry},
    sync::{
        Arc,
        mpsc::{self, TryRecvError, Receiver}
    }
};

use parking_lot::{RwLock, Mutex};

use nalgebra::{Vector2, Vector3, Matrix4};

use serde::{Serialize, Deserialize};

use image::RgbaImage;

use yanyaengine::{
    ResourceUploader,
    Assets,
    ObjectFactory,
    Transform,
    TextureId,
    SolidObject,
    DefaultTexture,
    DefaultModel,
    object::{texture::SimpleImage, Texture},
    camera::Camera,
    game_object::*
};

use crate::{
    debug_config::*,
    app::{ProgramShaders, TimestampQuery},
    common::{
        some_or_value,
        some_or_return,
        receiver_loop,
        render_info::*,
        lazy_transform::*,
        Loot,
        MessagePasser,
        ClientLight,
        TileMap,
        DataInfos,
        InventoryItem,
        AnyEntities,
        Entity,
        EntityInfo,
        Entities,
        EntityPasser,
        EntitiesController,
        OccludingCaster,
        OnChangeInfo,
        OnConnectInfo,
        sender_loop::BufferSender,
        message::Message,
        character::PartialCombinedInfo,
        systems::{
            render_system,
            physical_system,
            enemy_system,
            damaging_system,
            anatomy_system,
            collider_system::{self, ContactResolver}
        },
        entity::{
            iterate_components_many_with,
            ClientEntities
        },
        world::{
            TILE_SIZE,
            World,
            Pos3,
            Tile,
            TilePos,
            OccludedCheckerInfo,
            chunk::{rounded_single, to_tile_single}
        }
    }
};

use super::{
    ConnectionsHandler,
    TilesFactory,
    VisibilityChecker
};

pub use controls_controller::{ControlsController, UiControls, Control, ControlState, KeyMapping};

use notifications::{Notifications, Notification};

pub use anatomy_locations::UiAnatomyLocations;
pub use ui::{Ui, UiId, UiEntities, NotificationInfo, NotificationKindInfo};

mod controls_controller;

mod notifications;

mod anatomy_locations;
pub mod ui;


const DEFAULT_ZOOM: f32 = 3.0;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GlobalEntityId
{
    pub is_local: bool,
    pub entity: Entity
}

pub struct ClientEntitiesContainer
{
    pub entities: ClientEntities,
    pub camera_entity: Entity,
    pub player_entity: Entity,
    pub follow_target: Rc<RefCell<Entity>>,
    visible_renders: Vec<Vec<Entity>>,
    above_world_renders: Vec<Entity>,
    occluders: Vec<Entity>,
    light_renders: Vec<Entity>,
    shaded_renders: Vec<Vec<Entity>>,
    animation: f32
}

impl ClientEntitiesContainer
{
    pub fn new(infos: DataInfos, player_entity: Entity) -> Self
    {
        let mut entities = Entities::new(infos);

        let camera_entity = entities.push_eager(true, EntityInfo{
            lazy_transform: Some(LazyTransformInfo{
                connection: Connection::EaseOut{decay: 5.0, limit: None},
                ..Default::default()
            }.into()),
            ..Default::default()
        });

        Self{
            entities,
            camera_entity,
            player_entity,
            follow_target: Rc::new(RefCell::new(player_entity)),
            visible_renders: Vec::new(),
            above_world_renders: Vec::new(),
            occluders: Vec::new(),
            light_renders: Vec::new(),
            shaded_renders: Vec::new(),
            animation: 0.0
        }
    }

    pub fn set_follow_target(&mut self, follow_target: Entity)
    {
        *self.follow_target.borrow_mut() = follow_target;
    }

    pub fn follow_target(&self) -> Entity
    {
        *self.follow_target.borrow()
    }

    pub fn handle_message(
        &mut self,
        passer: &mut ConnectionsHandler,
        create_info: &mut UpdateBuffersInfo,
        message: Message,
        is_trusted: bool
    ) -> Option<Message>
    {
        self.entities.handle_message(passer, create_info, message, is_trusted)
    }

    pub fn update(
        &mut self,
        world: &mut World,
        passer: &mut ConnectionsHandler,
        loot: &Loot,
        damage_info: &CommonTextures,
        _is_trusted: bool,
        dt: f32
    )
    {
        crate::frame_time_this!{
            [update, update_pre] -> lazy_transform_update,
            self.entities.update_lazy(dt)
        };

        crate::frame_time_this!{
            [update, update_pre] -> anatomy_system_update,
            anatomy_system::update(&mut self.entities, dt)
        };

        let space = crate::frame_time_this!{
            [update, update_pre] -> spatial_grid_build,
            world.build_spatial(&self.entities, self.follow_target())
        };

        crate::frame_time_this!{
            [update, update_pre] -> sleeping_update,
            collider_system::update_sleeping(&self.entities, &space)
        };

        if DebugConfig::is_disabled(DebugTool::DisableEnemySystem)
        {
            crate::frame_time_this!{
                [update, update_pre] -> enemy_system_update,
                enemy_system::update(&mut self.entities, world, &space, dt)
            };
        }

        crate::frame_time_this!{
            [update, update_pre] -> lazy_mix_update,
            self.entities.update_lazy_mix(dt)
        };

        crate::frame_time_this!{
            [update, update_pre] -> physical_update,
            physical_system::update(&mut self.entities, world, dt)
        };

        let contacts = crate::frame_time_this!{
            [update, update_pre] -> collider_system_update,
            collider_system::update(&mut self.entities, world, &space)
        };

        crate::frame_time_this!{
            [update, update_pre] -> physical_system_apply,
            physical_system::apply(&mut self.entities, world)
        };

        crate::frame_time_this!{
            [update, update_pre] -> collided_entities_sync,
            contacts.iter().for_each(|contact|
            {
                let set_changed = self.entities.set_changed();

                set_changed.position_rotation(contact.a);

                if let Some(b) = contact.b
                {
                    set_changed.position_rotation(b);
                }
            })
        };

        crate::frame_time_this!{
            [update, update_pre] -> damaging_system_update,
            damaging_system::update(&mut self.entities, &space, world, loot, passer, damage_info)
        };

        crate::frame_time_this!{
            [update, update_pre] -> collision_system_resolution,
            ContactResolver::resolve(&self.entities, contacts, dt)
        };

        self.animation = (self.animation + dt) % (f32::consts::PI * 2.0);
    }

    pub fn update_resize(&mut self, size: Vector2<f32>)
    {
        some_or_return!(self.entities.target(self.camera_entity)).scale = size.xyx();
    }

    pub fn update_aspect(&mut self, size: Vector2<f32>)
    {
        self.update_resize(size);
    }

    pub fn main_player(&self) -> Entity
    {
        self.player_entity
    }

    pub fn player_exists(&self) -> bool
    {
        self.entities.exists(self.player_entity)
    }

    fn update_buffers_normal(
        &mut self,
        visibility: &VisibilityChecker,
        info: &mut UpdateBuffersInfo,
        caster: &OccludingCaster,
        world: &mut World,
        occluded_checker_info: &OccludedCheckerInfo
    )
    {
        fn insert_render<V>(renders: &mut BTreeMap<i32, (Vec<V>, Vec<ZLevel>)>, value: V, z: ZLevel, key: i32)
        {
            match renders.entry(key)
            {
                Entry::Vacant(entry) => { entry.insert((vec![value], vec![z])); },
                Entry::Occupied(mut entry) =>
                {
                    let entry = entry.get_mut();

                    let index = match entry.1.binary_search(&z)
                    {
                        Ok(index) => index + 1,
                        Err(index) => index
                    };

                    entry.0.insert(index, value);
                    entry.1.insert(index, z);
                }
            }
        }

        self.above_world_renders.clear();
        self.occluders.clear();

        let mut shaded_renders = BTreeMap::new();
        let mut visible_renders = BTreeMap::new();

        iterate_components_many_with!(
            self.entities,
            [render, transform],
            for_each,
            |entity, render_ref: Ref<ClientRenderInfo>, render_cell: &RefCell<ClientRenderInfo>, transform: &RefCell<Transform>|
            {
                let transform = transform.borrow();

                if !render_ref.visible_narrow(visibility, &transform)
                {
                    return;
                }

                drop(render_ref);

                let mut render = render_cell.borrow_mut();

                render.set_transform(transform.clone());

                let is_render_above = render.above_world;

                if is_render_above
                {
                    self.above_world_renders.push(entity);

                    crate::frame_time_this!{
                        [update_buffers, entities_update_buffers, normal] -> update_draw_buffers_above_world,
                        render.update_buffers(info)
                    };
                } else
                {
                    let real_z = (transform.position.z / TILE_SIZE).floor() as i32;

                    let below_player = {
                        let z = transform.position.z;

                        (visibility.world_position.chunk.0.z != rounded_single(z))
                            || (visibility.world_position.local.pos().z != to_tile_single(z))
                    };

                    let render_transform = some_or_return!(some_or_return!(render.object.as_ref()).transform());

                    let occluded_checker = world.occluded_checker(render_transform);

                    if DebugConfig::is_enabled(DebugTool::DisplayTouchedTiles)
                    {
                        if DebugConfig::get_debug_value_integer() as usize == entity.id
                        {
                            let render_transform = render.object.as_ref().unwrap().transform().unwrap();

                            occluded_checker.debug_touched_tiles(&self.entities, world.visual_global_mapper(), &render_transform);
                        }
                    }

                    let sky_occluded = below_player
                        && occluded_checker.sky_occluded(world.visual_chunks(), occluded_checker_info);

                    if sky_occluded
                    {
                        return;
                    }

                    let occluder_mut = self.entities.occluder_mut_no_change(entity);

                    let is_render_visible = occluder_mut.is_none()
                        && !occluded_checker.wall_occluded(world.visual_occluded(), occluded_checker_info);

                    let is_render_shadow = render.shadow_visible;

                    if let Some(mut occluder) = occluder_mut
                    {
                        occluder.set_transform(transform.clone());
                        occluder.update_buffers(info, caster);

                        if occluder.visible(visibility)
                        {
                            self.occluders.push(entity);
                        }
                    }

                    let z = render.z_level();

                    if is_render_visible
                    {
                        insert_render(&mut visible_renders, entity, z, real_z);
                    }

                    if is_render_shadow
                    {
                        insert_render(&mut shaded_renders, entity, z, real_z);
                    }

                    if is_render_visible || is_render_shadow
                    {
                        crate::frame_time_this!{
                            [update_buffers, entities_update_buffers, normal] -> update_draw_buffers,
                            render.update_buffers(info)
                        };
                    }
                }
            },
            with_ref_early_exit,
            |render: &ClientRenderInfo|
            {
                !render.visible_broad()
            });

        self.shaded_renders = shaded_renders.into_values().map(|(x, _)| x).collect();
        self.visible_renders = visible_renders.into_values().map(|(x, _)| x).collect();
    }

    fn update_buffers_lights(
        &mut self,
        visibility: &VisibilityChecker,
        info: &mut UpdateBuffersInfo,
        world: &mut World,
        occluded_checker_info: &OccludedCheckerInfo
    )
    {
        self.light_renders.clear();

        iterate_components_many_with!(
            self.entities,
            [light, transform],
            for_each,
            |entity, light_ref: Ref<ClientLight>, light_cell: &RefCell<ClientLight>, transform: &RefCell<Transform>|
            {
                let transform = transform.borrow();

                if !light_ref.visible_narrow(visibility, &transform)
                {
                    return;
                }

                let position = transform.position;

                let light_visibility = light_ref.visibility_checker_with(position);

                let below_player = !visibility.world_position.is_same_height(&light_visibility.world_position);

                let light_transform = Transform{
                    scale: light_ref.scale(),
                    ..*transform
                };

                let occluded_checker = world.occluded_checker(&light_transform);

                if below_player
                {
                    if occluded_checker.light_sky_occluded(world.visual_chunks(), occluded_checker_info)
                    {
                        return;
                    }
                }

                if occluded_checker.wall_occluded(world.visual_occluded(), occluded_checker_info)
                {
                    return;
                }

                drop(light_ref);

                crate::frame_time_this!{
                    [update_buffers, entities_update_buffers, lights] -> update_draw_buffers,
                    {
                        light_cell.borrow_mut().update_buffers(info, position);

                        world.update_buffers_light_shadows(
                            info,
                            &light_visibility,
                            &OccludingCaster::from(position),
                            self.light_renders.len()
                        );
                    }
                };

                self.light_renders.push(entity);
            },
            with_ref_early_exit,
            |light: &ClientLight|
            {
                !light.visible_broad()
            });
    }

    fn update_buffers(
        &mut self,
        visibility: &VisibilityChecker,
        info: &mut UpdateBuffersInfo,
        caster: &OccludingCaster,
        world: &mut World
    )
    {
        let occluded_checker_info = world.occluded_checker_info();

        crate::frame_time_this!{
            [update_buffers, entities_update_buffers] -> normal,
            self.update_buffers_normal(visibility, info, caster, world, &occluded_checker_info)
        };

        crate::frame_time_this!{
            [update_buffers, entities_update_buffers] -> lights,
            self.update_buffers_lights(visibility, info, world, &occluded_checker_info)
        };
    }
}

#[derive(Clone)]
pub struct GameStateInfo
{
    pub shaders: ProgramShaders,
    pub camera: Arc<RwLock<Camera>>,
    pub timestamp_query: TimestampQuery,
    pub data_infos: DataInfos,
    pub loot: Loot,
    pub tiles_factory: TilesFactory,
    pub anatomy_locations: Rc<RefCell<dyn FnMut(&mut ObjectCreateInfo, &str) -> UiAnatomyLocations>>,
    pub common_textures: CommonTextures,
    pub player_name: String,
    pub debug_mode: bool,
    pub host: bool
}

pub struct PartCreator<'a, 'b>
{
    pub resource_uploader: &'a mut ResourceUploader<'b>,
    pub assets: &'a mut Assets
}

impl PartCreator<'_, '_>
{
    pub fn create(&mut self, image: RgbaImage) -> TextureId
    {
        let texture = Texture::new(
            self.resource_uploader,
            SimpleImage::from(image).into()
        );

        self.assets.push_texture(texture)
    }
}

#[derive(Clone)]
pub enum UiEvent
{
    Restart,
    Action(Rc<dyn Fn(&mut GameState)>),
    Game(GameUiEvent)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InventoryWhich
{
    Player,
    Other
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UsageKind
{
    Ingest
}

impl UsageKind
{
    fn name(&self) -> &str
    {
        match self
        {
            Self::Ingest => "ingest"
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GameUiEvent
{
    Info{which: InventoryWhich, item: InventoryItem},
    Use{usage: UsageKind, which: InventoryWhich, item: InventoryItem},
    Drop{which: InventoryWhich, item: InventoryItem},
    Wield(InventoryItem),
    Take(InventoryItem)
}

impl GameUiEvent
{
    pub fn name(&self) -> &str
    {
        match self
        {
            Self::Info{..} => "info",
            Self::Use{usage, ..} => usage.name(),
            Self::Drop{..} => "drop",
            Self::Wield(..) => "wield",
            Self::Take(..) => "take"
        }
    }
}

pub struct UiReceiver
{
    events: Vec<UiEvent>
}

impl UiReceiver
{
    pub fn new() -> Rc<RefCell<Self>>
    {
        let this = Self{
            events: Vec::new()
        };

        Rc::new(RefCell::new(this))
    }

    pub fn push(&mut self, event: UiEvent)
    {
        self.events.push(event);
    }

    pub fn consume(&mut self) -> impl Iterator<Item=UiEvent>
    {
        mem::take(&mut self.events).into_iter()
    }
}

#[derive(Clone)]
pub struct CommonTextures
{
    pub dust: TextureId,
    pub blood: TextureId,
    pub solid: TextureId
}

impl CommonTextures
{
    pub fn new(assets: &mut Assets) -> Self
    {
        Self{
            dust: assets.texture_id("decals/dust.png"),
            blood: assets.texture_id("decals/blood.png"),
            solid: assets.default_texture(DefaultTexture::Solid)
        }
    }
}

type DebugVisibility = <DebugConfig as DebugConfigTrait>::DebugVisibility;

pub struct DebugVisibilityState
{
    detached: bool,
    visibility_camera: Camera
}

pub trait DebugVisibilityStateTrait
{
    fn new(camera: &Camera) -> Self;

    fn is_detached(&self) -> bool;

    fn input(&mut self, control: &yanyaengine::Control) -> bool;
    fn update(&mut self, camera: &Camera);

    fn camera(&self) -> &Camera;
}

impl DebugVisibilityStateTrait for DebugVisibilityState
{
    fn new(camera: &Camera) -> Self
    {
        Self{
            detached: false,
            visibility_camera: camera.clone()
        }
    }

    fn is_detached(&self) -> bool
    {
        self.detached
    }

    fn input(&mut self, control: &yanyaengine::Control) -> bool
    {
        use yanyaengine::{PhysicalKey, KeyCode, ElementState};

        if let yanyaengine::Control::Keyboard{
            keycode: PhysicalKey::Code(KeyCode::KeyK),
            state: ElementState::Pressed,
            ..
        } = control
        {
            self.detached = !self.detached;
            eprintln!("camera detached state: {}", self.detached);

            return true;
        }

        false
    }

    fn update(&mut self, camera: &Camera)
    {
        if !self.detached
        {
            self.visibility_camera = camera.clone();
        }
    }

    fn camera(&self) -> &Camera
    {
        &self.visibility_camera
    }
}

impl DebugVisibilityStateTrait for ()
{
    fn new(_camera: &Camera) -> Self {}

    fn is_detached(&self) -> bool { false }

    fn input(&mut self, _control: &yanyaengine::Control) -> bool { false }
    fn update(&mut self, _camera: &Camera) {}

    fn camera(&self) -> &Camera { unreachable!() }
}

pub trait DebugVisibilityTrait
{
    type State: DebugVisibilityStateTrait;

    fn as_bool() -> bool;
}

pub struct DebugVisibilityTrue;
pub struct DebugVisibilityFalse;

impl DebugVisibilityTrait for DebugVisibilityTrue
{
    type State = DebugVisibilityState;

    fn as_bool() -> bool { true }
}

impl DebugVisibilityTrait for DebugVisibilityFalse
{
    type State = ();

    fn as_bool() -> bool { false }
}

pub struct GameState
{
    pub mouse_position: Vector2<f32>,
    pub camera: Arc<RwLock<Camera>>,
    pub assets: Arc<Mutex<Assets>>,
    pub object_factory: Rc<ObjectFactory>,
    pub notifications: Notifications,
    pub entities: ClientEntitiesContainer,
    pub controls: ControlsController<UiId>,
    pub running: bool,
    pub debug_mode: bool,
    pub tilemap: Arc<TileMap>,
    pub data_infos: DataInfos,
    pub user_receiver: Rc<RefCell<UiReceiver>>,
    pub ui: Rc<RefCell<Ui>>,
    pub common_textures: CommonTextures,
    pub connected_and_ready: bool,
    pub world: World,
    loot: Loot,
    screen_object: SolidObject,
    ui_camera: Camera,
    timestamp_query: TimestampQuery,
    shaders: ProgramShaders,
    host: bool,
    is_restart: bool,
    is_trusted: bool,
    is_loading: bool,
    is_paused: bool,
    camera_scale: f32,
    sync_character_delay: f32,
    rare_timer: f32,
    dt: Option<f32>,
    debug_visibility: <DebugVisibility as DebugVisibilityTrait>::State,
    connections_handler: ConnectionsHandler,
    delayed_messages: VecDeque<Message>,
    receiver_handle: Option<JoinHandle<()>>,
    receiver: Receiver<Message>
}

impl Drop for GameState
{
    fn drop(&mut self)
    {
        if let Err(err) = self.connections_handler.send_blocking(&Message::PlayerDisconnect{
            time: Some(self.world.time()),
            restart: self.is_restart,
            host: self.host
        })
        {
            eprintln!("error sending player disconnect message: {err}");
        }

        while let Ok(x) = self.receiver.recv()
        {
            if let Message::PlayerDisconnectFinished = x
            {
                self.receiver_handle.take().unwrap().join().unwrap();

                eprintln!("client shut down properly");
                return;
            }
        }

        eprintln!("disconnect finished improperly");
    }
}

impl GameState
{
    pub fn new(
        object_info: &mut ObjectCreateInfo,
        message_passer: MessagePasser,
        info: GameStateInfo
    ) -> Rc<RefCell<Self>>
    {
        let mouse_position = Vector2::zeros();

        let notifications = Notifications::new();
        let controls = ControlsController::new();

        let mut handler = ConnectionsHandler::new(message_passer);

        let tilemap = info.tiles_factory.tilemap().clone();

        let OnConnectInfo{player_entity, player_position, time} = Self::connect_to_server(
            &mut handler,
            &info.player_name,
            info.host
        );

        let mut entities = ClientEntitiesContainer::new(
            info.data_infos.clone(),
            player_entity
        );

        let world = World::new(
            info.tiles_factory,
            info.camera.read().size(),
            player_position,
            time
        );

        let (sender, receiver) = mpsc::channel();

        let receiver_handle = Some(receiver_loop(handler.passer_clone(), move |message|
        {
            let is_disconnect = match message
            {
                Message::PlayerDisconnectFinished => true,
                _ => false
            };

            if let Err(err) = sender.send(message)
            {
                eprintln!("error sending: {err}");
                ControlFlow::Break(())
            } else if is_disconnect
            {
                ControlFlow::Break(())
            } else
            {
                ControlFlow::Continue(())
            }
        }, || ()));

        let user_receiver = UiReceiver::new();

        let assets = object_info.partial.assets.clone();

        let screen_object = {
            let assets = assets.lock();

            object_info.partial.object_factory.create_solid(
                assets.model(assets.default_model(DefaultModel::Square)).clone(),
                Transform{scale: Vector3::repeat(2.0), ..Default::default()}
            )
        };

        let ui = {
            let mut anatomy_locations = info.anatomy_locations.borrow_mut();

            Ui::new(
                info.data_infos.items_info.clone(),
                object_info,
                &mut entities.entities,
                UiEntities{
                    camera: entities.camera_entity,
                    player: entities.player_entity
                },
                &mut *anatomy_locations,
                user_receiver.clone()
            )
        };

        let debug_visibility = <DebugVisibility as DebugVisibilityTrait>::State::new(
            &info.camera.read()
        );

        let ui_camera = Camera::new(1.0, -1.0..1.0);

        let mut this = Self{
            mouse_position,
            camera: info.camera,
            assets,
            object_factory: object_info.partial.object_factory.clone(),
            notifications,
            entities,
            data_infos: info.data_infos,
            controls,
            running: true,
            screen_object,
            ui_camera,
            timestamp_query: info.timestamp_query,
            shaders: info.shaders,
            world,
            loot: info.loot,
            debug_mode: info.debug_mode,
            tilemap,
            camera_scale: 1.0,
            sync_character_delay: 0.0,
            rare_timer: 0.0,
            dt: None,
            ui,
            common_textures: info.common_textures,
            connected_and_ready: false,
            host: info.host,
            is_restart: false,
            is_trusted: false,
            is_loading: true,
            is_paused: false,
            user_receiver,
            debug_visibility,
            connections_handler: handler,
            delayed_messages: VecDeque::new(),
            receiver_handle,
            receiver
        };

        this.initialize();

        Rc::new(RefCell::new(this))
    }

    fn initialize(&mut self)
    {
        {
            let ui = self.ui.clone();
            let player_entity = self.entities.player_entity;
            let follow_target = self.entities.follow_target.clone();

            self.entities.entities.on_anatomy(Box::new(move |OnChangeInfo{entities, entity, ..}|
            {
                if let Some(mut anatomy) = entities.anatomy_mut_no_change(entity)
                {
                    if anatomy.take_killed()
                    {
                        if entity == player_entity
                        {
                            ui.borrow_mut().player_dead();

                            let death_follow = entities.push(true, EntityInfo{
                                transform: entities.transform(player_entity).as_deref().cloned(),
                                ..Default::default()
                            });

                            *follow_target.borrow_mut() = death_follow;

                            return;
                        }

                        if let Some(mut player) = entities.player_mut(player_entity)
                        {
                            player.kills += 1;
                        }
                    }
                }
            }));
        }

        self.entities.entities.on_inventory(Box::new(move |OnChangeInfo{entities, entity, ..}|
        {
            if let Some(mut character) = entities.character_mut_no_change(entity)
            {
                character.update_holding();
            }
        }));

        {
            let aspect = self.camera.read().aspect();

            self.set_camera_scale(DEFAULT_ZOOM);

            self.resize(aspect);
            self.camera_resized();
        }
    }

    pub fn restart(&mut self)
    {
        self.is_restart = true;
    }

    pub fn sync_character(&mut self, entity: Entity)
    {
        if self.sync_character_delay > 0.0
        {
            return;
        }

        {
            let entities = &self.entities.entities;
            let target = some_or_return!(entities.target_ref(entity));

            let position = target.position;
            self.connections_handler.send_message(Message::SyncPositionRotation{
                entity,
                position,
                rotation: target.rotation
            });
        }

        self.sync_character_delay = 0.5;
    }

    fn connect_to_server(
        handler: &mut ConnectionsHandler,
        name: &str,
        host: bool
    ) -> OnConnectInfo
    {
        let message = Message::PlayerConnect{name: name.to_owned(), host};
        if let Err(x) = handler.send_blocking(&message)
        {
            panic!("error connecting to server: {x}");
        }

        match handler.receive_blocking()
        {
            Ok(Some(Message::PlayerOnConnect(x))) => x,
            x => panic!("received wrong message on connect: {x:?}")
        }
    }

    pub fn entities(&self) -> &ClientEntities
    {
        &self.entities.entities
    }

    pub fn entities_mut(&mut self) -> &mut ClientEntities
    {
        &mut self.entities.entities
    }

    pub fn player_entity(&self) -> Entity
    {
        self.entities.player_entity
    }

    pub fn process_messages(&mut self, create_info: &mut UpdateBuffersInfo)
    {
        crate::frame_time_this!{
            [update, game_state_update, process_messages] -> send_buffered,
            {
                if self.connections_handler.send_buffered().is_err()
                {
                    self.running = false;
                }
            }
        };

        if let Some(message) = self.delayed_messages.pop_front()
        {
            self.process_message_inner(create_info, message);

            if !self.is_loading
            {
                return;
            }
        }

        loop
        {
            match self.receiver.try_recv()
            {
                Ok(message) =>
                {
                    if !self.process_message_inner(create_info, message) && !self.is_loading
                    {
                        return;
                    }
                },
                Err(TryRecvError::Empty) =>
                {
                    return;
                },
                Err(err) =>
                {
                    eprintln!("error getting message: {err}");
                    self.running = false;
                    return;
                }
            }
        }
    }

    pub fn is_paused(&self) -> bool
    {
        self.is_paused
    }

    pub fn pause(&mut self)
    {
        self.is_paused = !self.is_paused;

        self.ui.borrow_mut().set_paused(self.is_paused);
    }

    pub fn is_loading(&self) -> bool
    {
        self.is_loading
    }

    pub fn update_loading(&mut self)
    {
        if self.is_loading
        {
            if DebugConfig::is_enabled(DebugTool::LoadPosition)
            {
                eprintln!("client: {}", self.world.camera_position());
            }

            let (exists, missing) = self.world.exists_missing();

            let is_loading = if DebugConfig::is_enabled(DebugTool::SkipLoading)
            {
                false
            } else
            {
                missing != 0 || !self.connected_and_ready
            };

            let loading = is_loading.then(||
            {
                let world_progress = exists as f32 / (exists + missing) as f32;

                let f = 0.5;

                if self.connected_and_ready
                {
                    f + world_progress * (1.0 - f)
                } else
                {
                    world_progress * f
                }
            });

            self.set_loading(loading);
        }
    }

    fn process_message_inner(&mut self, create_info: &mut UpdateBuffersInfo, message: Message) -> bool
    {
        let is_chunk_sync = matches!(message, Message::ChunkSync{..});

        if DebugConfig::is_enabled(DebugTool::ClientMessages)
        {
            if DebugConfig::is_enabled(DebugTool::ClientMessagesFull)
            {
                eprintln!("client {message:#?}");
            } else
            {
                eprintln!("client message: {}", <&str>::from(&message));
            }
        }

        let message = crate::frame_time_this!{
            [update, game_state_update, process_messages] -> world_handle_message,
            some_or_value!{self.world.handle_message(&mut self.delayed_messages, self.is_trusted, message), true}
        };

        let message = some_or_value!{
            self.entities.handle_message(&mut self.connections_handler, create_info, message, self.is_trusted),
            true
        };

        match message
        {
            Message::PlayerFullyConnected =>
            {
                self.notifications.set(Notification::PlayerConnected);
            },
            Message::SetTrusted =>
            {
                self.is_trusted = true;
            },
            Message::RepeatMessage{message} => self.connections_handler.send_message(*message),
            #[cfg(debug_assertions)]
            Message::DebugMessage(_) => (),
            x => panic!("unhandled message: {x:?}")
        }

        // multiple chunk syncs in a single frame would cause stutters, i dont like those >:(
        !is_chunk_sync
    }

    fn set_loading(&mut self, value: Option<f32>)
    {
        self.is_loading = value.is_some();

        self.ui.borrow_mut().set_loading(value);
    }

    fn check_resize_camera(&mut self, dt: f32)
    {
        const ZOOM_SPEED: f32 = 2.0;

        if self.pressed(Control::ZoomIn)
        {
            self.resize_camera(1.0 - dt * ZOOM_SPEED);
        } else if self.pressed(Control::ZoomOut)
        {
            self.resize_camera(1.0 + dt * ZOOM_SPEED);
        } else if self.pressed(Control::ZoomReset)
        {
            self.set_camera_scale(DEFAULT_ZOOM);
        }
    }

    fn resize_camera(&mut self, factor: f32)
    {
        let _max_scale = World::zoom_limit(); // maybe i would wanna use this??

        self.camera_scale *= factor;
        if !self.debug_mode
        {
            self.camera_scale = self.camera_scale.clamp(0.2, DEFAULT_ZOOM);
        }

        self.set_camera_scale(self.camera_scale);
    }

    fn set_camera_scale(&mut self, scale: f32)
    {
        {
            self.camera_scale = scale;
            let mut camera = self.camera.write();

            camera.rescale(scale);
        }

        self.camera_resized();
    }

    fn camera_resized(&mut self)
    {
        let size = self.camera.read().size();

        if !self.debug_visibility.is_detached()
        {
            self.world.rescale(size);
        }

        self.entities.update_resize(size);
    }

    pub fn echo_message(&mut self, message: Message)
    {
        let message = Message::RepeatMessage{message: Box::new(message)};

        self.send_message(message);
    }

    pub fn send_message(&mut self, message: Message)
    {
        self.connections_handler.send_message(message);
    }

    pub fn tile(&self, index: TilePos) -> Option<&Tile>
    {
        self.world.tile(index)
    }

    pub fn destroy_tile(&mut self, tile: TilePos)
    {
        self.world.set_tile(&mut self.connections_handler, tile, Tile::none());
    }

    pub fn player_connected(&mut self) -> bool
    {
        self.notifications.get(Notification::PlayerConnected)
    }

    pub fn update_buffers(
        &mut self,
        info: &mut UpdateBuffersInfo
    )
    {
        {
            let projection = info.projection_view;

            info.projection_view = Matrix4::identity();
            self.screen_object.update_buffers(info);

            info.projection_view = projection;
        }

        self.debug_visibility.update(&self.camera.read());

        let caster = self.entities.entities.transform(self.entities.follow_target()).map(|x| x.position)
            .unwrap_or_default();

        let caster = OccludingCaster::from(caster);

        let visibility = self.visibility_checker();

        crate::frame_time_this!{
            [update_buffers] -> world_update_buffers_normal,
            self.world.update_buffers(info)
        };

        crate::frame_time_this!{
            [update_buffers] -> world_update_buffers_shadows,
            self.world.update_buffers_shadows(info, &visibility, &caster)
        };

        if DebugConfig::is_enabled(DebugTool::DebugTileField)
        {
            self.world.debug_tile_field(&self.entities.entities);
        }

        crate::frame_time_this!{
            [update_buffers] -> entities_update_buffers,
            self.entities.update_buffers(&visibility, info, &caster, &mut self.world)
        };

        {
            info.update_camera(&self.ui_camera);

            let mut ui = self.ui.borrow_mut();

            if let Some(dt) = self.dt
            {
                ui.create_renders(info, dt);
            }

            ui.update_buffers(info);
        }
    }

    pub fn draw(&self, info: &mut DrawInfo)
    {
        let animation = self.entities.animation.sin();

        let draw_entities = render_system::DrawEntities{
            solid: &self.screen_object,
            renders: &self.entities.visible_renders,
            above_world: &self.entities.above_world_renders,
            occluders: &self.entities.occluders,
            shaded_renders: &self.entities.shaded_renders,
            light_renders: &self.entities.light_renders,
            world: &self.world
        };

        let info = render_system::DrawingInfo{
            shaders: &self.shaders,
            info,
            timestamp_query: self.timestamp_query.clone()
        };

        let sky_colors = {
            let light = self.world.sky_light();

            render_system::SkyColors{
                light_color: light.light_color()
            }
        };

        render_system::draw(
            &self.entities.entities,
            &self.ui.borrow(),
            draw_entities,
            info,
            sky_colors,
            animation
        );
    }

    pub fn before_render_pass(&mut self, object_info: &mut UpdateBuffersInfo)
    {
        if DebugConfig::is_enabled(DebugTool::GpuDrawTimings)
        {
            self.timestamp_query.setup(object_info);
        }
    }

    pub fn render_pass_ended(&mut self)
    {
        if DebugConfig::is_enabled(DebugTool::GpuDrawTimings)
        {
            #[cfg(debug_assertions)]
            {
                let mut results = self.timestamp_query.get_results().into_iter();

                let start = some_or_return!(results.next().unwrap()) as i64;

                let mut last = start;
                results.enumerate().filter_map(|(index, x)|
                {
                    x.map(|x| (index, x as i64))
                }).for_each(|(index, x)|
                {
                    let us_from = |last|
                    {
                        let ns = (x - last) as f32 * self.timestamp_query.period;
                        ns * 0.001
                    };

                    let last_us = us_from(last);
                    let total_us = us_from(start);

                    last = x;

                    eprintln!("gpu draw timing #{index}: {last_us:.1} us (total {total_us:.2} us)");
                });
            }
        }
    }

    fn visibility_checker(&self) -> VisibilityChecker
    {
        let camera = self.camera.read();
        let camera = if <DebugVisibility as DebugVisibilityTrait>::as_bool()
        {
            self.debug_visibility.camera()
        } else
        {
            &camera
        };

        let size2d = camera.size();

        let z_low = -1.0;
        let z_high = 0.0;

        let size = Vector3::new(size2d.x, size2d.y, z_high - z_low);

        let z_middle = (z_low + z_high) / 2.0;

        let mut position = camera.position().coords;

        let world_position = TilePos::from(position);

        position.z += z_middle;

        VisibilityChecker{
            world_position,
            size,
            position
        }
    }

    pub fn on_player_connected(&mut self)
    {
        self.connected_and_ready = true;
    }

    pub fn update_pre(&mut self, dt: f32)
    {
        self.check_resize_camera(dt);

        crate::frame_time_this!{
            [update, update_pre] -> world_update,
            self.world.update(&mut self.connections_handler, dt)
        };

        if self.connected_and_ready && !self.is_paused
        {
            self.entities.update(
                &mut self.world,
                &mut self.connections_handler,
                &self.loot,
                &self.common_textures,
                self.is_trusted,
                dt
            );
        }
    }

    fn get_dt(&self) -> f32
    {
        self.dt.unwrap_or_else(|| 1.0 / 60.0)
    }

    pub fn no_update(&mut self)
    {
        if DebugConfig::is_disabled(DebugTool::FreezeUi)
        {
            self.ui_update();
        } else
        {
            self.dt = None;
        }
    }

    pub fn ui_update(&mut self) -> Vec<(Control, ControlState)>
    {
        let mut changed_this_frame = self.controls.changed_this_frame();

        self.ui.borrow_mut().update(&self.entities.entities, &mut changed_this_frame, self.get_dt());

        self.controls.consume_changed(changed_this_frame).collect()
    }

    pub fn update(
        &mut self,
        object_info: &mut UpdateBuffersInfo,
        dt: f32
    )
    {
        crate::frame_time_this!{
            [update, game_state_update] -> before_render_pass,
            self.before_render_pass(object_info)
        };

        self.dt = Some(dt);

        crate::frame_time_this!{
            [update, game_state_update] -> process_messages,
            self.process_messages(object_info)
        };

        if !self.is_paused
        {
            let assets = object_info.partial.assets.clone();
            let partial = PartialCombinedInfo{
                world: &self.world,
                assets: &assets,
                passer: &self.connections_handler,
                common_textures: &self.common_textures,
                characters_info: &self.data_infos.characters_info,
                items_info: &self.data_infos.items_info
            };

            crate::frame_time_this!{
                [update, game_state_update] -> characters_update,
                self.entities.entities.update_characters(
                    partial,
                    object_info,
                    dt
                )
            };

            crate::frame_time_this!{
                [update, game_state_update] -> watchers_update,
                self.entities.entities.update_watchers(dt)
            };

            crate::frame_time_this!{
                [update, game_state_update] -> create_queued,
                self.entities.entities.create_queued(object_info)
            };

            if self.is_trusted
            {
                crate::frame_time_this!{
                    [update, game_state_update] -> sync_changed,
                    self.entities.entities.sync_changed(&mut self.connections_handler)
                };
            }

            crate::frame_time_this!{
                [update, game_state_update] -> handle_on_change,
                self.entities.entities.handle_on_change()
            };

            crate::frame_time_this!{
                [update, game_state_update] -> create_render_queued,
                self.entities.entities.create_render_queued(object_info)
            };
        }

        self.sync_character_delay -= dt;

        if self.rare_timer <= 0.0
        {
            crate::frame_time_this!{
                [update, game_state_update] -> rare,
                self.rare()
            };

            self.rare_timer = env::var("STEPHANIE_RARE_TIMER")
                .map(|x| x.parse().unwrap())
                .unwrap_or(5.0);
        } else
        {
            self.rare_timer -= dt;
        }
    }

    fn rare(&mut self)
    {
        if self.is_trusted
        {
            self.send_message(Message::SyncWorldTime{time: self.world.time()});
        }

        if DebugConfig::is_disabled(DebugTool::NoDebugChecks)
        {
            self.entities.entities.check_guarantees();
        }
    }

    pub fn input(&mut self, control: yanyaengine::Control) -> bool
    {
        let debug_control = control.clone();

        self.controls.handle_input(control);

        if self.ui.borrow().is_input_captured()
        {
            return true;
        }

        if self.debug_visibility.input(&debug_control) { return true; };

        false
    }

    pub fn pressed(&self, control: Control) -> bool
    {
        self.controls.is_down(control)
    }

    pub fn mouse_moved(&mut self, position: Vector2<f32>)
    {
        self.mouse_position = position;

        let normalized_size = self.camera.read().normalized_size();
        let position = position.component_mul(&normalized_size) - (normalized_size / 2.0);

        self.ui.borrow_mut().set_mouse_position(position);
    }

    pub fn mouse_offset(&self) -> Vector2<f32>
    {
        self.mouse_position - Vector2::repeat(0.5)
    }

    pub fn world_mouse_position(&self) -> Vector2<f32>
    {
        self.mouse_offset().component_mul(&self.camera.read().size())
    }

    pub fn ui_mouse_position(&self) -> Vector2<f32>
    {
        self.mouse_offset().component_mul(&self.ui_camera.size())
    }

    pub fn camera_moved(&mut self, position: Pos3<f32>)
    {
        if !self.debug_visibility.is_detached()
        {
            self.world.camera_moved(position, ||
            {
                self.connections_handler.send_message(Message::SyncCamera{position});
            });
        }
    }

    pub fn resize(&mut self, aspect: f32)
    {
        let mut camera = self.camera.write();
        camera.resize(aspect);
        self.ui_camera.resize(aspect);

        let size = camera.size();
        drop(camera);

        if !self.debug_visibility.is_detached()
        {
            self.world.rescale(size);
        }

        self.entities.update_aspect(size);
    }
}

impl EntitiesController for GameState
{
    type Container = ClientEntitiesContainer;
    type Passer = ConnectionsHandler;

    fn container_ref(&self) -> &Self::Container
    {
        &self.entities
    }

    fn container_mut(&mut self) -> &mut Self::Container
    {
        &mut self.entities
    }
}
