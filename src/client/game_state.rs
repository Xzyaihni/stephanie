use std::{
    f32,
    mem,
    env,
    thread::JoinHandle,
    cell::{Ref, RefCell},
    rc::Rc,
    ops::ControlFlow,
    collections::{BTreeMap, btree_map::Entry},
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
        some_or_return,
        sender_loop,
        receiver_loop,
        render_info::*,
        lazy_transform::*,
        ClientLight,
        SpatialGrid,
        TileMap,
        DataInfos,
        ItemsInfo,
        InventoryItem,
        AnyEntities,
        CharactersInfo,
        Entity,
        EntityInfo,
        Entities,
        EntityPasser,
        EntitiesController,
        OccludingCaster,
        message::Message,
        character::PartialCombinedInfo,
        entity::{
            for_each_component,
            render_system,
            collider_system,
            physical_system,
            enemy_system,
            damaging_system,
            ClientEntities
        },
        world::{
            TILE_SIZE,
            World,
            Pos3,
            Tile,
            TilePos
        }
    }
};

use super::{
    ClientInfo,
    MessagePasser,
    ConnectionsHandler,
    TilesFactory,
    VisibilityChecker,
    world_receiver::WorldReceiver
};

pub use controls_controller::{ControlsController, UiControls, Control, ControlState, KeyMapping};

use notifications::{Notifications, Notification};

pub use anatomy_locations::UiAnatomyLocations;
pub use ui::{Ui, UiId, NotificationInfo, NotificationKindInfo};

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
    pub follow_entity: Entity,
    visible_renders: Vec<Vec<Entity>>,
    above_world_renders: Vec<Entity>,
    light_renders: Vec<Entity>,
    shaded_renders: Vec<Vec<Entity>>,
    player_entity: Entity,
    animation: f32
}

impl ClientEntitiesContainer
{
    pub fn new(infos: DataInfos, player_entity: Entity) -> Self
    {
        let mut entities = Entities::new(infos);

        let follow_entity = entities.push_eager(true, EntityInfo{
            transform: Some(Transform::default()),
            ..Default::default()
        });

        let camera_entity = entities.push_eager(true, EntityInfo{
            transform: Some(Transform::default()),
            follow_position: Some(FollowPosition::new(
                follow_entity,
                Connection::EaseOut{decay: 5.0, limit: None}
            )),
            ..Default::default()
        });

        Self{
            entities,
            camera_entity,
            follow_entity,
            player_entity,
            visible_renders: Vec::new(),
            above_world_renders: Vec::new(),
            light_renders: Vec::new(),
            shaded_renders: Vec::new(),
            animation: 0.0
        }
    }

    pub fn handle_message(
        &mut self,
        create_info: &mut UpdateBuffersInfo,
        message: Message
    ) -> Option<Message>
    {
        self.entities.handle_message(create_info, message)
    }

    pub fn update<Passer: EntityPasser>(
        &mut self,
        world: &mut World,
        passer: &RwLock<Passer>,
        damage_info: &CommonTextures,
        _is_trusted: bool,
        dt: f32
    )
    {
        let mut space = SpatialGrid::new();

        crate::frame_time_this!{
            spatial_grid_build,
            self.entities.build_space(&mut space)
        };

        crate::frame_time_this!{
            physical_system_update,
            physical_system::update(&mut self.entities, world, dt)
        };

        crate::frame_time_this!{
            lazy_transform_update,
            self.entities.update_lazy(dt)
        };

        crate::frame_time_this!{
            enemy_system_update,
            enemy_system::update(&mut self.entities, world, passer, dt)
        };

        crate::frame_time_this!{
            children_update,
            self.entities.update_children()
        };

        crate::frame_time_this!{
            damaging_system_update,
            damaging_system::update(&mut self.entities, world, passer, damage_info)
        };

        crate::frame_time_this!{
            lazy_mix_update,
            self.entities.update_lazy_mix(dt)
        };

        crate::frame_time_this!{
            outlineable_update,
            self.entities.update_outlineable(dt)
        };

        crate::frame_time_this!{
            collider_system_update,
            collider_system::update(&mut self.entities, world, &space, dt)
        };

        self.animation = (self.animation + dt) % (f32::consts::PI * 2.0);
    }

    pub fn update_resize(&mut self, size: Vector2<f32>)
    {
        self.entities.target(self.camera_entity).unwrap().scale = size.xyx();
    }

    pub fn update_aspect(&mut self, size: Vector2<f32>)
    {
        self.update_resize(size);
    }

    pub fn main_player(&self) -> Entity
    {
        self.player_entity
    }

    pub fn player_transform(&self) -> Option<Ref<Transform>>
    {
        self.entities.transform(self.main_player())
    }

    pub fn player_exists(&self) -> bool
    {
        self.entities.exists(self.player_entity)
    }

