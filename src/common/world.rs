use std::{
	thread,
	mem,
	sync::Arc
};

use parking_lot::{RwLock, Mutex};

use vulkano::memory::allocator::FastMemoryAllocator;

use crate::{
	client::{
		GameObject,
		BuilderType,
		TilesFactory,
		world_receiver::WorldReceiver
	},
	common::message::Message
};

use chunk::{CHUNK_SIZE, Pos3, Chunk, GlobalPos, LocalPos};
use vertical_chunk::VerticalChunk;

pub mod chunk;
mod vertical_chunk;


pub const OVERMAP_SIZE: usize = 3;
pub const OVERMAP_HALF: i32 = OVERMAP_SIZE as i32 / 2;
pub const OVERMAP_VOLUME: usize = OVERMAP_SIZE * OVERMAP_SIZE * OVERMAP_SIZE;

#[derive(Debug)]
pub struct Overmap
{
	world_receiver: WorldReceiver,
	tiles_factory: Arc<Mutex<TilesFactory>>,
	chunks: Vec<Option<Arc<Chunk>>>,
	vertical_chunks: Arc<Mutex<Vec<VerticalChunk>>>,
	ungenerated: Vec<usize>,
	aspect: (f32, f32),
	player_position: Arc<RwLock<GlobalPos>>
}

impl Overmap
{
	pub fn new(
		world_receiver: WorldReceiver,
		tiles_factory: TilesFactory,
		aspect: (f32, f32),
		player_position: Pos3<f32>
	) -> Self
	{
		let tiles_factory = Arc::new(Mutex::new(tiles_factory));

		let chunks = (0..OVERMAP_VOLUME).map(|_| None)
			.collect();

		let vertical_chunks = Arc::new(
			Mutex::new(
				(0..(OVERMAP_SIZE * OVERMAP_SIZE)).map(|_| VerticalChunk::new())
					.collect()
			)
		);

		let ungenerated = (0..OVERMAP_VOLUME).collect();

		let player_position = Arc::new(RwLock::new(Self::global_player_position(player_position)));

		Self{
			world_receiver,
			tiles_factory,
			chunks,
			vertical_chunks,
			ungenerated,
			aspect,
			player_position
		}
	}

	pub fn player_moved(&mut self, player_position: Pos3<f32>)
	{
		let player_position = Self::global_player_position(player_position);

		*self.player_position.write() = player_position;
	}

	pub fn generate_missing(&mut self)
	{
		mem::take(&mut self.ungenerated).into_iter().for_each(|index|
		{
			let local_pos = Self::index_to_pos(index);

			self.world_receiver.request_chunk(self.to_global(local_pos));
		});
	}

	fn set_chunk(&mut self, pos: GlobalPos, chunk: Chunk)
	{
		if let Some(local_pos) = self.to_local(pos)
		{
			let index = Self::to_index(local_pos);

			self.chunks[index] = Some(Arc::new(chunk));
			self.update_chunks(local_pos);
		}
	}

	fn update_chunks(&self, pos: LocalPos)
	{
		let LocalPos(Pos3{x, y, ..}) = pos;

		let chunks = (0..=OVERMAP_HALF as usize).rev().filter_map(|z|
		{
			let index = Self::to_index(LocalPos::new(x, y, z));

			self.chunks[index].clone()
		}).collect::<Vec<_>>();

		let chunk_pos = self.to_global(pos);

		let player_height = self.player_position.read().0.z;

		let height = (player_height) % CHUNK_SIZE as i32;
		let height = if height < 0
		{
			CHUNK_SIZE as i32 + height
		} else
		{
			height
		} as usize;

		let tiles_factory = self.tiles_factory.clone();

		let player_position = self.player_position.clone();
		let vertical_chunks = self.vertical_chunks.clone();

		thread::spawn(move ||
		{
			let mut tiles_factory = tiles_factory.lock();

			let (info_map, model_builder) = tiles_factory.build_info(player_height);

			let vertical_chunk = VerticalChunk::regenerate(
				info_map,
				model_builder,
				height,
				chunk_pos,
				chunks.iter()
			);

			let player_position = *player_position.read();
			if player_height != player_position.0.z
			{
				return;
			}

			if let Some(local_pos) = Self::to_local_associated(chunk_pos, player_position)
			{
				let index = Self::to_flat_index(local_pos);

				vertical_chunks.lock()[index] = vertical_chunk;
			}
		});
	}

	fn to_local(&self, pos: GlobalPos) -> Option<LocalPos>
	{
		Self::to_local_associated(pos, *self.player_position.read())
	}

