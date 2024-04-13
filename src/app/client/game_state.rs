use std::sync::{
    Arc,
    mpsc::{self, TryRecvError, Receiver}
};

use parking_lot::{RwLock, Mutex};

use yanyaengine::{
    Assets,
    ObjectFactory,
    camera::Camera,
    game_object::*
};

use crate::common::{
	sender_loop,
	receiver_loop,
    ObjectsStore,
    TileMap,
    EntityPasser,
	EntitiesContainer,
	EntitiesController,
	message::Message,
	player::Player,
    enemy::Enemy,
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

use object_pair::ObjectPair;
use notifications::{Notifications, Notification};

mod controls_controller;

pub mod object_pair;
mod notifications;


#[derive(Debug)]
pub struct ClientEntitiesContainer
{
	players: ObjectsStore<ObjectPair<Player>>,
	enemies: ObjectsStore<ObjectPair<Enemy>>,
	main_player: Option<usize>
}

impl ClientEntitiesContainer
{
	pub fn new() -> Self
	{
		let players = ObjectsStore::new();
		let enemies = ObjectsStore::new();
		let main_player = None;

		Self{players, enemies, main_player}
	}

	pub fn update(&mut self, dt: f32)
	{
		self.players.iter_mut().for_each(|(_, pair)| pair.update(dt));
		self.enemies.iter_mut().for_each(|(_, pair)| pair.update(dt));
	}

	pub fn player_exists(&self, id: usize) -> bool
	{
		self.players.contains(id)
	}
}

impl GameObject for ClientEntitiesContainer
{
	fn update_buffers(&mut self, info: &mut UpdateBuffersInfo)
    {
		self.players.iter_mut().for_each(|(_, pair)| pair.update_buffers(info));
		self.enemies.iter_mut().for_each(|(_, pair)| pair.update_buffers(info));
    }

	fn draw(&self, info: &mut DrawInfo)
    {
		if let Some(player_id) = self.main_player
		{
			self.players.iter().filter(|(id, _)| *id != player_id)
				.for_each(|(_, pair)| pair.draw(info));

			self.players[player_id].draw(info);
		} else
		{
			self.players.iter().for_each(|(_, pair)| pair.draw(info));
		}

        self.enemies.iter().for_each(|(_, pair)| pair.draw(info));
    }
}

impl EntitiesContainer for ClientEntitiesContainer
{
	type PlayerObject = ObjectPair<Player>;
	type EnemyObject = ObjectPair<Enemy>;

	fn players_ref(&self) -> &ObjectsStore<Self::PlayerObject>
	{
		&self.players
	}

	fn players_mut(&mut self) -> &mut ObjectsStore<Self::PlayerObject>
	{
		&mut self.players
	}

	fn enemies_ref(&self) -> &ObjectsStore<Self::EnemyObject>
	{
		&self.enemies
	}

	fn enemies_mut(&mut self) -> &mut ObjectsStore<Self::EnemyObject>
	{
		&mut self.enemies
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
}

impl From<(f64, f64)> for MousePosition
{
	fn from(value: (f64, f64)) -> Self
	{
		Self{x: value.0 as f32, y: value.1 as f32}
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
	player_id: usize,
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
		let entities = ClientEntitiesContainer::new();
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

		sender_loop(connections_handler.clone());

		let handler = connections_handler.read().passer_clone();

		let (sender, receiver) = mpsc::channel();

		receiver_loop(handler, move |message| sender.send(message).unwrap(), || ());

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
			player_id,
			world,
			connections_handler,
			receiver
		}
	}

	fn connect_to_server(handler: Arc<RwLock<ConnectionsHandler>>, name: &str) -> usize
	{
		let message = Message::PlayerConnect{name: name.to_owned()};

		let mut handler = handler.write();

		if let Err(x) = handler.send_blocking(&message)
		{
			panic!("error connecting to server: {x}");
		}

		match handler.receive_blocking()
		{
			Ok(Some(Message::PlayerOnConnect{id})) =>
			{
				id
			},
			x => panic!("received wrong message on connect: {x:?}")
		}
	}

	pub fn player_id(&self) -> usize
	{
		self.player_id
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
				Err(err) if err == TryRecvError::Empty =>
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
			Message::EnemyCreate{id, enemy} =>
			{
				let enemy = ObjectPair::new(create_info, enemy);

				self.entities.enemies_mut().insert(id, enemy);
			},
			Message::PlayerCreate{id, player} =>
			{
				let player = ObjectPair::new(create_info, player);

				self.entities.players_mut().insert(id, player);
			},
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

    pub fn add_client_enemy(&self, enemy: Enemy)
    {
        let id = self.entities.empty_enemy();
        self.echo_message(Message::EnemyCreate{id, enemy});
    }

    pub fn echo_message(&self, message: Message)
    {
        let message = Message::RepeatMessage{message: Box::new(message)};

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
            ControlState::Held => true,
            _ => false
        }
	}

	#[allow(dead_code)]
	pub fn clicked(&mut self, control: Control) -> bool
	{
        match self.controls.state(control)
        {
            ControlState::Clicked => true,
            _ => false
        }
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
