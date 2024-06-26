use std::{
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
    client::RenderCreateInfo,
    common::{
        some_or_return,
        sender_loop,
        receiver_loop,
        TileMap,
        DataInfos,
        ItemsInfo,
        InventoryItem,
        AnyEntities,
        CharactersInfo,
        Entity,
        Entities,
        EntityPasser,
        EntitiesController,
        OccludingCasters,
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

pub use ui::Ui;
use ui::InventoryActions;

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
    player_entity: Option<Entity>
}

impl ClientEntitiesContainer
{
    pub fn new() -> Self
    {
        Self{
            entities: Entities::new(),
            player_entity: None
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
        self.entities.update_physical(dt);
        self.entities.update_lazy(dt);
        self.entities.update_follows(dt);
        self.entities.update_enemy(passer, dt);
        self.entities.update_children();

        {
            let passer = &mut *passer;
            self.entities.update_colliders(world, is_trusted.then(move || passer));
        }

        self.entities.update_damaging(passer, damage_info);
    }

    pub fn main_player(&self) -> Entity
    {
        self.player_entity.unwrap()
    }

    pub fn player_transform(&self) -> Option<Ref<Transform>>
    {
        self.entities.transform(self.main_player())
    }

    pub fn player_exists(&self) -> bool
    {
        self.player_entity.map(|player| self.entities.exists(player)).unwrap_or(false)
    }

    fn update_buffers(
        &mut self,
        visibility: &VisibilityChecker,
        info: &mut UpdateBuffersInfo,
        casters: &OccludingCasters
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

        let mut queue: Vec<_> = self.entities.render.iter().map(|(_, x)| x).collect();

        queue.sort_unstable_by_key(|render| render.get().z_level);

        queue.into_iter().for_each(|render|
        {
            render.get().draw(visibility, info);
        });

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InventoryWhich
{
    Player,
    Other
}

pub enum UserEvent
{
    Close(InventoryWhich),
    Wield(InventoryItem),
    Take(InventoryItem)
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

pub struct GameState
{
    pub mouse_position: Vector2<f32>,
    pub camera: Arc<RwLock<Camera>>,
    pub assets: Arc<Mutex<Assets>>,
    pub object_factory: Rc<ObjectFactory>,
    pub notifications: Notifications,
    pub entities: ClientEntitiesContainer,
    pub controls: ControlsController,
    pub running: bool,
    pub debug_mode: bool,
    pub tilemap: Arc<TileMap>,
    pub items_info: Arc<ItemsInfo>,
    pub characters_info: Arc<CharactersInfo>,
    pub user_receiver: Rc<RefCell<Vec<UserEvent>>>,
    pub ui: Rc<RefCell<Ui>>,
    pub common_textures: CommonTextures,
    shaders: ProgramShaders,
    host: bool,
    is_trusted: bool,
    camera_scale: f32,
    dt: f32,
    rare_timer: f32,
    world: World,
    connections_handler: Arc<RwLock<ConnectionsHandler>>,
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
                return;
            }
        }
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
        let mut entities = ClientEntitiesContainer::new();

        let tilemap = info.tiles_factory.tilemap().clone();

        let world_receiver = WorldReceiver::new(connections_handler.clone());
        let world = World::new(
            world_receiver,
            info.tiles_factory,
            info.camera.read().size(),
            Pos3::new(0.0, 0.0, 0.0)
        );

        entities.player_entity = Some(Self::connect_to_server(
            connections_handler.clone(),
            &info.client_info.name
        ));

        sender_loop(connections_handler.clone());

        let handler = connections_handler.read().passer_clone();

        let (sender, receiver) = mpsc::channel();

        receiver_loop(handler, move |message|
        {
            if sender.send(message).is_err()
            {
                ControlFlow::Break(())
            } else
            {
                ControlFlow::Continue(())
            }
        }, || ());

        let user_receiver = Rc::new(RefCell::new(Vec::new()));

        let ui = {
            // mmm i love the borrow checker
            let urx00 = user_receiver.clone();
            let urx01 = user_receiver.clone();

            let urx10 = user_receiver.clone();
            let urx11 = user_receiver.clone();

            let player_actions = InventoryActions{
                on_close: move ||
                {
                    urx00.borrow_mut().push(UserEvent::Close(InventoryWhich::Player));
                },
                on_change: move |item|
                {
                    urx01.borrow_mut().push(UserEvent::Wield(item));
                }
            };

            let other_actions = InventoryActions{
                on_close: move ||
                {
                    urx10.borrow_mut().push(UserEvent::Close(InventoryWhich::Other));
                },
                on_change: move |item|
                {
                    urx11.borrow_mut().push(UserEvent::Take(item));
                }
            };

            Ui::new(
                &mut entities.entity_creator(),
                info.data_infos.items_info.clone(),
                player_actions,
                other_actions
            )
        };

        let ui = Rc::new(RefCell::new(ui));

        let assets = info.object_info.partial.assets;
        let common_textures = CommonTextures::new(&mut assets.lock());

        entities.entities.update_ui_aspect(info.camera.read().aspect());

        let this = Self{
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
            shaders: info.shaders,
            debug_mode: info.client_info.debug_mode,
            tilemap,
            camera_scale: 1.0,
            dt: 0.0,
            rare_timer: 0.0,
            world,
            ui,
            common_textures,
            host: info.host,
            is_trusted: false,
            user_receiver,
            connections_handler,
            receiver
        };

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
        let message = Message::PlayerConnect{name: name.to_owned()};

        let mut handler = handler.write();

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
        self.entities.player_entity.unwrap()
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
                Err(_) =>
                {
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
        self.camera_scale = scale;
        let mut camera = self.camera.write();

        camera.rescale(scale);
        self.world.rescale(camera.size());
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

        let casters: Vec<_> = self.entities.player_transform().map(|x| x.position)
            .into_iter()
            .collect();

        let casters = OccludingCasters::from(casters);

        let visibility = self.visibility_checker();

        self.world.update_buffers(info, &visibility, &casters);

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

        self.entities.update_buffers(&visibility, info, &casters);

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

    pub fn update_pre(&mut self, dt: f32)
    {
        self.check_resize_camera(dt);

        self.world.update(dt);

        let player_transform = self.entities.player_transform().as_deref().cloned();

        self.ui.borrow_mut().update(
            &mut self.entities.entity_creator(),
            &self.camera.read(),
            player_transform,
            dt
        );

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
            
            self.rare_timer = 5.0;
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
        self.world.rescale(size);

        self.entities.entities.update_ui_aspect(aspect);
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
