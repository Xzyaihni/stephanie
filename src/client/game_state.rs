use std::{
	sync::Arc
};

use parking_lot::RwLock;

use slab::Slab;

use vulkano::memory::allocator::FastMemoryAllocator;

use crate::common::{
	sender_loop,
	receiver_loop,
	EntitiesContainer,
	EntitiesController,
	message::Message,
	player::Player,
	physics::PhysicsEntity,
	world::{World, chunk::Pos3}
};

use super::{
	GameObject,
	BuilderType,
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
	players: Slab<ObjectPair<Player>>
}

impl ClientEntitiesContainer
{
	pub fn new() -> Self
	{
		let players = Slab::new();

		Self{players}
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

	fn regenerate_buffers(&mut self, allocator: &FastMemoryAllocator)
	{
		self.players.iter_mut().for_each(|(_, pair)| pair.regenerate_buffers(allocator));
	}

	fn draw(&self, builder: BuilderType)
	{
		self.players.iter().for_each(|(_, pair)| pair.object.draw(builder));
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

#[derive(Debug)]
pub struct GameState
{
	pub controls: [ControlState; controls::COUNT],
	pub camera: Arc<RwLock<Camera>>,
	pub object_factory: ObjectFactory,
	pub notifications: Notifications,
	pub entities: ClientEntitiesContainer,
	pub running: bool,
	world: World,
	connection_handler: Arc<RwLock<ConnectionsHandler>>
}

impl GameState
{
	pub fn new(
		camera: Arc<RwLock<Camera>>,
		object_factory: ObjectFactory,
		tiles_factory: TilesFactory,
		message_passer: MessagePasser
	) -> Self
	{
		let controls = [ControlState::Released; controls::COUNT];

		let notifications = Notifications::new();
		let entities = ClientEntitiesContainer::new();
		let connection_handler = Arc::new(RwLock::new(ConnectionsHandler::new(message_passer)));

		let world_receiver = WorldReceiver::new(connection_handler.clone());
		let world = World::new(
			world_receiver,
			tiles_factory,
			camera.read().aspect(),
			Pos3::new(0.0, 0.0, 0.0)
		);

		Self{
			controls,
			camera,
			object_factory,
			notifications,
			entities,
			running: true,
			world,
			connection_handler
		}
	}

	pub fn connect(this: Arc<RwLock<Self>>, name: &str) -> usize
	{
		let message = Message::PlayerConnect{name: name.to_owned()};

		let player_id = {
			let reader = this.read();
			let mut handler = reader.connection_handler.write();

			if let Err(x) = handler.send(&message)
			{
				panic!("error connecting to server: {:?}", x);
			}

			match handler.receive()
			{
				Ok(Message::PlayerOnConnect{id}) =>
				{
					id
				},
				x => panic!("received wrong message on connect: {:?}", x)
			}
		};

		sender_loop(this.read().connection_handler.clone());

		let handler = this.read().connection_handler.read().passer_clone();
		Self::listen(this, handler, |this| this.write().running = false);

		player_id
	}

	fn listen<F>(this: Arc<RwLock<Self>>, handler: MessagePasser, exit_callback: F)
	where
		F: FnOnce(Arc<RwLock<Self>>) + Send + 'static
	{
		receiver_loop(this, handler, Self::process_message, exit_callback);
	}

	fn process_message(this: Arc<RwLock<Self>>, message: Message)
	{
		let id_mismatch = || panic!("id mismatch in clientside process message");

		let mut writer = this.write();

		let message = match writer.entities.handle_message(message)
		{
			Some(x) => x,
			None => return
		};

		let message = match writer.world.handle_message(message)
		{
			Some(x) => x,
			None => return
		};

		match message
		{
			Message::PlayerCreate{id, player} =>
			{
				let player = ObjectPair::new(&writer.object_factory, player);

				if id != writer.entities.players_mut().insert(player)
				{
					id_mismatch();
				}
			},
			Message::PlayerFullyConnected =>
			{
				writer.notifications.set(Notification::PlayerConnected);
			},
			x => panic!("unhandled message: {:?}", x)
		}
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

	pub fn release_clicked(&mut self)
	{
		// or i can just keep a vec of keys to release but wutever
		self.controls.iter_mut().filter(|x| **x == ControlState::Clicked).for_each(|clicked|
		{
			*clicked = ControlState::Released;
		});
	}

	pub fn player_moved(&mut self, pos: Pos3<f32>)
	{
		self.world.player_moved(pos);
	}

	pub fn resize(&mut self, aspect: f32)
	{
		let mut camera = self.camera.write();
		camera.resize(aspect);

		self.world.resize(camera.aspect());
	}
}

impl GameObject for GameState
{
	fn update(&mut self, dt: f32)
	{
		self.world.update(dt);

		self.entities.update(dt);
	}

	fn regenerate_buffers(&mut self, allocator: &FastMemoryAllocator)
	{
		self.world.regenerate_buffers(allocator);

		self.entities.regenerate_buffers(allocator);
	}

	fn draw(&self, builder: BuilderType)
	{
		self.world.draw(builder);

		self.entities.draw(builder);
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
		self.connection_handler.clone()
	}
}