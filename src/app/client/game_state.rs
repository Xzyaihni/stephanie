use std::{
    f32,
    mem,
    env,
    thread::JoinHandle,
    cell::{Ref, RefCell},
    rc::Rc,
    ops::ControlFlow,
    sync::{
        Arc,
        mpsc::{self, TryRecvError, Receiver}
    },
    collections::HashMap
};

use parking_lot::{RwLock, Mutex};

use nalgebra::{Vector2, Vector3};

use serde::{Serialize, Deserialize};

use yanyaengine::{
    Assets,
    ObjectFactory,
    Transform,
    TextureId,
    ModelId,
    UniformLocation,
    camera::Camera,
    object::model::Uvs,
    game_object::*
};

use crate::{
    debug_config::*,
    ProgramShaders,
    client::RenderCreateInfo,
    common::{
        some_or_return,
        sender_loop,
        receiver_loop,
        render_info::*,
        lazy_transform::*,
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
        entity::{for_each_component, render_system, ClientEntities},
        world::{
            World,
            Pos3,
            Tile,
            TilePos
        }
    }
};

use super::{
    UiEvent,
    ClientInfo,
    MessagePasser,
    ConnectionsHandler,
    TilesFactory,
    VisibilityChecker,
    world_receiver::WorldReceiver
};

pub use controls_controller::{Control, ControlState, KeyMapping};
pub use entity_creator::EntityCreator;

use controls_controller::ControlsController;

use notifications::{Notifications, Notification};

pub use ui::{
    close_ui,
    Ui,
    UiSpecializedWindow,
    WindowCreateInfo,
    WindowError,
    WindowType
};

use ui::{NotificationCreateInfo, NotificationKind};

mod controls_controller;

mod notifications;

mod entity_creator;
mod ui;


const DEFAULT_ZOOM: f32 = 2.3;

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
    visible_renders: Vec<Entity>,
    ui_renders: Vec<(Entity, bool)>,
    player_entity: Entity,
    positions_sync: f32,
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
            ui_renders: Vec::new(),
            positions_sync: 0.0,
            animation: 0.0
        }
    }
    
    pub fn handle_message(
        &mut self,
        create_info: &mut RenderCreateInfo,
        message: Message
    ) -> Option<Message>
    {
        self.entities.handle_message(create_info, message)
    }

    pub fn entity_creator(&mut self) -> EntityCreator
    {
        EntityCreator{
            entities: &mut self.entities
        }
    }

    pub fn update(
        &mut self,
        world: &World,
        passer: &mut impl EntityPasser,
        damage_info: TextureId,
        is_trusted: bool,
        dt: f32
    )
    {
        let mut space = SpatialGrid::new();
        self.entities.build_space(&mut space);

        self.entities.update_physical(world, dt);
        self.entities.update_lazy(dt);
        self.entities.update_enemy(passer, dt);
        self.entities.update_children();

        self.entities.update_damaging(passer, damage_info);

        self.entities.update_lazy_mix(dt);

        self.entities.update_outlineable(dt);

        self.entities.update_colliders(world, &space, dt);

        if is_trusted
        {
            if self.positions_sync <= 0.0
            {
                self.entities.sync_physical_positions(passer);

                self.positions_sync = 1.0;
            }

            self.positions_sync -= dt;
        }

        self.animation = (self.animation + dt) % (f32::consts::PI * 2.0);
    }

    pub fn update_resize(&mut self, size: Vector2<f32>)
    {
        self.entities.target(self.camera_entity).unwrap().scale = size.xyx();
    }

    pub fn update_aspect(&mut self, size: Vector2<f32>, aspect: f32)
    {
        self.update_resize(size);

        self.entities.update_ui_aspect(aspect);
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
        ui_camera: &Camera,
        caster: &OccludingCaster
    )
    {
        self.visible_renders.clear();
        self.ui_renders.clear();

        let mut world_ui = Vec::new();
        for_each_component!(self.entities, render, |entity, render: &RefCell<ClientRenderInfo>|
        {
            let transform = some_or_return!(self.entities.transform(entity));

            let render = render.borrow();
            if !render.visible_with(visibility, &transform)
            {
                return;
            }

            if render.z_level() >= ZLevel::UiLow
            {
                let is_world = self.entities.ui_element(entity).map(|x| x.world_position)
                    .unwrap_or(false);

                if is_world
                {
                    world_ui.push(entity);
                }

                self.ui_renders.push((entity, is_world));
            } else
            {
                self.visible_renders.push(entity);
            }
        });

        render_system::update_buffers(
            &self.entities,
            self.visible_renders.iter().copied().chain(world_ui),
            info,
            caster
        );

        info.update_camera(ui_camera);

        self.ui_renders.iter().for_each(|&(entity, is_world)|
        {
            if is_world
            {
                return;
            }

            let transform = self.entities.transform(entity).unwrap().clone();
            let mut render = self.entities.render_mut(entity).unwrap();

            render.set_transform(transform);
            render.update_buffers(info);
        });
    }

    pub fn draw(
        &self,
        visibility: &VisibilityChecker,
        info: &mut DrawInfo,
        shaders: &ProgramShaders
    )
    {
        if !self.player_exists()
        {
            return;
        }

        let animation = self.animation.sin();

        self.visible_renders.iter().for_each(|&entity|
        {
            self.entities.render(entity).unwrap().draw(visibility, info, animation);
        });

        info.set_depth_test(false);

        info.bind_pipeline(shaders.shadow);
        self.visible_renders.iter().filter_map(|entity|
        {
            self.entities.occluder(*entity)
        }).for_each(|occluder|
        {
            occluder.draw(info);
        });

        info.bind_pipeline(shaders.ui);

        self.ui_renders.iter().for_each(|&(entity, _)|
        {
            self.entities.render(entity).unwrap().draw(visibility, info, animation);
        });

        info.set_depth_test(true);
    }
}

