use std::{
    process,
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
    TransformContainer,
    camera::Camera,
    game_object::*
};

use crate::common::{
	sender_loop,
	receiver_loop,
    ObjectsStore,
    TileMap,
    Damage,
    Entity,
    Entities,
    EntityPasser,
	EntitiesController,
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

mod controls_controller;

mod notifications;


struct RaycastResult
{
    distance: f32,
    pierce: f32
}

pub struct ClientEntitiesContainer
{
	entities: Entities,
	main_player: Option<Entity>
}

impl ClientEntitiesContainer
{
	pub fn new() -> Self
	{
		let entities = Entities::new();
		let main_player = None;

		Self{entities, main_player}
	}
    
    pub fn handle_message(&mut self, message: Message) -> Option<Message>
    {
        self.entities.handle_message(message)
    }

	pub fn update(&mut self, dt: f32)
	{
        return;
        todo!();
		// self.entities.iter_mut().for_each(|(_, entity)| entity.update(dt));
	}

	pub fn player_exists(&self, entity: Entity) -> bool
	{
        return false;
        todo!();
		// self.players.contains(id)
	}

    fn raycast_entity(
        start: &Vector3<f32>,
        direction: &Unit<Vector3<f32>>,
        entity: &Entity
    ) -> Option<RaycastResult>
    {
        /*let scale = entity.scale();

        // im not dealing with this
        debug_assert!(scale.x == scale.y && scale.x == scale.z);
        let radius = scale.x / 2.0;

        let position = todo!();//entity.position();

        let offset = start - position;

        let left = direction.dot(&offset).powi(2);
        let right = todo!();//offset.magnitude_squared() - radius.powi(2);

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
        }*/
        todo!();
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

        /*let mut hits: Vec<_> = self.enemies.iter()
            .map(|(id, enemy)| (EntityType::Enemy(id), enemy.entity_ref()))
            .chain(self.players.iter()
                .filter(|(id, _)|
                {
                    if info.ignore_player
                    {
                        self.main_player != Some(*id)
                    } else
                    {
                        true
                    }
                })
                .map(|(id, player)| (EntityType::Player(id), player.entity_ref())))
            .filter_map(|(id, entity)|
            {
                Self::raycast_entity(start, &direction, entity).and_then(|hit|
                {
                    let backwards = (hit.distance + hit.pierce) < 0.0;
                    let past_end = (hit.distance > max_distance) && !info.ignore_end;

                    if backwards || past_end
                    {
                        None
                    } else
                    {
                        let id = RaycastHitId::Entity(id);
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

        RaycastHits{start: *start, direction, hits}*/
        todo!();
    }
}

impl GameObject for ClientEntitiesContainer
{
	fn update_buffers(&mut self, info: &mut UpdateBuffersInfo)
    {
        todo!();
		// self.players.iter_mut().for_each(|(_, pair)| pair.update_buffers(info));
		// self.enemies.iter_mut().for_each(|(_, pair)| pair.update_buffers(info));
    }

	fn draw(&self, info: &mut DrawInfo)
    {
        todo!();
        /*self.enemies.iter().for_each(|(_, pair)| pair.draw(info));

		if let Some(player_id) = self.main_player
		{
			self.players.iter().filter(|(id, _)| *id != player_id)
				.for_each(|(_, pair)| pair.draw(info));

            // player could be uninitialized
			if let Some(player) = self.players.get(player_id)
            {
                player.draw(info);
            }
		} else
		{
			self.players.iter().for_each(|(_, pair)| pair.draw(info));
		}*/
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
	world: World,
	connections_handler: Arc<RwLock<ConnectionsHandler>>,
	receiver: Receiver<Message>
}

impl GameState
{
	pub fn new(
		camera: Arc<RwLock<Camera>>,
        assets: Arc<Mutex<Assets>>,
		object_factory: Arc<ObjectFactory>,
		tiles_factory: TilesFactory,
		message_passer: MessagePasser,
		client_info: &ClientInfo
	) -> Self
	{
		let mouse_position = MousePosition::new(0.0, 0.0);

		let notifications = Notifications::new();
		let mut entities = ClientEntitiesContainer::new();
        let controls = ControlsController::new();
		let connections_handler = Arc::new(RwLock::new(ConnectionsHandler::new(message_passer)));

        let tilemap = tiles_factory.tilemap().clone();

		let world_receiver = WorldReceiver::new(connections_handler.clone());
		let world = World::new(
			world_receiver,
			tiles_factory,
			camera.read().aspect(),
			Pos3::new(0.0, 0.0, 0.0)
		);

		let player_id = Self::connect_to_server(connections_handler.clone(), &client_info.name);
        entities.main_player = Some(player_id);

		sender_loop(connections_handler.clone());

		let handler = connections_handler.read().passer_clone();

		let (sender, receiver) = mpsc::channel();

		receiver_loop(handler, move |message|
        {
            if let Err(_) = sender.send(message)
            {
                process::exit(0);
            }
        }, || ());

		Self{
			mouse_position,
			camera,
            assets,
			object_factory,
			notifications,
            entities,
            controls,
			running: true,
			debug_mode: client_info.debug_mode,
            tilemap,
			world,
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

	fn connect_to_server(handler: Arc<RwLock<ConnectionsHandler>>, name: &str) -> Entity
	{
		let message = Message::PlayerConnect{name: name.to_owned()};

		let mut handler = handler.write();

		if let Err(x) = handler.send_blocking(&message)
		{
			panic!("error connecting to server: {x}");
		}

		match handler.receive_blocking()
		{
			Ok(Some(Message::PlayerOnConnect{entity})) => entity,
			x => panic!("received wrong message on connect: {x:?}")
		}
	}

    /*pub fn damage_entity(&mut self, id: EntityType, damage: Damage)
    {
        if id.is_player()
        {
            return;
        }

        self.send_message(Message::EntityDamage{id, damage: damage.clone()});

        self.entities.damage(id, damage);
    }*/

    pub fn entities(&self) -> &Entities
    {
        &self.entities.entities
    }

    pub fn entities_mut(&mut self) -> &mut Entities
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
		let message = match self.entities.handle_message(message)
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

    /*pub fn remove_client_entity(&self, id: EntityType)
    {
        self.send_message(Message::EntityDestroy{id});
    }

    pub fn add_client_entity(&self, entity: EntityAny)
    {
        self.send_message(Message::EntityAdd{entity});
    }*/

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

		self.entities.update_buffers(info);
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

		self.world.rescale(camera.aspect());
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
