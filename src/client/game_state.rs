use std::{
	sync::Arc
};

use parking_lot::RwLock;

use slab::Slab;

use vulkano::command_buffer::{AutoCommandBufferBuilder, PrimaryAutoCommandBuffer};

use crate::common::{
	sender_loop,
	receiver_loop,
	BufferSender,
	EntityPasser,
	EntitiesContainer,
	EntitiesController,
	message::{
		Message,
		MessageBuffer
	},
	player::Player,
	physics::PhysicsEntity
};

use super::{
	MessagePasser,
	game::{
		ObjectFactory,
		camera::Camera,
		object::resource_uploader::DescriptorSetUploader
	}
};

use object_pair::ObjectPair;
use notifications::{Notifications, Notification};

pub mod object_pair;
mod notifications;


#[repr(usize)]
#[derive(Debug, Clone, Copy)]
pub enum Control
{
	MoveUp = 0,
	MoveDown,
	MoveRight,
	MoveLeft,
	MainAction,
	SecondaryAction,
	LAST
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ControlState
{
	Held,
	Clicked,
	Released
}

impl ControlState
{
	pub fn active(self) -> bool
	{
		match self
		{
			ControlState::Released => false,
			_ => true
		}
	}
}

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

	pub fn regenerate_buffers(&mut self)
	{
		self.players.iter_mut().for_each(|(_, pair)| pair.object.regenerate_buffer());
	}

	pub fn update(&mut self, dt: f32)
	{
		self.players.iter_mut().for_each(|(_, pair)| pair.update(dt));
	}

	pub fn draw(&self, builder: &mut AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>)
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
pub struct ConnectionsHandler
{
	message_buffer: MessageBuffer,
	message_passer: MessagePasser
}

impl ConnectionsHandler
{
	pub fn new(message_passer: MessagePasser) -> Self
	{
		let message_buffer = MessageBuffer::new();

		Self{message_buffer, message_passer}
	}

	pub fn send(&mut self, message: &Message) -> Result<(), bincode::Error>
	{
		self.message_passer.send(message)
	}

	pub fn receive(&mut self) -> Result<Message, bincode::Error>
	{
		self.message_passer.receive()
	}

	pub fn passer_clone(&self) -> MessagePasser
	{
		self.message_passer.try_clone()
	}
}

impl EntityPasser for ConnectionsHandler
{
	fn send_message(&mut self, message: Message)
	{
		self.message_buffer.set_message(message);
	}
}

impl BufferSender for ConnectionsHandler
{
	fn send_buffered(&mut self) -> Result<(), bincode::Error>
	{
		self.message_buffer.get_buffered().try_for_each(|message|
		{
			self.message_passer.send(&message)
		})
	}
}

#[derive(Debug)]
pub struct GameState
{
	pub controls: [ControlState; Control::LAST as usize],
	pub camera: Arc<RwLock<Camera>>,
	pub object_factory: ObjectFactory,
	pub notifications: Notifications,
	pub entities: ClientEntitiesContainer,
	connection_handler: Arc<RwLock<ConnectionsHandler>>
}

impl GameState
{
	pub fn new(
		camera: Arc<RwLock<Camera>>,
		object_factory: ObjectFactory,
		message_passer: MessagePasser
	) -> Self
	{
		let controls = [ControlState::Released; Control::LAST as usize];

		let notifications = Notifications::new();
		let entities = ClientEntitiesContainer::new();
		let connection_handler = Arc::new(RwLock::new(ConnectionsHandler::new(message_passer)));

		Self{controls, camera, object_factory, notifications, entities, connection_handler}
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

		this.read().sender_loop();

		let handler = this.read().connection_handler.read().passer_clone();
		Self::listen(this, handler);

		player_id
	}

	fn sender_loop(&self)
	{
		sender_loop(self.connection_handler.clone());
	}

	fn listen(this: Arc<RwLock<Self>>, handler: MessagePasser)
	{
		receiver_loop(this, handler, Self::process_message, |_| ());
	}

	fn process_message(this: Arc<RwLock<Self>>, message: Message)
	{
		let id_mismatch = || panic!("id mismatch in clientside process message");

		let mut writer = this.write();

		let message = writer.entities.handle_message(message);

		if let Some(message) = message
		{
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
				_ => ()
			}
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