	fn to_local_associated(pos: GlobalPos, player_position: GlobalPos)  -> Option<LocalPos>
	{
		let player_distance = pos + player_position;

		let GlobalPos(pos) = player_distance;
		let centered = GlobalPos::new(
			pos.x + OVERMAP_HALF,
			pos.y + OVERMAP_HALF,
			pos.z + OVERMAP_HALF
		);

		LocalPos::from_global(centered, OVERMAP_SIZE as i32)
	}

	fn to_global(&self, pos: LocalPos) -> GlobalPos
	{
		let LocalPos(pos) = pos;

		let shifted = GlobalPos::new(
			pos.x as i32 - OVERMAP_HALF,
			pos.y as i32 - OVERMAP_HALF,
			pos.z as i32 - OVERMAP_HALF
		);

		shifted - *self.player_position.read()
	}

	fn to_index(pos: LocalPos) -> usize
	{
		pos.to_cube(OVERMAP_SIZE)
	}

	fn to_flat_index(pos: LocalPos) -> usize
	{
		let LocalPos(pos) = pos;

		pos.y * OVERMAP_SIZE + pos.x
	}

	fn index_to_pos(index: usize) -> LocalPos
	{
		let x = index % OVERMAP_SIZE;
		let y = (index / OVERMAP_SIZE) % OVERMAP_SIZE;
		let z = index / (OVERMAP_SIZE * OVERMAP_SIZE);

		LocalPos::new(x, y, z)
	}

	fn index_to_flat_pos(index: usize) -> LocalPos
	{
		let x = index % OVERMAP_SIZE;
		let y = index / OVERMAP_SIZE;

		LocalPos::new(x, y, 0)
	}

	fn global_player_position(pos: Pos3<f32>) -> GlobalPos
	{
		let size = CHUNK_SIZE as f32;

		let round_left = |value| if value < 0.0 {value as i32 - 1} else {value as i32};
		GlobalPos::new(
			round_left(pos.x / size),
			round_left(pos.y / size),
			round_left(pos.z / size)
		)
	}

	pub fn resize(&mut self, aspect: (f32, f32))
	{
		self.aspect = aspect;
	}
}

impl GameObject for Overmap
{
	fn update(&mut self, dt: f32)
	{
		self.vertical_chunks.lock().iter_mut().for_each(|chunk| chunk.update(dt));
	}

	fn regenerate_buffers(&mut self, allocator: &FastMemoryAllocator)
	{
		self.vertical_chunks.lock().iter_mut().for_each(|chunk|
		{
			chunk.regenerate_buffers(allocator)
		});
	}

	fn draw(&self, builder: BuilderType)
	{
		self.vertical_chunks.lock().iter().enumerate().filter(|(index, _)|
		{
			let GlobalPos(chunk_pos) = self.to_global(Self::index_to_flat_pos(*index));

			let in_range = |value: i32, limit: f32| value.abs() <= limit.ceil() as i32;

			in_range(chunk_pos.x, self.aspect.0) && in_range(chunk_pos.y, self.aspect.1)
		}).for_each(|(_, chunk)| chunk.draw(builder));
	}
}

#[derive(Debug)]
pub struct World
{
	overmap: Overmap
}

impl World
{
	pub fn new(
		world_receiver: WorldReceiver,
		tiles_factory: TilesFactory,
		aspect: (f32, f32),
		player_position: Pos3<f32>
	) -> Self
	{
		Self{overmap: Overmap::new(world_receiver, tiles_factory, aspect, player_position)}
	}

	pub fn resize(&mut self, aspect: (f32, f32))
	{
		self.overmap.resize(aspect);
	}

	pub fn player_moved(&mut self, pos: Pos3<f32>)
	{
		self.overmap.player_moved(pos);
	}

	pub fn handle_message(&mut self, message: Message) -> Option<Message>
	{
		match message
		{
			Message::ChunkSync{pos, chunk} =>
			{
				self.overmap.set_chunk(pos, chunk);
				None
			},
			_ => Some(message)
		}
	}
}

impl GameObject for World
{
	fn update(&mut self, dt: f32)
	{
		self.overmap.generate_missing();

		self.overmap.update(dt);
	}

	fn regenerate_buffers(&mut self, allocator: &FastMemoryAllocator)
	{
		self.overmap.regenerate_buffers(allocator);
	}

	fn draw(&self, builder: BuilderType)
	{
		self.overmap.draw(builder);
	}
}