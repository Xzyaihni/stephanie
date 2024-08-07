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

use nalgebra::Vector2;

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
    ProgramShaders,
    client::{ui_element::*, RenderCreateInfo},
    common::{
        some_or_return,
        sender_loop,
        receiver_loop,
        render_info::*,
        lazy_transform::*,
        TileMap,
        DataInfos,
        Item,
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
        entity::ClientEntities,
        message::Message,
        character::PartialCombinedInfo,
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

pub use ui::{close_ui, Ui};
use ui::{BarNotification, TextNotification, NotificationId};

mod controls_controller;

mod notifications;

mod entity_creator;
mod ui;


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
    player_entity: Entity,
    positions_sync: f32,
    animation: f32
}

impl ClientEntitiesContainer
{
    pub fn new(infos: DataInfos, player_entity: Entity) -> Self
    {
        let mut entities = Entities::new(infos);

        let camera_entity = entities.push_eager(true, EntityInfo{
            transform: Some(Transform::default()),
            follow_position: Some(FollowPosition::new(
                player_entity,
                Connection::EaseOut{decay: 5.0, limit: None}
            )),
            ..Default::default()
        });

        Self{
            entities,
            camera_entity,
            player_entity,
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

    pub fn update_objects(
        &mut self,
        create_info: &mut RenderCreateInfo,
        dt: f32
    )
    {
        self.entities.create_queued(create_info);
        self.entities.create_render_queued(create_info);

        self.entities.update_watchers(dt);
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
        self.entities.update_physical(world, dt);
        self.entities.update_lazy(dt);
        self.entities.update_enemy(passer, dt);
        self.entities.update_children();

        self.entities.update_damaging(passer, damage_info);

        self.entities.update_lazy_mix(dt);

        self.entities.update_outlineable(dt);

        self.entities.update_colliders(world);

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
        casters: &OccludingCaster
    )
    {
        self.entities.update_render();

        self.entities.render.iter_mut().for_each(|(_, render)|
        {
            render.get_mut().update_buffers(visibility, info);
        });

        self.entities.occluding_plane.iter_mut().for_each(|(_, plane)|
        {
            plane.get_mut().update_buffers(visibility, info, casters);
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

        let ui_end = some_or_return!(self.entities.render.iter().rev().find_map(|(id, render)|
        {
            (render.get().z_level() < ZLevel::UiLow).then_some(id)
        }));

        let (normal, ui) = self.entities.render.split_at(ui_end + 1);

        let animation = self.animation.sin();

        normal.iter().filter_map(|x| x.as_ref()).for_each(|render|
        {
            render.get().draw(visibility, info, animation);
        });

        info.bind_pipeline(shaders.ui);
        info.set_depth_test(false);
        ui.iter().filter_map(|x| x.as_ref()).for_each(|render|
        {
            render.get().draw(visibility, info, animation);
        });

        info.set_depth_test(true);

        info.bind_pipeline(shaders.shadow);
        self.entities.occluding_plane.iter().for_each(|(_, x)|
        {
            x.get().draw(visibility, info);
        });
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowWhich
{
    ItemInfo,
    Inventory(InventoryWhich)
}

#[derive(Debug, Clone)]
pub enum UserEvent
{
    Popup{anchor: Entity, responses: Vec<UserEvent>},
    Info{which: InventoryWhich, item: InventoryItem},
    Drop{which: InventoryWhich, item: InventoryItem},
    Close(WindowWhich),
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
            Self::Close(..) => "close",
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
    pub stamina: NotificationId,
    pub weapon_cooldown: NotificationId,
    pub tile_tooltip: NotificationId
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
    shaders: ProgramShaders,
    host: bool,
    is_trusted: bool,
    camera_scale: f32,
    dt: f32,
    rare_timer: f32,
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

        let mut ui = {
            let camera_entity = entities.camera_entity;

            Ui::new(
                &mut entities.entity_creator(),
                info.data_infos.items_info.clone(),
                camera_entity,
                user_receiver.clone()
            )
        };

        let ui_notifications = {
            let mut creator = entities.entity_creator();

            let mut create_bar = |name: &str| -> ui::Notification
            {
                BarNotification::new(&mut creator, player_entity, name.to_owned()).into()
            };

            UiNotifications{
                stamina: ui.push_notification(create_bar("STAMINA")),
                weapon_cooldown: ui.push_notification(create_bar("WEAPON")),
                tile_tooltip: ui.push_notification(
                    TextNotification::new(&mut creator, player_entity, "undefined".to_owned())
                        .into()
                )
            }
        };

        let ui = Rc::new(RefCell::new(ui));

        let assets = info.object_info.partial.assets;
        let common_textures = CommonTextures::new(&mut assets.lock());

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
            shaders: info.shaders,
            world,
            debug_mode: info.client_info.debug_mode,
            tilemap,
            camera_scale: 1.0,
            dt: 0.0,
            rare_timer: 0.0,
            ui,
            common_textures,
            connected_and_ready: false,
            host: info.host,
            is_trusted: false,
            user_receiver,
            connections_handler,
            receiver_handle,
            receiver
        };

        {
            let aspect = this.camera.read().aspect();

            this.set_camera_scale(1.6);

            this.resize(aspect);
            this.camera_resized();
        }

        Rc::new(RefCell::new(this))
    }

    pub fn sync_transform(&mut self, entity: Entity)
    {
        let transform = self.entities().transform(entity).unwrap().clone();

        self.send_message(Message::SetTransform{entity, component: transform});
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
            self.set_camera_scale(1.0);
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
        self.world.rescale(size);

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

    pub fn create_popup(&mut self, anchor: Entity, responses: Vec<UserEvent>)
    {
        let distance = {
            let mouse_position = self.world_mouse_position();

            let transform_of = |entity|
            {
                self.entities.entities.transform(entity)
            };

            let camera_position = some_or_return!(transform_of(self.entities.camera_entity))
                .position
                .xy();

            let query = UiQuery{
                transform: some_or_return!(transform_of(anchor)),
                camera_position
            };

            -query.distance(mouse_position.xy())
        };

        let mut creator = EntityCreator{
            entities: &mut self.entities.entities
        };

        self.ui.borrow_mut().create_popup(
            distance,
            &mut creator,
            self.user_receiver.clone(),
            anchor,
            responses
        );
    }

    pub fn close_popup(&mut self)
    {
        self.ui.borrow_mut().close_popup(&mut self.entities.entities);
    }

    pub fn create_info_window(&mut self, item: Item)
    {
        let mut creator = EntityCreator{
            entities: &mut self.entities.entities
        };

        self.ui.borrow_mut().create_info_window(&mut creator, item);
    }

    pub fn close_info_window(&mut self)
    {
        self.ui.borrow_mut().close_info_window(&mut self.entities.entities);
    }

    pub fn set_bar(&self, id: NotificationId, amount: f32)
    {
        self.ui.borrow_mut().set_bar(&self.entities.entities, id, amount);
    }

    pub fn set_notification_text(&self, id: NotificationId, text: String)
    {
        self.ui.borrow_mut().set_notification_text(&self.entities.entities, id, text);
    }

    pub fn activate_notification(&self, id: NotificationId, delay: f32)
    {
        self.ui.borrow_mut().activate_notification(&self.entities.entities, id, delay);
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
        partial_info: UpdateBuffersPartialInfo
    )
    {
        let mut info = UpdateBuffersInfo::new(partial_info, &self.camera.read());
        let info = &mut info;

        let caster = self.entities.player_transform().map(|x| x.position)
            .unwrap_or_default();

        let caster = OccludingCaster::from(caster);

        let visibility = self.visibility_checker();

        self.world.update_buffers(info, &visibility, &caster);

        let mut create_info = RenderCreateInfo{
            location: UniformLocation{set: 0, binding: 0},
            shader: self.shaders.default,
            squares,
            object_info: &mut info.object_info
        };

        self.process_messages(&mut create_info);

        let partial = PartialCombinedInfo{
            passer: &self.connections_handler,
            common_textures: &self.common_textures,
            characters_info: &self.characters_info,
            items_info: &self.items_info
        };

        self.entities.entities.update_characters(
            partial,
            &mut create_info,
            self.dt
        );

        self.entities.update_objects(
            &mut create_info,
            self.dt
        );

        self.entities.update_buffers(&visibility, info, &caster);

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

        VisibilityChecker{
            size: camera.size(),
            position: camera.position().coords
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
            &self.camera.read(),
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

    pub fn update(&mut self, dt: f32)
    {
        self.entities.entities.update_ui(
            self.camera.read().position().coords.xy(),
            UiEvent::MouseMove(self.world_mouse_position())
        );

        self.ui.borrow_mut().update_after(&mut self.entities.entity_creator(), &self.camera.read());

        self.dt = dt;

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
        if cfg!(debug_assertions)
        {
            self.entities.entities.check_guarantees();
        }
    }

    pub fn input(&mut self, control: yanyaengine::Control)
    {
        self.controls.handle_input(control);
    }

    pub fn pressed(&self, control: Control) -> bool
    {
        self.controls.is_down(control)
    }

    pub fn mouse_moved(&mut self, position: Vector2<f32>)
    {
        self.mouse_position = position;
    }

    pub fn world_mouse_position(&self) -> Vector2<f32>
    {
        let camera_size = self.camera.read().size();

        (self.mouse_position - Vector2::repeat(0.5)).component_mul(&camera_size)
    }

    pub fn camera_moved(&mut self, position: Pos3<f32>)
    {
        self.world.camera_moved(position);
    }

    pub fn resize(&mut self, aspect: f32)
    {
        let mut camera = self.camera.write();
        camera.resize(aspect);

        let size = camera.size();
        drop(camera);

        self.world.rescale(size);

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
