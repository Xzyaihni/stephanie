use std::{
    mem,
    rc::Rc,
    ops::ControlFlow,
    cmp::Ordering,
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
    ServerToClient,
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
    UiEvent,
    ClientInfo,
    MessagePasser,
    ConnectionsHandler,
    TilesFactory,
    VisibilityChecker,
    world_receiver::WorldReceiver
};

pub use controls_controller::{Control, ControlState};
pub use entity_creator::{EntityCreator, ReplaceObject};

use controls_controller::ControlsController;

use notifications::{Notifications, Notification};

use ui::Ui;

mod controls_controller;

mod notifications;

mod entity_creator;
mod ui;


struct RaycastResult
{
    distance: f32,
    pierce: f32
}

pub struct ClientEntitiesContainer
{
    local_objects: Vec<(Entity, ReplaceObject)>,
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
            local_objects: Vec::new(),
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

    fn update_objects(
        &mut self,
        visibility: &VisibilityChecker,
        enemies_info: &EnemiesInfo,
        info: &mut UpdateBuffersInfo
    )
    {
        self.entities.update_sprites(&mut info.object_info, enemies_info);
        self.local_entities.update_sprites(&mut info.object_info, enemies_info);

        mem::take(&mut self.local_objects).into_iter().for_each(|(entity, object)|
        {
            let transform = self.local_entities.transform(entity).cloned();

            match object
            {
                ReplaceObject::Full(object) =>
                {
                    let object = object.server_to_client(
                        || transform.unwrap(),
                        &mut info.object_info
                    );

                    self.local_entities.set_render(entity, Some(object));
                },
                ReplaceObject::Object(object) =>
                {
                    if let Some(render) = self.local_entities.render_mut(entity)
                    {
                        render.object = object.into_client(
                            transform.unwrap(),
                            &mut info.object_info
                        );
                    }
                },
                ReplaceObject::Scissor(scissor) =>
                {
                    if let Some(render) = self.local_entities.render_mut(entity)
                    {
                        render.scissor = Some(scissor.into_global(info.object_info.partial.size));
                    }
                }
            }
        });

        self.update_buffers(visibility, info);
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

    pub fn update_buffers(
        &mut self,
        visibility: &VisibilityChecker,
        info: &mut UpdateBuffersInfo
    )
    {
        self.entities.update_render();
        self.local_entities.update_render();

        let renders = self.entities.render.iter_mut()
            .chain(self.local_entities.render.iter_mut());

        renders.for_each(|(_, entity)|
        {
            entity.get_mut().update_buffers(visibility, info);
        });
    }

    pub fn draw(
        &self,
        visibility: &VisibilityChecker,
        info: &mut DrawInfo
    )
    {
        let renders = self.entities.render.iter()
            .chain(self.local_entities.render.iter());

        let mut queue: Vec<_> = renders.map(|(_, x)| x).collect();

        queue.sort_unstable_by_key(|render| render.get().z_level);

        queue.into_iter().for_each(|render|
        {
            render.get().draw(visibility, info);
        });
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
    camera_scale: f32,
    items_info: Arc<ItemsInfo>,
    enemies_info: Arc<EnemiesInfo>,
    world: World,
    ui: Ui,
    connections_handler: Arc<RwLock<ConnectionsHandler>>,
    receiver: Receiver<Message>
}

impl GameState
{
    pub fn new(info: GameStateInfo) -> Self
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

        let aspect = info.camera.read().aspect();

        let mut entity_creator = EntityCreator{
            objects: &mut entities.local_objects,
            entities: &mut entities.local_entities
        };

        let ui = Ui::new(
            &mut entity_creator,
            info.items_info.clone(),
            aspect
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
            camera_scale: 1.0,
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
                self.update_inventory();
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

        let visibility = self.visibility_checker();

        self.world.update_buffers(info);

        self.entities.update_objects(&visibility, &self.enemies_info, info);

        self.controls.release_clicked();
    }

    pub fn draw(&self, info: &mut DrawInfo)
    {
        self.world.draw(info);

        let visibility = self.visibility_checker();

        self.entities.draw(&visibility, info);
    }

    fn visibility_checker(&self) -> VisibilityChecker
    {
        let camera = self.camera.read();

        VisibilityChecker{
            size: camera.size(),
            position: camera.position().coords
        }
    }

    pub fn update(&mut self, dt: f32)
    {
        self.check_resize_camera(dt);
        self.camera_moved();

        self.world.update(dt);

        if self.controls.is_clicked(Control::SecondaryAction)
        {
            let player = self.player();
            let inventory = self.entities.entities.inventory_mut(player).unwrap();

            inventory.push(&self.items_info, self.items_info.random());

            self.update_inventory();
        }

        self.entities.local_entities.update_ui(
            self.camera.read().position().coords.xy(),
            UiEvent::MouseMove(self.world_mouse_position())
        );

        let player_transform = self.entities.player_transform().cloned();

        let mut entity_creator = EntityCreator{
            objects: &mut self.entities.local_objects,
            entities: &mut self.entities.local_entities
        };

        self.ui.update(
            &mut entity_creator,
            &self.camera.read(),
            player_transform,
            dt
        );

        self.entities.update(dt);
    }

    pub fn update_inventory(&mut self)
    {
        let player_id = self.player();

        let entities = &mut self.entities.entities;
        let local_objects = &mut self.entities.local_objects;
        let local_entities = &mut self.entities.local_entities;

        let player = entities.player(player_id).unwrap();
        let inventory = entities.inventory(player_id).unwrap();

        let mut entity_creator = EntityCreator{
            objects: local_objects,
            entities: local_entities
        };

        self.ui.player_inventory.full_update(
            &mut entity_creator,
            player.name.clone(),
            inventory
        );
    }

    pub fn input(&mut self, control: yanyaengine::Control)
    {
        let matched = self.controls.handle_input(control);

        if let Some((state, control)) = matched
        {
            let event = UiEvent::from_control(|| self.world_mouse_position(), state, control);
            if let Some(event) = event
            {
                self.entities.local_entities.update_ui(
                    self.camera.read().position().coords.xy(),
                    event
                );
            }
        }
    }

    pub fn pressed(&self, control: Control) -> bool
    {
        self.controls.is_down(control)
    }

    #[allow(dead_code)]
    pub fn clicked(&mut self, control: Control) -> bool
    {
        self.controls.is_clicked(control)
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

    pub fn camera_moved(&mut self)
    {
        let pos = *self.camera.read().position();

        self.world.camera_moved(pos.into());
    }

    pub fn resize(&mut self, aspect: f32)
    {
        let mut camera = self.camera.write();
        camera.resize(aspect);

        let size = camera.size();
        self.world.rescale(size);
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