    fn update_buffers(
        &mut self,
        visibility: &VisibilityChecker,
        info: &mut UpdateBuffersInfo,
        caster: &OccludingCaster,
        world: &mut World
    )
    {
        fn insert_render<V>(renders: &mut BTreeMap<i32, Vec<V>>, value: V, key: i32)
        {
            match renders.entry(key)
            {
                Entry::Vacant(entry) => { entry.insert(vec![value]); },
                Entry::Occupied(mut entry) => entry.get_mut().push(value)
            }
        }

        self.above_world_renders.clear();
        self.light_renders.clear();

        let mut shaded_renders = BTreeMap::new();
        let mut visible_renders = BTreeMap::new();
        for_each_component!(self.entities, render, |entity, render: &RefCell<ClientRenderInfo>|
        {
            let transform = some_or_return!(self.entities.transform(entity));

            let mut render = render.borrow_mut();

            // uses transform because update buffers might not be called and transforms not synced
            if !render.visible_with(visibility, &transform)
            {
                return;
            }

            if DebugConfig::is_enabled(DebugTool::Sleeping)
            {
                if let Some(physical) = self.entities.physical(entity)
                {
                    let color = if physical.sleeping()
                    {
                        [0.2, 0.2, 1.0, 1.0]
                    } else
                    {
                        [0.2, 1.0, 0.2, 1.0]
                    };

                    render.mix = Some(MixColor{color, amount: 0.7, keep_transparency: true});
                }
            }

            let is_render_above = render.above_world;

            let mut update_buffers = |entities: &ClientEntities, render: &mut ClientRenderInfo|
            {
                render.set_transform(transform.clone());
                render.update_buffers(info);

                if let Some(mut occluder) = entities.occluder_mut_no_change(entity)
                {
                    occluder.set_transform(transform.clone());
                    occluder.update_buffers(info, caster);
                }
            };

            if is_render_above
            {
                self.above_world_renders.push(entity);
                update_buffers(&self.entities, &mut render);
            } else
            {
                let real_z = (transform.position.z / TILE_SIZE).floor() as i32;

                let below_player = !visibility.world_position.is_same_height(&TilePos::from(transform.position));
                let sky_occluded = below_player && world.sky_occluded(&transform);

                let is_render_visible = !world.wall_occluded(&transform) && !sky_occluded;

                let is_render_shadow = render.shadow_visible && !sky_occluded;

                if is_render_visible
                {
                    insert_render(&mut visible_renders, entity, real_z);
                }

                if is_render_shadow
                {
                    insert_render(&mut shaded_renders, entity, real_z);
                }

                if is_render_visible || is_render_shadow
                {
                    update_buffers(&self.entities, &mut render);
                }
            }
        });

        for_each_component!(self.entities, light, |entity, light: &RefCell<ClientLight>|
        {
            let transform = some_or_return!(self.entities.transform(entity));

            let mut light = light.borrow_mut();

            if !light.visible_with(visibility, &transform)
            {
                return;
            }

            let position = transform.position;

            let light_visibility = light.visibility_checker_with(position);

            let below_player = !visibility.world_position.is_same_height(&light_visibility.world_position);

            let light_transform = Transform{
                scale: light.scale(),
                ..*transform
            };

            if below_player
            {
                if world.sky_occluded(&light_transform)
                {
                    return;
                }
            }

            if world.wall_occluded(&light_transform)
            {
                return;
            }

            light.update_buffers(info, position);

            world.update_buffers_light_shadows(
                info,
                &light_visibility,
                &OccludingCaster::from(position),
                self.light_renders.len()
            );

            self.light_renders.push(entity);
        });

        self.shaded_renders = shaded_renders.into_values().collect();
        self.visible_renders = visible_renders.into_values().collect();
    }
}

pub struct GameStateInfo<'a>
{
    pub shaders: ProgramShaders,
    pub camera: Arc<RwLock<Camera>>,
    pub timestamp_query: TimestampQuery,
    pub object_info: ObjectCreateInfo<'a>,
    pub data_infos: DataInfos,
    pub tiles_factory: TilesFactory,
    pub message_passer: MessagePasser,
    pub client_info: &'a ClientInfo,
    pub host: bool
}

pub struct PartCreator<'a, 'b>
{
    resource_uploader: &'a mut ResourceUploader<'b>,
    assets: &'a mut Assets
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
    pub items_info: Arc<ItemsInfo>,
    pub characters_info: Arc<CharactersInfo>,
    pub user_receiver: Rc<RefCell<UiReceiver>>,
    pub ui: Rc<RefCell<Ui>>,
    pub common_textures: CommonTextures,
    pub connected_and_ready: bool,
    pub world: World,
    screen_object: SolidObject,
    ui_camera: Camera,
    timestamp_query: TimestampQuery,
    shaders: ProgramShaders,
    host: bool,
    is_trusted: bool,
    is_loading: bool,
    camera_scale: f32,
    rare_timer: f32,
    dt: Option<f32>,
    debug_visibility: <DebugVisibility as DebugVisibilityTrait>::State,
    connections_handler: Arc<RwLock<ConnectionsHandler>>,
    receiver_handle: Option<JoinHandle<()>>,
    receiver: Receiver<Message>
}

