use std::{
	iter,
	thread,
	sync::Arc
};

use parking_lot::{RwLock, Mutex};

use vulkano::memory::allocator::StandardMemoryAllocator;

use crate::{
	client::{
		GameObject,
		BuilderType,
		LayoutType,
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

type OvermapLocal = LocalPos<OVERMAP_SIZE>;

#[derive(Debug)]
pub struct Overmap
{
	world_receiver: WorldReceiver,
	tiles_factory: Arc<Mutex<TilesFactory>>,
	chunks: Vec<Option<Arc<Chunk>>>,
	chunk_ordering: Vec<usize>,
	vertical_chunks: Arc<Mutex<Vec<VerticalChunk>>>,
	size: (f32, f32),
	player_position: Arc<RwLock<GlobalPos>>,
	visual_player_position: Pos3<f32>
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

		let visual_player_position = player_position;
		let player_position = Self::global_player_position(player_position);

		let mut chunk_ordering = (0..OVERMAP_VOLUME).collect::<Vec<_>>();
		chunk_ordering.sort_unstable_by(move |a, b|
		{
			let distance = |value: usize| -> f32
			{
				let local_pos = Self::index_to_pos(value);

				let GlobalPos(pos) = Self::to_global_associated(local_pos, player_position);

				((pos.x.pow(2) + pos.y.pow(2) + pos.z.pow(2)) as f32).sqrt()
			};

			distance(*a).total_cmp(&distance(*b))
		});

		let vertical_chunks = Arc::new(
			Mutex::new(
				(0..(OVERMAP_SIZE * OVERMAP_SIZE)).map(|_| VerticalChunk::new())
					.collect()
			)
		);

		let player_position = Arc::new(RwLock::new(player_position));

		let mut this = Self{
			world_receiver,
			tiles_factory,
			chunks,
			chunk_ordering,
			vertical_chunks,
			size,
			player_position,
			visual_player_position
		};

		this.generate_missing();

		this
	}

	pub fn player_moved(&mut self, player_position: Pos3<f32>)
	{
		self.visual_player_position = player_position;
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

		self.chunk_ordering.iter().filter(|index| self.chunks[**index].is_none())
			.for_each(|index|
			{
				let local_pos = Self::index_to_pos(*index);
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
	fn remove_chunk(&mut self, pos: OvermapLocal)
	{
		self.remove_nonvisual_chunk(pos);
		self.remove_visual_chunk(pos);
	}

	fn remove_nonvisual_chunk(&mut self, pos: OvermapLocal)
	{
		let index = Self::to_index(pos);
		self.chunks[index] = None;
	}

	fn remove_visual_chunk(&mut self, pos: OvermapLocal)
	{
		let flat_index = Self::to_flat_index(pos);
		self.vertical_chunks.lock()[flat_index] = VerticalChunk::new();
	}

	#[allow(dead_code)]
	fn swap_chunks(&mut self, a: OvermapLocal, b: OvermapLocal)
	{
		self.swap_nonvisual_chunks(a, b);
		self.swap_visual_chunks(a, b);
	}

	fn swap_nonvisual_chunks(&mut self, a: OvermapLocal, b: OvermapLocal)
	{
		let (index_a, index_b) = (Self::to_index(a), Self::to_index(b));

		self.chunks.swap(index_a, index_b);
	}

	fn swap_visual_chunks(&mut self, a: OvermapLocal, b: OvermapLocal)
	{
		let mut vertical_chunks = self.vertical_chunks.lock();

		let (index_a, index_b) = (Self::to_flat_index(a), Self::to_flat_index(b));
		vertical_chunks.swap(index_a, index_b);
	}

	fn set_chunk(&mut self, pos: GlobalPos, chunk: Chunk)
	{
		if let Some(local_pos) = self.to_local(pos)
		{
			let index = Self::to_index(local_pos);

			self.chunks[index] = Some(Arc::new(chunk));

			self.recursive_check_vertical(local_pos);
		}
	}

	fn line_exists(&self, pos: OvermapLocal) -> bool
	{
		(0..OVERMAP_SIZE).all(|z|
		{
			let pos = OvermapLocal::new(pos.0.x, pos.0.y, z);

			self.chunks[Self::to_index(pos)].is_some()
		})
	}

	fn recursive_check_vertical(&self, pos: OvermapLocal)
	{
		pos.directions_inclusive().flatten().for_each(|position|
			self.check_vertical(position)
		);
	}

	fn check_vertical(&self, pos: OvermapLocal)
	{
		let ready_to_draw = pos.directions_inclusive().flatten().all(|pos|
			self.line_exists(pos)
		);

		if ready_to_draw
		{
			self.draw_vertical(pos);
		}
	}

	fn draw_vertical(&self, pos: OvermapLocal)
	{
		let LocalPos(Pos3{x, y, ..}) = pos;

		let chunks = (0..=OVERMAP_HALF as usize).rev().map(|z|
		{
			let local_pos = OvermapLocal::new(x, y, z);
			local_pos.directions_inclusive_group(|position|
			{
				let index = Self::to_index(position);

				self.chunks[index].clone().unwrap()
			})
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

			let (info_map, model_builder) = tiles_factory.build_info();

			let vertical_chunk = VerticalChunk::regenerate(
				info_map,
				model_builder,
				height,
				chunk_pos,
				&chunks
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

	fn to_local(&self, pos: GlobalPos) -> Option<OvermapLocal>
	{
		Self::to_local_associated(pos, *self.player_position.read())
	}

	fn to_local_associated(pos: GlobalPos, player_position: GlobalPos)  -> Option<OvermapLocal>
	{
		let player_distance = pos - player_position;

		let GlobalPos(pos) = player_distance;
		let centered = GlobalPos::new(
			pos.x + OVERMAP_HALF,
			pos.y + OVERMAP_HALF,
			pos.z + OVERMAP_HALF
		);

		OvermapLocal::from_global(centered, OVERMAP_SIZE as i32)
	}

	fn to_global(&self, pos: OvermapLocal) -> GlobalPos
	{
		Self::to_global_associated(pos, *self.player_position.read())
	}

	fn to_global_associated(pos: OvermapLocal, player_position: GlobalPos) -> GlobalPos
	{
		Self::player_offset(pos) + player_position
	}

	fn player_offset(pos: OvermapLocal) -> GlobalPos
	{
		let LocalPos(pos) = pos;

		GlobalPos::new(
			pos.x as i32 - OVERMAP_HALF,
			pos.y as i32 - OVERMAP_HALF,
			pos.z as i32 - OVERMAP_HALF
		)
	}

	fn to_index(pos: OvermapLocal) -> usize
	{
		pos.to_cube(OVERMAP_SIZE)
	}

	fn to_flat_index(pos: OvermapLocal) -> usize
	{
		let LocalPos(pos) = pos;

		pos.y * OVERMAP_SIZE + pos.x
	}

	fn index_to_pos(index: usize) -> OvermapLocal
	{
		let x = index % OVERMAP_SIZE;
		let y = (index / OVERMAP_SIZE) % OVERMAP_SIZE;
		let z = index / (OVERMAP_SIZE * OVERMAP_SIZE);

		OvermapLocal::new(x, y, z)
	}

	fn index_to_flat_pos(index: usize) -> OvermapLocal
	{
		let x = index % OVERMAP_SIZE;
		let y = index / OVERMAP_SIZE;

		OvermapLocal::new(x, y, 0)
	}

	fn global_player_position(pos: Pos3<f32>) -> GlobalPos
	{
		GlobalPos::new(
			Self::coordinate_to_global(pos.x),
			Self::coordinate_to_global(pos.y),
			Self::coordinate_to_global(pos.z)
		)
	}

	fn coordinate_to_global(coordinate: f32) -> i32
	{
		let size = CHUNK_SIZE as f32 * TILE_SIZE;
		let coordinate = coordinate / size;

		if coordinate < 0.0
		{
			coordinate as i32 - 1
		} else
		{
			coordinate as i32
		}
	}

	pub fn rescale(&mut self, size: (f32, f32))
	{
		self.size = size;
	}

	pub fn visible(&self, pos: GlobalPos) -> bool
	{
		let chunk_offset = self.visual_chunk_offset(Self::visual_chunk(pos));

		let in_range = |value: f32, limit: f32| -> bool
		{
			let half_limit = limit * 0.5;

			let bottom_right_corner =
				(value < 0.0) && (value + CHUNK_VISUAL_SIZE > -half_limit);

			bottom_right_corner || (value.abs() < half_limit)
		};

		in_range(chunk_offset.x, self.size.0) && in_range(chunk_offset.y, self.size.1)
	}

	fn visual_chunk_offset(&self, pos: Pos3<f32>) -> Pos3<f32>
	{
		Pos3::new(
			pos.x - self.visual_player_position.x,
			pos.y - self.visual_player_position.y,
			pos.z - self.visual_player_position.z
		)
	}

	fn visual_chunk(pos: GlobalPos) -> Pos3<f32>
	{
		let GlobalPos(pos) = pos;

		Pos3::new(
			pos.x as f32 * CHUNK_VISUAL_SIZE,
			pos.y as f32 * CHUNK_VISUAL_SIZE,
			pos.z as f32 * CHUNK_VISUAL_SIZE
		)
	}
}

impl GameObject for Overmap
{
	fn update(&mut self, dt: f32)
	{
		self.vertical_chunks.lock().iter_mut().for_each(|chunk| chunk.update(dt));
	}

	fn regenerate_buffers(&mut self, allocator: &StandardMemoryAllocator)
	{
		self.vertical_chunks.lock().iter_mut().for_each(|chunk|
		{
			chunk.regenerate_buffers(allocator)
		});
	}

	fn draw(&self, builder: BuilderType, layout: LayoutType)
	{
		self.vertical_chunks.lock().iter().enumerate().filter(|(index, _)|
		{
			let chunk_pos = self.to_global(Self::index_to_flat_pos(*index));

			self.visible(chunk_pos)
		}).for_each(|(_, chunk)| chunk.draw(builder, layout.clone()));
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

	fn regenerate_buffers(&mut self, allocator: &StandardMemoryAllocator)
	{
		self.overmap.regenerate_buffers(allocator);
	}

	fn draw(&self, builder: BuilderType, layout: LayoutType)
	{
		self.overmap.draw(builder, layout);
	}
}