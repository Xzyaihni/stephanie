use std::{
	sync::Arc
};

use super::{
	world_generator::{WorldGenerator, WorldChunk}
};

use crate::common::world::{
	LocalPos,
	GlobalPos,
	Pos3,
	Chunk,
	overmap::{Overmap, OvermapIndexing, ChunksContainer}
};


#[derive(Debug)]
pub struct ServerOvermap<const SIZE: usize>
{
	world_generator: Arc<WorldGenerator>,
	world_chunks: ChunksContainer<SIZE, Option<WorldChunk>>,
	player_position: GlobalPos
}

impl<const SIZE: usize> ServerOvermap<SIZE>
{
	pub fn new(
		world_generator: Arc<WorldGenerator>,
		player_position: Pos3<f32>
	) -> Self
	{
		let world_chunks = ChunksContainer::new(|_| None);

		let player_position = player_position.rounded();

		let mut this = Self{
			world_generator,
			world_chunks,
			player_position
		};

		this.generate_missing();

		this
	}

	pub fn generate_chunk(&mut self, pos: GlobalPos) -> Chunk
	{
		let margin = 1;
		let padding = 1;

		let over_edge = |value| -> i32
		{
			if value < padding
			{
				(value - padding) - margin
			} else if value >= (SIZE as i32 - padding)
			{
				value - (SIZE as i32 - padding) + 1 + margin
			} else
			{
				0
			}
		};

		let GlobalPos(difference) = pos - self.player_position + SIZE as i32 / 2;

		let shift_offset = GlobalPos::new(
			over_edge(difference.x),
			over_edge(difference.y),
			over_edge(difference.z)
		);

		let non_shifted = shift_offset.0.x == 0 && shift_offset.0.y == 0 && shift_offset.0.z == 0;

		if non_shifted
		{
			self.generate_existing_chunk(self.to_local(pos).unwrap())
		} else
		{
			self.shift_overmap_by(shift_offset);

			self.generate_existing_chunk(self.to_local(pos).unwrap())
		}.expect("chunk must not touch any edges")
	}

	fn shift_overmap_by(&mut self, shift_offset: GlobalPos)
	{
		let new_player_position = self.player_position + shift_offset;

		self.player_position = new_player_position;

		self.position_offset(shift_offset);
	}

	fn generate_existing_chunk(&self, local_pos: LocalPos<SIZE>) -> Option<Chunk>
	{
		let group = local_pos.always_group();
		if group.is_none()
		{
			println!("out of range {}, {}, {}", local_pos.0.x, local_pos.0.y, local_pos.0.z);
		}

		group.map(|group|
		{
			let group = group.map(|position| self.world_chunks[position].unwrap());

			self.world_generator.generate_chunk(group)
		})
	}
}

impl<const SIZE: usize> Overmap<SIZE, WorldChunk> for ServerOvermap<SIZE>
{
	type Container = ChunksContainer<SIZE, Option<WorldChunk>>;

	fn remove(&mut self, pos: LocalPos<SIZE>)
	{
		self.world_chunks[pos] = None;
	}

	fn swap(&mut self, a: LocalPos<SIZE>, b: LocalPos<SIZE>)
	{
		self.world_chunks.swap(a, b);
	}

	fn get_local(&self, pos: LocalPos<SIZE>) -> &Option<WorldChunk>
	{
		&self.world_chunks[pos]
	}

	fn mark_ungenerated(&mut self, _pos: LocalPos<SIZE>) {}

	fn generate_missing(&mut self)
	{
		let to_global = |local_pos|
		{
			Self::to_global_associated(local_pos, self.player_position)
		};

		self.world_generator.generate_missing(&mut self.world_chunks, to_global);
	}
}

impl<const SIZE: usize> OvermapIndexing<SIZE> for ServerOvermap<SIZE>
{
	fn player_position(&self) -> GlobalPos
	{
		self.player_position
	}
}