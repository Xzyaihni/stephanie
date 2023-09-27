use std::sync::{
    Arc,
    mpsc::{self, TryRecvError, Receiver}
};

use parking_lot::RwLock;

use slab::Slab;

use crate::common::{
	sender_loop,
	receiver_loop,
	TransformContainer,
	EntitiesContainer,
	EntitiesController,
	message::Message,
	player::Player,
	world::{
		World,
		Pos3
	}
};

use super::{
	GameObject,
	game_object_types::*,
	ClientInfo,
	MessagePasser,
	ConnectionsHandler,
	TilesFactory,
	world_receiver::WorldReceiver,
	game::{
		ObjectFactory,
		camera::Camera,
		object::resource_uploader::DescriptorSetUploader
	}
};

use object_pair::ObjectPair;
use notifications::{Notifications, Notification};

pub use controls::{Control, ControlState};

pub mod object_pair;
mod notifications;

pub mod controls;


#[derive(Debug)]
pub struct ClientEntitiesContainer
{
	players: Slab<ObjectPair<Player>>,
	main_player: Option<usize>
}

impl ClientEntitiesContainer
{
	pub fn new() -> Self
	{
		let players = Slab::new();
		let main_player = None;

		Self{players, main_player}
	}

	pub fn player_exists(&self, id: usize) -> bool
	{
		self.players.contains(id)
	}
}

impl GameObject for ClientEntitiesContainer
{
	fn update(&mut self, dt: f32)
	{
		self.players.iter_mut().for_each(|(_, pair)| pair.update(dt));
	}

	fn update_buffers(&mut self, builder: BuilderType, index: usize)
	{
		self.players.iter_mut().for_each(|(_, pair)| pair.update_buffers(builder, index));
	}

	fn draw(&self, builder: BuilderType, layout: LayoutType, index: usize)
	{
		if let Some(player_id) = self.main_player
		{
			self.players.iter().filter(|(id, _)| *id != player_id)
				.for_each(|(_, pair)| pair.draw(builder, layout.clone(), index));

			self.players[player_id].draw(builder, layout, index);
		} else
		{
			self.players.iter().for_each(|(_, pair)|
				pair.draw(builder, layout.clone(), index)
			);
		}
	}
}

impl EntitiesContainer for ClientEntitiesContainer
{
	type PlayerObject = ObjectPair<Player>;

	fn players_ref(&self) -> &Slab<Self::PlayerObject>
	{
		&self.players
	}

	fn players_mut(&mut self) -> &mut Slab<Self::PlayerObject>
	{
		&mut self.players
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

#[derive(Debug)]
pub struct GameState
{
	pub controls: [ControlState; Control::COUNT],
	pub mouse_position: MousePosition,
	pub camera: Arc<RwLock<Camera>>,
	pub object_factory: ObjectFactory,
	pub notifications: Notifications,
	pub entities: ClientEntitiesContainer,
	pub running: bool,
	pub debug_mode: bool,
	player_id: usize,
	world: World,
	connections_handler: Arc<RwLock<ConnectionsHandler>>,
	receiver: Receiver<Message>
}

impl GameState
{
	pub fn new(
		camera: Arc<RwLock<Camera>>,
		object_factory: ObjectFactory,
		tiles_factory: TilesFactory,
		message_passer: MessagePasser,
		client_info: &ClientInfo
	) -> Self
	{
		let controls = [ControlState::Released; Control::COUNT];
		let mouse_position = MousePosition::new(0.0, 0.0);

		let notifications = Notifications::new();
		let entities = ClientEntitiesContainer::new();
		let connections_handler = Arc::new(RwLock::new(ConnectionsHandler::new(message_passer)));

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
			controls,
			mouse_position,
			camera,
			object_factory,
			notifications,
			entities,
			running: true,
			debug_mode: client_info.debug_mode,
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
			panic!("error connecting to server: {:?}", x);
		}

		match handler.receive_blocking()
		{
			Ok(Some(Message::PlayerOnConnect{id})) =>
			{
				id
			},
			x => panic!("received wrong message on connect: {:?}", x)
		}
	}

	pub fn player_id(&self) -> usize
	{
		self.player_id
	}

	pub fn process_messages(&mut self)
	{
		loop
		{
			match self.receiver.try_recv()
			{
				Ok(message) =>
				{
					self.process_message_inner(message);
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

	fn process_message_inner(&mut self, message: Message)
	{
		let id_mismatch = || panic!("id mismatch in clientside process message");

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
			Message::PlayerCreate{id, player} =>
			{
				let player = ObjectPair::new(&self.object_factory, player);

				if id != self.entities.players_mut().insert(player)
				{
					id_mismatch();
				}
			},
			Message::PlayerFullyConnected =>
			{
				self.notifications.set(Notification::PlayerConnected);
			},
			x => panic!("unhandled message: {:?}", x)
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

	pub fn player_connected(&mut self) -> bool
	{
		self.notifications.get(Notification::PlayerConnected)
	}

	pub fn swap_pipeline(&mut self, uploader: &DescriptorSetUploader)
	{
		self.object_factory.swap_pipeline(uploader);
	}

	pub fn pressed(&self, control: Control) -> bool
	{
		self.controls[control as usize].active()
	}

	#[allow(dead_code)]
	pub fn clicked(&mut self, control: Control) -> bool
	{
		let held = matches!(self.controls[control as usize], ControlState::Held);

		if held
		{
			self.controls[control as usize] = ControlState::Locked;
		}

		held
	}

	pub fn release_clicked(&mut self)
	{
		// or i can just keep a vec of keys to release but wutever
		self.controls.iter_mut().filter(|x| **x == ControlState::Clicked).for_each(|clicked|
		{
			*clicked = ControlState::Released;
		});
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

impl GameObject for GameState
{
	fn update(&mut self, dt: f32)
	{
		self.process_messages();

		self.check_resize_camera(dt);
		self.camera_moved();

		self.world.update(dt);

		self.entities.update(dt);
	}

	fn update_buffers(&mut self, builder: BuilderType, index: usize)
	{
		self.world.update_buffers(builder, index);

		self.entities.update_buffers(builder, index);
	}

	fn draw(&self, builder: BuilderType, layout: LayoutType, index: usize)
	{
		self.world.draw(builder, layout.clone(), index);

		self.entities.draw(builder, layout, index);
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