pub struct GameStateInfo<'a>
{
    pub shaders: ProgramShaders,
    pub camera: Arc<RwLock<Camera>>,
    pub object_info: ObjectCreateInfo<'a>,
    pub data_infos: DataInfos,
    pub tiles_factory: TilesFactory,
    pub message_passer: MessagePasser,
    pub client_info: &'a ClientInfo,
    pub host: bool
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryWhich
{
    Player,
    Other
}

#[derive(Debug, Clone)]
pub enum UserEvent
{
    Popup{responses: Vec<UserEvent>},
    Info{which: InventoryWhich, item: InventoryItem},
    Drop{which: InventoryWhich, item: InventoryItem},
    Wield(InventoryItem),
    Take(InventoryItem)
}

impl UserEvent
{
    pub fn name(&self) -> &str
    {
        match self
        {
            Self::Popup{..} => "popup",
            Self::Info{..} => "info",
            Self::Drop{..} => "drop",
            Self::Wield(..) => "wield",
            Self::Take(..) => "take"
        }
    }
}

#[derive(Debug)]
pub struct UiReceiver
{
    events: Vec<UserEvent>
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

    pub fn push(&mut self, event: UserEvent)
    {
        self.events.push(event);
    }

    pub fn consume(&mut self) -> impl Iterator<Item=UserEvent>
    {
        mem::take(&mut self.events).into_iter()
    }
}

pub struct CommonTextures
{
    pub dust: TextureId,
    pub blood: TextureId
}

impl CommonTextures
{
    pub fn new(assets: &mut Assets) -> Self
    {
        Self{
            dust: assets.texture_id("decals/dust.png"),
            blood: assets.texture_id("decals/blood.png")
        }
    }
}

pub struct UiNotifications
{
    ui: Rc<RefCell<Ui>>,
    pub stamina: Option<WindowType>,
    pub weapon_cooldown: Option<WindowType>,
    pub tile_tooltip: Option<WindowType>
}

impl UiNotifications
{
    fn set_notification(
        notification: &mut Option<WindowType>,
        entities: &mut ClientEntities,
        ui: &Rc<RefCell<Ui>>,
        owner: Entity,
        lifetime: f32,
        update: impl FnOnce(&mut ClientEntities, &mut NotificationKind),
        create: impl FnOnce() -> NotificationCreateInfo
    )
    {
        if let Some(notification) = notification.as_ref().and_then(|x| x.upgrade())
        {
            let mut notification = notification.borrow_mut();
            let notification = notification.as_notification_mut().unwrap();

            notification.lifetime = lifetime;
            update(entities, &mut notification.kind);
        } else
        {
            let window = WindowCreateInfo::Notification{owner, lifetime, info: create()};

            let mut creator = EntityCreator{entities};
            let window = Ui::add_window(ui.clone(), &mut creator, window);
            *notification = Some(window);
        }
    }

