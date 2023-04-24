use std::{
	thread,
	sync::Arc
};

use parking_lot::{RwLock, Mutex};

use crate::{
	client::{
		TilesFactory,
		GameObject,
		game_object_types::*
	}
};

use super::{
	chunk::{
		CHUNK_SIZE,
		CHUNK_VISUAL_SIZE,
		Pos3,
		Chunk,
		GlobalPos,
		LocalPos
	},
	overmap::{
		OvermapIndexing,
		ChunksContainer,
		FlatChunksContainer,
		visual_chunk::VisualChunk
	}
};


#[derive(Debug)]
pub struct VisualOvermap<const SIZE: usize>
{
	tiles_factory: Arc<Mutex<TilesFactory>>,
	chunks: Arc<Mutex<FlatChunksContainer<SIZE, VisualChunk>>>,
	size: (f32, f32),
	player_position: Arc<RwLock<Pos3<f32>>>
}

impl<const SIZE: usize> VisualOvermap<SIZE>
{
	pub fn new(
		tiles_factory: TilesFactory,
		size: (f32, f32),
		player_position: Pos3<f32>
	) -> Self
	{
		let tiles_factory = Arc::new(Mutex::new(tiles_factory));

		let chunks = Arc::new(Mutex::new(
			FlatChunksContainer::new(VisualChunk::new)
		));

		let player_position = Arc::new(RwLock::new(player_position));

		Self{tiles_factory, chunks, size, player_position}
	}

	pub fn generate(&self, chunks: &ChunksContainer<SIZE, Option<Arc<Chunk>>>, pos: LocalPos<SIZE>)
	{
		let LocalPos(Pos3{x, y, ..}) = pos;

		let chunks = (0..=(SIZE / 2)).rev().map(|z|
		{
			let local_pos = LocalPos::new(x, y, z);
			local_pos.directions_inclusive_group(|position|
			{
				chunks[position].clone().unwrap()
			})
		}).collect::<Vec<_>>();

		let chunk_pos = self.to_global(pos);

		let player_height = self.player_position.read().rounded().0.z;

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
		let visual_chunks = self.chunks.clone();

		thread::spawn(move ||
		{
			let mut tiles_factory = tiles_factory.lock();

			let (info_map, model_builder) = tiles_factory.build_info();

			let vertical_chunk = VisualChunk::regenerate(
				info_map,
				model_builder,
				height,
				chunk_pos,
				&chunks
			);

			let player_position = player_position.read().rounded();
			if player_height != player_position.0.z
			{
				return;
			}

			if let Some(local_pos) = Self::to_local_associated(chunk_pos, player_position)
			{
				visual_chunks.lock()[local_pos] = vertical_chunk;
			}
		});
	}

	pub fn rescale(&mut self, size: (f32, f32))
	{
		self.size = size;
	}

	pub fn visible(&self, pos: LocalPos<SIZE>) -> bool
	{
		let player_offset = self.player_position.read().modulo(CHUNK_VISUAL_SIZE);

		let offset_position = Pos3::from(pos) - (SIZE / 2) as f32;
		let chunk_offset = offset_position * CHUNK_VISUAL_SIZE - player_offset;

		let in_range = |value: f32, limit: f32| -> bool
		{
			let limit = limit / 2.0;

			((-limit - CHUNK_VISUAL_SIZE)..limit).contains(&value)
		};

		in_range(chunk_offset.x, self.size.0) && in_range(chunk_offset.y, self.size.1)
	}

	pub fn player_moved(&mut self, player_position: Pos3<f32>)
	{
		*self.player_position.write() = player_position;
	}

	pub fn mark_ungenerated(&self, pos: LocalPos<SIZE>)
	{
		self.chunks.lock()[pos].mark_ungenerated();
	}

	pub fn is_generated(&self, pos: LocalPos<SIZE>) -> bool
	{
		self.chunks.lock()[pos].is_generated()
	}

	pub fn remove(&mut self, pos: LocalPos<SIZE>)
	{
		if pos.0.z == 0
		{
			self.chunks.lock()[pos] = VisualChunk::new();
		}
	}

	pub fn swap(&mut self, a: LocalPos<SIZE>, b: LocalPos<SIZE>)
	{
		if a.0.z == 0 && b.0.z == 0
		{
			self.chunks.lock().swap(a, b);
		}
	}
}

impl<const SIZE: usize> OvermapIndexing<SIZE> for VisualOvermap<SIZE>
{
	fn player_position(&self) -> GlobalPos
	{
		self.player_position.read().rounded()
	}
}

impl<const SIZE: usize> GameObject for VisualOvermap<SIZE>
{
	fn update(&mut self, dt: f32)
	{
		self.chunks.lock().iter_mut().for_each(|(_, chunk)| chunk.update(dt));
	}

	fn draw(&self, allocator: AllocatorType, builder: BuilderType, layout: LayoutType)
	{
		self.chunks.lock().iter().filter(|(pos, _)|
		{
			self.visible(*pos)
		}).for_each(|(_, chunk)| chunk.draw(allocator, builder, layout.clone()));
	}
}