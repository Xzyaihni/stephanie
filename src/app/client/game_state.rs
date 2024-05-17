use std::{
    ops::ControlFlow,
    sync::{
        Arc,
        mpsc::{self, TryRecvError, Receiver}
    }
};

use parking_lot::{RwLock, Mutex};

use nalgebra::{Unit, Vector3, Vector2};

use yanyaengine::{
    Assets,
    ObjectFactory,
    Transform,
    camera::Camera,
    game_object::*
};

use crate::common::{
    sender_loop,
    receiver_loop,
    TileMap,
    Damage,
    ItemsInfo,
    Entity,
    Entities,
    EnemiesInfo,
    Damageable,
    EntityPasser,
    EntitiesController,
    entity::ClientEntities,
    message::Message,
    world::{
        World,
        Pos3,
        Tile,
        TilePos
    }
};

use super::{
    ClientInfo,
    MessagePasser,
    ConnectionsHandler,
    TilesFactory,
    world_receiver::WorldReceiver
};

pub use controls_controller::Control;

use controls_controller::{ControlsController, ControlState};

use notifications::{Notifications, Notification};

use ui::Ui;

mod controls_controller;

mod notifications;

mod ui;


struct RaycastResult
{
    distance: f32,
    pierce: f32
}

pub struct ClientEntitiesContainer
{
    local_entities: ClientEntities,
    entities: ClientEntities,
    main_player: Option<Entity>,
    player_children: Vec<Entity>
}

impl ClientEntitiesContainer
{
    pub fn new() -> Self
    {
        Self{
            local_entities: Entities::new(),
            entities: Entities::new(),
            main_player: None,
            player_children: Vec::new()
        }
    }
    
    pub fn handle_message(
        &mut self,
        create_info: &mut ObjectCreateInfo,
        message: Message
    ) -> Option<Message>
    {
        self.entities.handle_message(create_info, message)
    }

    fn update_objects(&mut self, enemies_info: &EnemiesInfo, info: &mut UpdateBuffersInfo)
    {
        self.entities.update_sprites(&mut info.object_info, enemies_info);
        self.local_entities.update_sprites(&mut info.object_info, enemies_info);

        self.update_buffers(info);
    }

    pub fn update(&mut self, dt: f32)
    {
        Self::update_entities(&mut self.entities, dt);
        Self::update_entities(&mut self.local_entities, dt);
    }

    fn update_entities(entities: &mut ClientEntities, dt: f32)
    {
        entities.update_physical(dt);
        entities.update_lazy(dt);
        entities.update_enemy(dt);
    }

    pub fn player_transform(&self) -> Option<&Transform>
    {
        self.player_exists().then(||
        {
            self.entities.transform(self.main_player.unwrap()).unwrap()
        })
    }

    pub fn player_exists(&self) -> bool
    {
        if let Some(player) = self.main_player
        {
            self.entities.exists(player)
        } else
        {
            false
        }
    }

