use std::{
    mem,
    cell::{Ref, RefCell},
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

use serde::{Serialize, Deserialize};

use yanyaengine::{
    Assets,
    ObjectFactory,
    Transform,
    TextureId,
    UniformLocation,
    camera::Camera,
    game_object::*
};

use crate::{
    ProgramShaders,
    client::RenderCreateInfo,
    common::{
        sender_loop,
        receiver_loop,
        collider::*,
        TileMap,
        DamagePartial,
        ItemsInfo,
        InventoryItem,
        AnyEntities,
        Entity,
        Entities,
        EnemiesInfo,
        ServerToClient,
        EntityPasser,
        EntitiesController,
        PlayerEntities,
        OccludingCasters,
        entity::{ComponentWrapper, ClientEntities},
        message::Message,
        world::{
            World,
            Pos3,
            Tile,
            TilePos
        }
    }
};

use super::{
    Game,
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

pub use ui::Ui;
use ui::InventoryActions;

mod controls_controller;

mod notifications;

mod entity_creator;
mod ui;


struct RaycastResult
{
    distance: f32,
    pierce: f32
}

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
    local_objects: Vec<(Entity, ReplaceObject)>,
    player_entities: Option<PlayerEntities>
}

impl ClientEntitiesContainer
{
    pub fn new() -> Self
    {
        Self{
            local_objects: Vec::new(),
            entities: Entities::new(),
            player_entities: None
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
            objects: &mut self.local_objects,
            entities: &mut self.entities
        }
    }

    pub fn update_objects(
        &mut self,
        enemies_info: &EnemiesInfo,
        create_info: &mut RenderCreateInfo,
        dt: f32
    )
    {
        self.entities.create_queued(create_info);
        self.entities.update_sprites(create_info, enemies_info);
        self.entities.update_watchers(dt);

        mem::take(&mut self.local_objects).into_iter().for_each(|(entity, object)|
        {
            match object
            {
                ReplaceObject::Full(object) =>
                {
                    let object = object.server_to_client(
                        || self.entities.target_ref(entity).unwrap().clone(),
                        create_info
                    );

                    self.entities.set_render(entity, Some(object));
                },
                ReplaceObject::Object(object) =>
                {
                    if let Some(mut render) = self.entities.render_mut(entity)
                    {
                        render.object = object.into_client(
                            self.entities.target_ref(entity).unwrap().clone(),
                            create_info
                        );
                    }
                },
                ReplaceObject::Scissor(scissor) =>
                {
                    if let Some(mut render) = self.entities.render_mut(entity)
                    {
                        render.scissor = Some(scissor.into_global(create_info.object_info.partial.size));
                    }
                }
            }
        });
    }

    pub fn update(
        &mut self,
        passer: &mut impl EntityPasser,
        damage_info: TextureId,
        dt: f32
    )
    {
        self.entities.update_physical(dt);
        self.entities.update_lazy(dt);
        self.entities.update_follows(dt);
        self.entities.update_enemy(dt);
        self.entities.update_children();
        self.entities.update_colliders(passer);
        self.entities.update_damaging(passer, damage_info);
    }

    pub fn main_player(&self) -> Entity
    {
        self.player_entities.as_ref().unwrap().player
    }

    pub fn player_transform(&self) -> Option<Ref<Transform>>
    {
        self.entities.transform(self.main_player())
    }

    pub fn player_exists(&self) -> bool
    {
        if let Some(player) = self.player_entities.as_ref()
        {
            self.entities.exists(player.player)
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

        let mut hits: Vec<_> = self.entities.collider.iter()
            .filter_map(|(_, ComponentWrapper{
                entity,
                component: collider
            })|
            {
                let collides = collider.borrow().layer.collides(&info.layer);

                (collides && !collider.borrow().ghost).then(||
                {
                    *entity
                })
            })
            .filter_map(|entity|
            {
                let transform = self.entities.transform(entity);

                transform.and_then(|transform|
                {
                    if info.ignore_player
                    {
                        let is_player = self.player_entities.as_ref()
                            .map(|x| x.is_player(entity))
                            .unwrap_or(false);

                        (!is_player).then_some((entity, transform))
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

pub struct RaycastInfo
{
    pub pierce: Option<f32>,
    pub layer: ColliderLayer,
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
    pub shaders: ProgramShaders,
    pub camera: Arc<RwLock<Camera>>,
    pub object_info: ObjectCreateInfo<'a>,
    pub items_info: Arc<ItemsInfo>,
    pub enemies_info: Arc<EnemiesInfo>,
    pub tiles_factory: TilesFactory,
    pub message_passer: MessagePasser,
    pub client_info: &'a ClientInfo,
    pub host: bool
}

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
    pub blood_texture: TextureId
}

impl CommonTextures
{
    pub fn new(assets: &mut Assets) -> Self
    {
        Self{
            dust: assets.texture_id("decals/dust.png"),
            blood_texture: assets.texture_id("decals/blood.png")
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
    pub user_receiver: Rc<RefCell<Vec<UserEvent>>>,
    pub ui: Ui,
    pub common_textures: CommonTextures,
    shaders: ProgramShaders,
    host: bool,
    camera_scale: f32,
    dt: f32,
    enemies_info: Arc<EnemiesInfo>,
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

        entities.player_entities = Some(Self::connect_to_server(
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
                info.enemies_info.clone(),
                info.items_info.clone(),
                player_actions,
                other_actions
            )
        };

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
            items_info: info.items_info,
            enemies_info: info.enemies_info,
            controls,
            running: true,
            shaders: info.shaders,
            debug_mode: info.client_info.debug_mode,
            tilemap,
            camera_scale: 1.0,
            dt: 0.0,
            world,
            ui,
            common_textures,
            host: info.host,
            user_receiver,
            connections_handler,
            receiver
        };

        Rc::new(RefCell::new(this))
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

        self.send_message(Message::SetTransform{entity, component: transform});
    }

    fn connect_to_server(
        handler: Arc<RwLock<ConnectionsHandler>>,
        name: &str
    ) -> PlayerEntities
    {
        let message = Message::PlayerConnect{name: name.to_owned()};

        let mut handler = handler.write();

        if let Err(x) = handler.send_blocking(&message)
        {
            panic!("error connecting to server: {x}");
        }

        match handler.receive_blocking()
        {
            Ok(Some(Message::PlayerOnConnect{player_entities})) =>
            {
                player_entities
            },
            x => panic!("received wrong message on connect: {x:?}")
        }
    }

    pub fn damage_entity(&mut self, angle: f32, entity: Entity, damage: DamagePartial)
    {
        let mut passer = self.connections_handler.write();
        self.entities.entities.damage_entity(
            &mut *passer,
            self.damage_info(),
            angle,
            entity,
            damage
        );
    }

    fn damage_info(&self) -> TextureId
    {
        self.common_textures.blood_texture
    }

    pub fn entities(&self) -> &ClientEntities
    {
        &self.entities.entities
    }

    pub fn entities_mut(&mut self) -> &mut ClientEntities
    {
        &mut self.entities.entities
    }

    pub fn object_change(&mut self, entity: Entity, object: ReplaceObject)
    {
        self.entities.local_objects.push((entity, object));
    }

    pub fn player_entities(&self) -> &PlayerEntities
    {
        self.entities.player_entities.as_ref().unwrap()
    }

    pub fn player(&self) -> Entity
    {
        self.entities.main_player()
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

    pub fn player_connected(&mut self) -> bool
    {
        self.notifications.get(Notification::PlayerConnected)
    }

    pub fn update_buffers(&mut self, partial_info: UpdateBuffersPartialInfo)
    {
        let mut info = UpdateBuffersInfo::new(partial_info, &self.camera.read());
        let info = &mut info;

        let casters: Vec<_> = self.entities.player_transform().map(|x| x.position)
            .into_iter()
            .collect();

        let casters = OccludingCasters::from(casters);

        self.camera.write().update();

        let visibility = self.visibility_checker();

        self.world.update_buffers(info, &visibility, &casters);

        let mut create_info = RenderCreateInfo{
            location: UniformLocation{set: 0, binding: 0},
            shader: self.shaders.default,
            object_info: &mut info.object_info
        };

        self.process_messages(&mut create_info);

        self.entities.update_objects(
            &self.enemies_info,
            &mut create_info,
            self.dt
        );

        self.entities.update_buffers(&visibility, info, &casters);
    }

    pub fn draw(&self, info: &mut DrawInfo)
    {
        info.bind_pipeline(self.shaders.world);

        let visibility = self.visibility_checker();

        self.world.draw(info, &visibility, self.shaders.shadow);

        info.bind_pipeline(self.shaders.default);
        info.set_depth_write(false);

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

    fn on_control(&mut self, game: &mut Game, state: ControlState, control: Control)
    {
        game.on_control(self, state, control);
    }

    pub fn update(&mut self, game: &mut Game, dt: f32)
    {
        self.check_resize_camera(dt);

        self.world.update(dt);

        let player_transform = self.entities.player_transform().as_deref().cloned();

        self.ui.update(
            &mut self.entities.entity_creator(),
            &self.camera.read(),
            player_transform,
            dt
        );

        {
            let mut passer = self.connections_handler.write();
            self.entities.update(&mut *passer, self.damage_info(), dt);
        }

        game.update(self, dt);

        let changed_this_frame = self.controls.changed_this_frame();
        for (state, control) in changed_this_frame
        {
            let event = UiEvent::from_control(|| self.world_mouse_position(), state, control);
            if let Some(event) = event
            {
                let captured = self.entities.entities.update_ui(
                    self.camera.read().position().coords.xy(),
                    event
                );

                if captured
                {
                    continue;
                }
            }

            self.on_control(game, state, control);
        }

        self.entities.entities.update_ui(
            self.camera.read().position().coords.xy(),
            UiEvent::MouseMove(self.world_mouse_position())
        );

        self.ui.update_after(&mut self.entities.entity_creator(), &self.camera.read());

        self.dt = dt;
    }

    pub fn update_inventory(&mut self)
    {
        let player_id = self.player();

        let mut entity_creator = EntityCreator{
            objects: &mut self.entities.local_objects,
            entities: &mut self.entities.entities
        };

        self.ui.player_inventory.full_update(
            &mut entity_creator,
            player_id
        );
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
