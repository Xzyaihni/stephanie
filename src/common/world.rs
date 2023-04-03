use std::{
	iter,
	thread,
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

use chunk::{
	CHUNK_SIZE,
	CHUNK_VISUAL_SIZE,
	TILE_SIZE,
	Pos3,
	Chunk,
	GlobalPos,
	LocalPos
};

use vertical_chunk::VerticalChunk;

pub mod chunk;
mod vertical_chunk;


pub const OVERMAP_SIZE: usize = 5;
pub const OVERMAP_HALF: i32 = OVERMAP_SIZE as i32 / 2;
pub const OVERMAP_VOLUME: usize = OVERMAP_SIZE * OVERMAP_SIZE * OVERMAP_SIZE;

#[derive(Debug)]
pub struct Overmap
{
	world_receiver: WorldReceiver,
	tiles_factory: Arc<Mutex<TilesFactory>>,
	chunks: Vec<Option<Arc<Chunk>>>,
	vertical_chunks: Arc<Mutex<Vec<VerticalChunk>>>,
	size: (f32, f32),
	player_position: Arc<RwLock<GlobalPos>>
}

impl Overmap
{
	pub fn new(
		world_receiver: WorldReceiver,
		tiles_factory: TilesFactory,
		size: (f32, f32),
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

		let player_position = Arc::new(RwLock::new(Self::global_player_position(player_position)));

		let mut this = Self{
			world_receiver,
			tiles_factory,
			chunks,
			vertical_chunks,
			size,
			player_position
		};

		this.generate_missing();

		this
	}

	pub fn player_moved(&mut self, player_position: Pos3<f32>)
	{
		let player_position = Self::global_player_position(player_position);

		let old_position = *self.player_position.read();
		if player_position != old_position
		{
			*self.player_position.write() = player_position;

			self.position_offset(player_position - old_position);
		}
	}

	pub fn generate_missing(&mut self)
	{
		let player_pos = *self.player_position.read();
		self.chunks.iter().enumerate().filter(|(_, chunk)| chunk.is_none())
			.for_each(|(index, _)|
			{
				let local_pos = Self::index_to_pos(index);
				let global_pos = Self::to_global_associated(local_pos, player_pos);

				self.world_receiver.request_chunk(global_pos);
			});
	}

	fn position_offset(&mut self, offset: GlobalPos)
	{
		self.shift_chunks(offset);
		self.generate_missing();
	}

	fn shift_chunks(&mut self, offset: GlobalPos)
	{
		Self::conditional_overmap(offset.0.z < 0).flat_map(|z|
		{
			Self::conditional_overmap(offset.0.y < 0).flat_map(move |y|
			{
				Self::conditional_overmap(offset.0.x < 0).map(move |x| LocalPos::new(x, y, z))
			})
		}).for_each(|old_local|
		{
			let old_index = Self::to_index(old_local);
			//early return if the chunk is empty
			if self.chunks[old_index].is_none()
			{
				return;
			}

			let old_position = self.to_global(old_local);
			let position = old_position - offset;

			if let Some(local_pos) = self.to_local(position)
			{
				//move the chunk to the new position
				self.swap_nonvisual_chunks(old_local, local_pos);

				if old_local.0.z == 0
				{
					self.swap_visual_chunks(old_local, local_pos);
				}
			} else
			{
				//chunk now outside the player range, remove it
				self.remove_nonvisual_chunk(old_local);

				if old_local.0.z == 0
				{
					self.remove_visual_chunk(old_local);
				}
			}
		});
	}

	fn conditional_overmap(reversed: bool) -> impl Iterator<Item=usize>
	{
		let (mut start, step) = if reversed
		{
			(OVERMAP_SIZE - 1, -1)
		} else
		{
			(0, 1)
		};

		iter::repeat_with(move ||
		{
			let return_value = start;
			start = (start as i32 + step) as usize;

			return_value
		}).take(OVERMAP_SIZE)
	}

	#[allow(dead_code)]
	fn remove_chunk(&mut self, pos: LocalPos)
	{
		self.remove_nonvisual_chunk(pos);
		self.remove_visual_chunk(pos);
	}

	fn remove_nonvisual_chunk(&mut self, pos: LocalPos)
	{
		let index = Self::to_index(pos);
		self.chunks[index] = None;
	}

	fn remove_visual_chunk(&mut self, pos: LocalPos)
	{
		let flat_index = Self::to_flat_index(pos);
		self.vertical_chunks.lock()[flat_index] = VerticalChunk::new();
	}

	#[allow(dead_code)]
	fn swap_chunks(&mut self, a: LocalPos, b: LocalPos)
	{
		self.swap_nonvisual_chunks(a, b);
		self.swap_visual_chunks(a, b);
	}

	fn swap_nonvisual_chunks(&mut self, a: LocalPos, b: LocalPos)
	{
		let (index_a, index_b) = (Self::to_index(a), Self::to_index(b));

		self.chunks.swap(index_a, index_b);
	}

	fn swap_visual_chunks(&mut self, a: LocalPos, b: LocalPos)
	{
		let mut vertical_chunks = self.vertical_chunks.lock();

		let (index_a, index_b) = (Self::to_flat_index(a), Self::to_flat_index(b));
		vertical_chunks.swap(index_a, index_b);
	}

	fn set_chunk(&mut self, pos: GlobalPos, chunk: Chunk)
	{
		chunk.get_tile(LocalPos::new(0, 0, 0));

		if let Some(local_pos) = self.to_local(pos)
		{
			let index = Self::to_index(local_pos);

			self.chunks[index] = Some(Arc::new(chunk));

			let line_full = (0..OVERMAP_SIZE).all(|z|
			{
				let pos = LocalPos::new(local_pos.0.x, local_pos.0.y, z);

				self.chunks[Self::to_index(pos)].is_some()
			});

			if line_full
			{
				self.update_vertical(local_pos);
			}
		}
	}

	fn update_vertical(&self, pos: LocalPos)
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

			let player_position = player_position.read();
			if player_height != player_position.0.z
			{
				return;
			}

			if let Some(local_pos) = Self::to_local_associated(chunk_pos, *player_position)
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
		let player_distance = pos - player_position;

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
		Self::to_global_associated(pos, *self.player_position.read())
	}

	fn to_global_associated(pos: LocalPos, player_position: GlobalPos) -> GlobalPos
	{
		Self::player_offset(pos) + player_position
	}

	fn player_offset(pos: LocalPos) -> GlobalPos
	{
		let LocalPos(pos) = pos;

		GlobalPos::new(
			pos.x as i32 - OVERMAP_HALF,
			pos.y as i32 - OVERMAP_HALF,
			pos.z as i32 - OVERMAP_HALF
		)
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
		let size = CHUNK_SIZE as f32 * TILE_SIZE;

		let round_left = |value| if value < 0.0 {value as i32 - 1} else {value as i32};
		GlobalPos::new(
			round_left(pos.x / size),
			round_left(pos.y / size),
			round_left(pos.z / size)
		)
	}

	pub fn rescale(&mut self, size: (f32, f32))
	{
		self.size = size;
	}

	pub fn visible(&self, pos: LocalPos) -> bool
	{
		let GlobalPos(chunk_offset) = Self::player_offset(pos);

		let in_range = |value: i32, limit: f32| -> bool
		{
			let visual_position = value as f32 * CHUNK_VISUAL_SIZE;

			(visual_position.abs() - CHUNK_VISUAL_SIZE) < limit
		};

		in_range(chunk_offset.x, self.size.0) && in_range(chunk_offset.y, self.size.1)
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
			self.visible(Self::index_to_flat_pos(*index))
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
		size: (f32, f32),
		player_position: Pos3<f32>
	) -> Self
	{
		Self{overmap: Overmap::new(world_receiver, tiles_factory, size, player_position)}
	}

	pub fn rescale(&mut self, size: (f32, f32))
	{
		self.overmap.rescale(size);
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