    fn raycast_entity(
        start: &Vector3<f32>,
        direction: &Unit<Vector3<f32>>,
        transform: &Transform
    ) -> Option<RaycastResult>
    {
        let scale = transform.scale;

        let radius = scale.x.max(scale.y.max(scale.z)) / 2.0;

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

        let mut hits: Vec<_> = self.entities.entities_iter()
            .filter_map(|entity|
            {
                let transform = self.entities.transform(entity);

                transform.and_then(|transform|
                {
                    if info.ignore_player
                    {
                        let is_player = self.main_player == Some(entity)
                            || self.player_children.contains(&entity);

                        (!is_player).then_some((entity, transform))
                    } else
                    {
                        Some((entity, transform))
                    }
                })
            })
            .filter_map(|(entity, transform)|
            {
                Self::raycast_entity(start, &direction, transform).and_then(|hit|
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

        hits.sort_unstable_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap());

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
}

impl GameObject for ClientEntitiesContainer
{
    fn update_buffers(&mut self, info: &mut UpdateBuffersInfo)
    {
        self.entities.update_render();
        self.local_entities.update_render();

        let renders = self.entities.render.iter_mut()
            .chain(self.local_entities.render.iter_mut());

        renders.for_each(|(_, entity)|
        {
            if let Some(object) = entity.get_mut().object.as_mut()
            {
                object.update_buffers(info);
            }
        });
    }

    fn draw(&self, info: &mut DrawInfo)
    {
        let renders = self.entities.render.iter()
            .chain(self.local_entities.render.iter());

        let mut queue: Vec<_> = renders.map(|(_, x)| x).collect();

        queue.sort_unstable_by_key(|render| render.get().z_level);

        queue.into_iter().for_each(|render|
        {
            if let Some(object) = render.get().object.as_ref()
            {
                object.draw(info);
            }
        });
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MousePosition
{
    pub x: f32,
    pub y: f32
}

impl MousePosition
{
    pub fn new(x: f32, y: f32) -> Self
    {
        Self{x, y}
    }

    pub fn center_offset(self) -> Vector2<f32>
    {
        Vector2::new(self.x - 0.5, self.y - 0.5)
    }
}

impl From<(f64, f64)> for MousePosition
{
    fn from(value: (f64, f64)) -> Self
    {
        Self{x: value.0 as f32, y: value.1 as f32}
    }
}

pub struct RaycastInfo
{
    pub pierce: Option<f32>,
    pub ignore_player: bool,
    pub ignore_end: bool
}

#[derive(Debug)]
pub enum RaycastHitId
{
    Entity(Entity),
    // later
    Tile
}

#[derive(Debug)]
pub struct RaycastHit
{
    pub id: RaycastHitId,
    pub distance: f32,
    pub width: f32
}

#[derive(Debug)]
pub struct RaycastHits
{
    start: Vector3<f32>,
    direction: Unit<Vector3<f32>>,
    pub hits: Vec<RaycastHit>
}

impl RaycastHits
{
    pub fn hit_position(&self, hit: &RaycastHit) -> Vector3<f32>
    {
        self.start + self.direction.into_inner() * hit.distance
    }
}

pub struct GameStateInfo<'a>
{
    pub camera: Arc<RwLock<Camera>>,
    pub object_info: ObjectCreateInfo<'a>,
    pub items_info: Arc<ItemsInfo>,
    pub enemies_info: Arc<EnemiesInfo>,
    pub tiles_factory: TilesFactory,
    pub message_passer: MessagePasser,
    pub client_info: &'a ClientInfo
}

pub struct GameState
{
    pub mouse_position: MousePosition,
    pub camera: Arc<RwLock<Camera>>,
    pub assets: Arc<Mutex<Assets>>,
    pub object_factory: Arc<ObjectFactory>,
    pub notifications: Notifications,
    pub entities: ClientEntitiesContainer,
    pub controls: ControlsController,
    pub running: bool,
    pub debug_mode: bool,
    pub tilemap: Arc<TileMap>,
    items_info: Arc<ItemsInfo>,
    enemies_info: Arc<EnemiesInfo>,
    world: World,
    ui: Ui,
    connections_handler: Arc<RwLock<ConnectionsHandler>>,
    receiver: Receiver<Message>
}

impl GameState
{
    pub fn new(mut info: GameStateInfo) -> Self
    {
        let mouse_position = MousePosition::new(0.0, 0.0);

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
            info.camera.read().aspect(),
            Pos3::new(0.0, 0.0, 0.0)
        );

        let (player_id, player_children) = Self::connect_to_server(
            connections_handler.clone(),
            &info.client_info.name
        );

        entities.main_player = Some(player_id);
        entities.player_children = player_children;

        sender_loop(connections_handler.clone());

        let handler = connections_handler.read().passer_clone();

        let (sender, receiver) = mpsc::channel();

        receiver_loop(handler, move |message|
        {
            if let Err(_) = sender.send(message)
            {
                ControlFlow::Break(())
            } else
            {
                ControlFlow::Continue(())
            }
        }, || ());

        let (x, y) = info.camera.read().aspect();
        let ui = Ui::new(
            &mut info.object_info,
            &mut entities.local_entities,
            x / y
        );

        Self{
            mouse_position,
            camera: info.camera,
            assets: info.object_info.partial.assets,
            object_factory: info.object_info.partial.object_factory,
            notifications,
            entities,
            items_info: info.items_info,
            enemies_info: info.enemies_info,
            controls,
            running: true,
            debug_mode: info.client_info.debug_mode,
            tilemap,
            world,
            ui,
            connections_handler,
            receiver
        }
    }

    pub fn raycast(
        &self,
        info: RaycastInfo,
        start: &Vector3<f32>,
        end: &Vector3<f32>
    ) -> RaycastHits
    {
        self.entities.raycast(info, start, end)
    }

    pub fn sync_transform(&mut self, entity: Entity)
    {
        let transform = self.entities().transform(entity).unwrap().clone();

        self.send_message(Message::SetTransform{entity, transform});
    }

    fn connect_to_server(
        handler: Arc<RwLock<ConnectionsHandler>>,
        name: &str
    ) -> (Entity, Vec<Entity>)
    {
        let message = Message::PlayerConnect{name: name.to_owned()};

        let mut handler = handler.write();

        if let Err(x) = handler.send_blocking(&message)
        {
            panic!("error connecting to server: {x}");
        }

        match handler.receive_blocking()
        {
            Ok(Some(Message::PlayerOnConnect{entity, children})) =>
            {
                (entity, children)
            },
            x => panic!("received wrong message on connect: {x:?}")
        }
    }

    pub fn damage_entity(&mut self, entity: Entity, damage: Damage)
    {
        if self.entities().player(entity).is_some()
        {
            return;
        }

        self.send_message(Message::EntityDamage{entity, damage: damage.clone()});

        if let Some(anatomy) = self.entities_mut().anatomy_mut(entity)
        {
            anatomy.damage(damage);

            self.entities_mut().anatomy_changed(entity);
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

    pub fn player(&self) -> Entity
    {
        self.entities.main_player.unwrap()
    }

    pub fn process_messages(&mut self, create_info: &mut ObjectCreateInfo)
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

    fn process_message_inner(&mut self, create_info: &mut ObjectCreateInfo, message: Message)
    {
        let message = match self.entities.handle_message(create_info, message)
        {
            Some(x) => x,
            None => return
        };

        let message = match self.world.handle_message(message)
        {
            Some(x) => x,
            None => return
        };

        match message
        {
            Message::PlayerFullyConnected =>
            {
                self.notifications.set(Notification::PlayerConnected);
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
        let camera_scale = self.camera.read().aspect();
        let (highest, mut lowest) = (
            camera_scale.0.max(camera_scale.1) * factor,
            camera_scale.1.min(camera_scale.0) * factor
        );

        if !self.debug_mode
        {
            let (min_scale, max_scale) = World::zoom_limits();

            let adjust_factor = if highest > max_scale
            {
                max_scale / highest
            } else
            {
                1.0
            };

            lowest *= adjust_factor;
            lowest = lowest.max(min_scale);
        }

        self.set_camera_scale(lowest);
    }

    fn set_camera_scale(&mut self, scale: f32)
    {
        let mut camera = self.camera.write();

        camera.rescale(scale);
        self.world.rescale(camera.aspect());
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

    pub fn player_tile(&self) -> TilePos
    {
        self.world.player_tile()
    }

    pub fn player_connected(&mut self) -> bool
    {
        self.notifications.get(Notification::PlayerConnected)
    }

    pub fn update_buffers(&mut self, partial_info: UpdateBuffersPartialInfo)
    {
        let mut info = UpdateBuffersInfo::new(partial_info, &self.camera.read());
        let info = &mut info;

        self.camera.write().update();

        self.process_messages(&mut info.object_info);

        self.world.update_buffers(info);

        self.entities.update_objects(&self.enemies_info, info);
    }

    pub fn draw(&self, info: &mut DrawInfo)
    {
        self.world.draw(info);

        self.entities.draw(info);
    }

    pub fn update(&mut self, dt: f32)
    {
        self.check_resize_camera(dt);
        self.camera_moved();

        self.world.update(dt);

        let (x, y) = self.camera.read().aspect();
        let camera_size = Vector2::new(x, y);

        let player_transform = self.entities.player_transform().cloned();

        self.ui.update(&mut self.entities.local_entities, player_transform, camera_size);
        self.entities.update(dt);

        self.controls.release_clicked();
    }

    pub fn input(&mut self, control: yanyaengine::Control)
    {
        self.controls.handle_input(control);
    }

    pub fn pressed(&self, control: Control) -> bool
    {
        match self.controls.state(control)
        {
            ControlState::Pressed => true,
            _ => false
        }
    }

    #[allow(dead_code)]
    pub fn clicked(&mut self, control: Control) -> bool
    {
        self.controls.is_clicked(control)
    }

    pub fn world_mouse_position(&self) -> Vector2<f32>
    {
        let camera_size = self.camera.read().aspect();
        let scale = Vector2::new(camera_size.0, camera_size.1);

        self.mouse_position.center_offset().component_mul(&scale)
    }

    pub fn camera_moved(&mut self)
    {
        let pos = *self.camera.read().position();

        self.world.camera_moved(pos.into());
    }

    pub fn resize(&mut self, aspect: f32)
    {
        let mut camera = self.camera.write();
        camera.resize(aspect);

        let aspect = camera.aspect();
        self.world.rescale(aspect);
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