    fn set_bar(
        id: &mut Option<WindowType>,
        entities: &mut ClientEntities,
        ui: &Rc<RefCell<Ui>>,
        owner: Entity,
        lifetime: f32,
        amount: f32,
        f: impl FnOnce() -> NotificationCreateInfo
    )
    {
        Self::set_notification(id, entities, ui, owner, lifetime, move |entities, kind|
        {
            kind.as_bar_mut().unwrap().set_amount(entities, amount);
        }, f)
    }

    fn set_text(
        id: &mut Option<WindowType>,
        entities: &mut ClientEntities,
        ui: &Rc<RefCell<Ui>>,
        owner: Entity,
        lifetime: f32,
        text: String,
        f: impl FnOnce() -> NotificationCreateInfo
    )
    {
        Self::set_notification(id, entities, ui, owner, lifetime, move |entities, kind|
        {
            kind.as_text_mut().unwrap().set_text(entities, text);
        }, f)
    }

    pub fn set_stamina_bar(
        &mut self,
        entities: &mut ClientEntities,
        owner: Entity,
        lifetime: f32,
        amount: f32
    )
    {
        Self::set_bar(&mut self.stamina, entities, &self.ui, owner, lifetime, amount, ||
        {
            NotificationCreateInfo::Bar{name: "STAMINA".to_owned(), amount}
        })
    }

    pub fn set_weapon_cooldown_bar(
        &mut self,
        entities: &mut ClientEntities,
        owner: Entity,
        lifetime: f32,
        amount: f32
    )
    {
        Self::set_bar(&mut self.weapon_cooldown, entities, &self.ui, owner, lifetime, amount, ||
        {
            NotificationCreateInfo::Bar{name: "WEAPON".to_owned(), amount}
        })
    }