impl Drop for GameState
{
    fn drop(&mut self)
    {
        let mut writer = self.connections_handler.write();
        if let Err(err) = writer.send_blocking(&Message::PlayerDisconnect{host: self.host})
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
    pub fn new(mut info: GameStateInfo) -> Rc<RefCell<Self>>
    {
        let mouse_position = Vector2::zeros();

        let notifications = Notifications::new();
        let controls = ControlsController::new();

        let handler = ConnectionsHandler::new(info.message_passer);
        let connections_handler = Arc::new(RwLock::new(handler));

        let tilemap = info.tiles_factory.tilemap().clone();

        let world_receiver = WorldReceiver::new(connections_handler.clone());
        let world = World::new(
            world_receiver,
            info.tiles_factory,
            info.camera.read().size(),
            Pos3::new(0.0, 0.0, 0.0)
        );

        let player_entity = Self::connect_to_server(
            connections_handler.clone(),
            &info.client_info.name
        );

        let mut entities = ClientEntitiesContainer::new(
            info.data_infos.clone(),
            player_entity
        );

        let _sender_handle = sender_loop(connections_handler.clone());

        let handler = connections_handler.read().passer_clone();

        let (sender, receiver) = mpsc::channel();

        let receiver_handle = Some(receiver_loop(handler, move |message|
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

        let assets = info.object_info.partial.assets.clone();

        let screen_object = {
            let assets = assets.lock();

            info.object_info.partial.object_factory.create_solid(
                assets.model(assets.default_model(DefaultModel::Square)).clone(),
                Transform{scale: Vector3::repeat(2.0), ..Default::default()}
            )
        };

        let anatomy_locations = |object_info: &mut ObjectCreateInfo, name: &str| -> UiAnatomyLocations
        {
            let base_image = image::open(format!("textures/special/{name}.png"))
                .unwrap_or_else(|err|
                {
                    panic!("{name}.png must exist: {err}")
                });

            let mut assets = assets.lock();

            let part_creator = PartCreator{
                assets: &mut assets,
                resource_uploader: object_info.partial.builder_wrapper.resource_uploader_mut()
            };

            UiAnatomyLocations::new(part_creator, base_image)
        };

        let ui = Ui::new(
            info.data_infos.items_info.clone(),
            &mut info.object_info,
            &mut entities.entities,
            entities.camera_entity,
            anatomy_locations,
            user_receiver.clone()
        );

        let common_textures = CommonTextures::new(&mut assets.lock());

        let debug_visibility = <DebugVisibility as DebugVisibilityTrait>::State::new(
            &info.camera.read()
        );

        let ui_camera = Camera::new(1.0, -1.0..1.0);

        let mut this = Self{
            mouse_position,
            camera: info.camera,
            assets,
            object_factory: info.object_info.partial.object_factory,
            notifications,
            entities,
            items_info: info.data_infos.items_info,
            characters_info: info.data_infos.characters_info,
            controls,
            running: true,
            screen_object,
            ui_camera,
            timestamp_query: info.timestamp_query,
            shaders: info.shaders,
            world,
            debug_mode: info.client_info.debug,
            tilemap,
            camera_scale: 1.0,
            rare_timer: 0.0,
            dt: None,
            ui,
            common_textures,
            connected_and_ready: false,
            host: info.host,
            is_trusted: false,
            is_loading: true,
            user_receiver,
            debug_visibility,
            connections_handler,
            receiver_handle,
            receiver
        };

        {
            let aspect = this.camera.read().aspect();

            this.set_camera_scale(DEFAULT_ZOOM);

            this.resize(aspect);
            this.camera_resized();
        }

        Rc::new(RefCell::new(this))
    }

    pub fn sync_character(&mut self, entity: Entity)
    {
        let entities = self.entities();
        if let Some(target) = entities.target_ref(entity)
        {
            self.send_message(Message::SetTarget{entity, target: Box::new(target.clone())});
        }
    }

    fn connect_to_server(
        handler: Arc<RwLock<ConnectionsHandler>>,
        name: &str
    ) -> Entity
    {
        let mut handler = handler.write();

        let message = Message::PlayerConnect{name: name.to_owned()};
        if let Err(x) = handler.send_blocking(&message)
        {
            panic!("error connecting to server: {x}");
        }

        match handler.receive_blocking()
        {
            Ok(Some(Message::PlayerOnConnect{player_entity})) =>
            {
                player_entity
            },
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

    pub fn player(&self) -> Entity
    {
        self.entities.main_player()
    }

    pub fn process_messages(&mut self, create_info: &mut UpdateBuffersInfo)
    {
        loop
        {
            match self.receiver.try_recv()
            {
                Ok(message) =>
                {
                    self.process_message_inner(create_info, message);
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

    fn process_message_inner(&mut self, create_info: &mut UpdateBuffersInfo, message: Message)
    {
        if DebugConfig::is_enabled(DebugTool::ShowMessages)
        {
            eprintln!("{message:#?}");
        }

        let message = some_or_return!{self.entities.handle_message(create_info, message)};
        let message = if let Some(x) = self.world.handle_message(message)
        {
            x
        } else
        {
            if self.is_loading
            {
                let (exists, missing) = self.world.exists_missing();

                let is_loading = missing != 0 || !self.connected_and_ready;

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

            return;
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
            x => panic!("unhandled message: {x:?}")
        }
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

    pub fn echo_message(&self, message: Message)
    {
        let message = Message::RepeatMessage{message: Box::new(message)};

        self.send_message(message);
    }

    pub fn send_message(&self, message: Message)
    {
        self.connections_handler.write().send_message(message);
    }

    pub fn tile(&self, index: TilePos) -> Option<&Tile>
    {
        self.world.tile(index)
    }

    pub fn destroy_tile(&mut self, tile: TilePos)
    {
        self.world.set_tile(tile, Tile::none());
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

        let caster = self.entities.player_transform().map(|x| x.position)
            .unwrap_or_default();

        let caster = OccludingCaster::from(caster);

        let visibility = self.visibility_checker();

        self.world.update_buffers(info);
        self.world.update_buffers_shadows(info, &visibility, &caster);

        if DebugConfig::is_enabled(DebugTool::DrawTileOcclusion)
        {
            self.world.debug_tile_occlusion(&self.entities.entities);
        }

        self.entities.entities.create_render_queued(info);

        self.entities.update_buffers(&visibility, info, &caster, &mut self.world);

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
        let visibility = self.visibility_checker();

        let animation = self.entities.animation.sin();

        let draw_entities = render_system::DrawEntities{
            solid: &self.screen_object,
            renders: &self.entities.visible_renders,
            above_world: &self.entities.above_world_renders,
            shaded_renders: &self.entities.shaded_renders,
            light_renders: &self.entities.light_renders,
            world: &self.world
        };

        let info = render_system::DrawingInfo{
            shaders: &self.shaders,
            info,
            timestamp_query: self.timestamp_query.clone()
        };

        render_system::draw(
            &self.entities.entities,
            &self.ui.borrow(),
            draw_entities,
            info,
            &visibility,
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

        self.world.update(dt);

        if self.connected_and_ready
        {
            self.entities.update(
                &mut self.world,
                &self.connections_handler,
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
        self.dt = None;
    }

    pub fn ui_update(&mut self, controls: &mut UiControls<UiId>)
    {
        self.ui.borrow_mut().update(&self.entities.entities, controls, self.get_dt());
    }

    pub fn update(
        &mut self,
        object_info: &mut UpdateBuffersInfo,
        dt: f32
    )
    {
        self.before_render_pass(object_info);

        self.dt = Some(dt);

        crate::frame_time_this!{
            process_messages,
            self.process_messages(object_info)
        };

        let assets = object_info.partial.assets.clone();
        let partial = PartialCombinedInfo{
            world: &self.world,
            assets: &assets,
            passer: &self.connections_handler,
            common_textures: &self.common_textures,
            characters_info: &self.characters_info,
            items_info: &self.items_info
        };

        crate::frame_time_this!{
            characters_update,
            self.entities.entities.update_characters(
                partial,
                object_info,
                dt
            )
        };

        crate::frame_time_this!{
            watchers_update,
            self.entities.entities.update_watchers(dt)
        };

        crate::frame_time_this!{
            create_queued,
            self.entities.entities.create_queued(object_info)
        };

        crate::frame_time_this!{
            handle_on_change,
            self.entities.entities.handle_on_change()
        };

        if self.rare_timer <= 0.0
        {
            self.rare();

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
        if DebugConfig::is_debug()
        {
            self.entities.entities.check_guarantees();
        }
    }

    pub fn input(&mut self, control: yanyaengine::Control) -> bool
    {
        if self.debug_visibility.input(&control) { return true; };

        self.controls.handle_input(control);

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
            self.world.camera_moved(position);
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

    fn passer(&self) -> Arc<RwLock<Self::Passer>>
    {
        self.connections_handler.clone()
    }
}