    pub fn set_tile_tooltip_text(
        &mut self,
        entities: &mut ClientEntities,
        owner: Entity,
        lifetime: f32,
        text: String
    )
    {
        Self::set_text(&mut self.tile_tooltip, entities, &self.ui, owner, lifetime, text.clone(), ||
        {
            NotificationCreateInfo::Text{text}
        })
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
    fn new(_camera: &Camera) -> Self { () }

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
    pub ui_notifications: UiNotifications,
    pub entities: ClientEntitiesContainer,
    pub controls: ControlsController,
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
    ui_camera: Camera,
    shaders: ProgramShaders,
    host: bool,
    is_trusted: bool,
    camera_scale: f32,
    rare_timer: f32,
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
    pub fn new(info: GameStateInfo) -> Rc<RefCell<Self>>
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

        let entities = ClientEntitiesContainer::new(
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

        let ui = Ui::new(
            info.data_infos.items_info.clone(),
            user_receiver.clone()
        );

        let ui = Rc::new(RefCell::new(ui));

        let assets = info.object_info.partial.assets;
        let common_textures = CommonTextures::new(&mut assets.lock());

        let debug_visibility = <DebugVisibility as DebugVisibilityTrait>::State::new(
            &info.camera.read()
        );

        let ui_notifications = UiNotifications{
            ui: ui.clone(),
            stamina: None,
            weapon_cooldown: None,
            tile_tooltip: None
        };

        let ui_camera = Camera::new(1.0, -1.0..1.0);

        let mut this = Self{
            mouse_position,
            camera: info.camera,
            assets,
            object_factory: info.object_info.partial.object_factory,
            notifications,
            ui_notifications,
            entities,
            items_info: info.data_infos.items_info,
            characters_info: info.data_infos.characters_info,
            controls,
            running: true,
            ui_camera,
            shaders: info.shaders,
            world,
            debug_mode: info.client_info.debug,
            tilemap,
            camera_scale: 1.0,
            rare_timer: 0.0,
            ui,
            common_textures,
            connected_and_ready: false,
            host: info.host,
            is_trusted: false,
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
            self.send_message(Message::SetTarget{entity, target: target.clone()});
        }

        if let Some(character) = entities.character(entity)
        {
            self.send_message(Message::SyncCharacter{entity, info: character.get_sync_info()});
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

    fn damage_info(&self) -> TextureId
    {
        self.common_textures.blood
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

    pub fn entity_creator(&mut self) -> EntityCreator
    {
        self.entities.entity_creator()
    }

    pub fn process_messages(&mut self, create_info: &mut RenderCreateInfo)
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

    fn process_message_inner(&mut self, create_info: &mut RenderCreateInfo, message: Message)
    {
        let message = some_or_return!{self.entities.handle_message(create_info, message)};
        let message = some_or_return!{self.world.handle_message(message)};

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
        let (min_scale, max_scale) = World::zoom_limits();

        self.camera_scale *= factor;
        if !self.debug_mode
        {
            self.camera_scale = self.camera_scale.clamp(min_scale, max_scale);
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

        self.ui.borrow().update_resize(&self.entities.entities, size);
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

    pub fn create_popup(&mut self, responses: Vec<UserEvent>)
    {
        let popup_position = self.ui_mouse_position();

        let mut creator = EntityCreator{
            entities: &mut self.entities.entities
        };

        Ui::add_window(
            self.ui.clone(),
            &mut creator,
            WindowCreateInfo::ActionsList{popup_position, responses}
        );
    }

    pub fn close_popup(&mut self)
    {
        self.ui.borrow_mut().close_popup(&mut self.entities.entities);
    }

    pub fn add_window(&mut self, info: WindowCreateInfo) -> WindowType
    {
        let mut creator = EntityCreator{
            entities: &mut self.entities.entities
        };

        Ui::add_window(self.ui.clone(), &mut creator, info)
    }

    pub fn remove_window(
        &mut self,
        window: Rc<RefCell<UiSpecializedWindow>>
    ) -> Result<(), WindowError>
    {
        self.ui.borrow_mut().remove_window(&self.entities.entities, window)
    }

    pub fn tile(&self, index: TilePos) -> Option<&Tile>
    {
        self.world.tile(index)
    }

    pub fn tile_of(&self, position: Pos3<f32>) -> TilePos
    {
        self.world.tile_of(position)
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
        squares: &HashMap<Uvs, ModelId>,
        info: &mut UpdateBuffersInfo
    )
    {
        self.debug_visibility.update(&self.camera.read());

        let caster = self.entities.player_transform().map(|x| x.position)
            .unwrap_or_default();

        let caster = OccludingCaster::from(caster);

        let visibility = self.visibility_checker();

        self.world.update_buffers(info, &visibility, &caster);

        let mut create_info = RenderCreateInfo{
            location: UniformLocation{set: 0, binding: 0},
            shader: self.shaders.default,
            squares,
            object_info: info
        };

        self.entities.entities.create_render_queued(&mut create_info);

        self.entities.update_buffers(&visibility, info, &self.ui_camera, &caster);

        self.entities.entities.handle_on_change();
    }

    pub fn draw(&self, info: &mut DrawInfo)
    {
        info.bind_pipeline(self.shaders.world);

        let visibility = self.visibility_checker();

        self.world.draw(info, &visibility, self.shaders.shadow);

        info.bind_pipeline(self.shaders.default);

        self.entities.draw(&visibility, info, &self.shaders);

        info.set_depth_write(true);
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
        position.z += z_middle;

        VisibilityChecker{
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

        self.ui.borrow_mut().update(
            &mut self.entities.entity_creator(),
            &self.ui_camera,
            dt
        );

        if self.connected_and_ready
        {
            let mut passer = self.connections_handler.write();
            self.entities.update(
                &self.world,
                &mut *passer,
                self.damage_info(),
                self.is_trusted,
                dt
            );
        }
    }

    pub fn update(
        &mut self,
        squares: &HashMap<Uvs, ModelId>,
        object_info: &mut UpdateBuffersInfo,
        dt: f32
    )
    {
        self.entities.entities.update_ui(
            self.camera.read().position().coords.xy(),
            UiEvent::MouseMove(self.ui_mouse_position())
        );

        self.ui.borrow_mut().update_after(&mut self.entities.entity_creator(), &self.camera.read());

        let mut create_info = RenderCreateInfo{
            location: UniformLocation{set: 0, binding: 0},
            shader: self.shaders.default,
            squares,
            object_info
        };

        self.process_messages(&mut create_info);

        let assets = create_info.object_info.partial.assets.clone();
        let partial = PartialCombinedInfo{
            assets: &assets,
            passer: &self.connections_handler,
            common_textures: &self.common_textures,
            characters_info: &self.characters_info,
            items_info: &self.items_info
        };

        self.entities.entities.update_characters(
            partial,
            &mut create_info,
            dt
        );

        self.entities.entities.update_watchers(dt);

        self.entities.entities.create_queued(&mut create_info);

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

        self.controls.handle_input(control).is_some()
    }

    pub fn pressed(&self, control: Control) -> bool
    {
        self.controls.is_down(control)
    }

    pub fn mouse_moved(&mut self, position: Vector2<f32>)
    {
        self.mouse_position = position;
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

        self.ui.borrow().update_resize(&self.entities.entities, size);

        self.entities.update_aspect(size, aspect);
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
