use std::iter;

use chunk::{
	GlobalPos,
	LocalPos
};

pub use chunks_container::{ChunkIndexing, ChunksContainer, FlatChunksContainer};

pub mod chunk;
pub mod visual_chunk;

pub mod chunks_container;


pub trait Overmap<const SIZE: usize, T>: OvermapIndexing<SIZE>
{
	type Container: ChunkIndexing<SIZE>;

	fn remove(&mut self, pos: LocalPos<SIZE>);

	fn swap(&mut self, a: LocalPos<SIZE>, b: LocalPos<SIZE>);

	fn get_local(&self, pos: LocalPos<SIZE>) -> &Option<T>;

	fn mark_ungenerated(&mut self, pos: LocalPos<SIZE>);

	fn get(&self, pos: GlobalPos) -> Option<&T>
	{
		self.to_local(pos).map(|local_pos| self.get_local(local_pos).as_ref()).flatten()
	}

	fn default_ordering() -> Box<[usize]>
	{
		let mut ordering = (0..(SIZE * SIZE * SIZE)).collect::<Vec<_>>();
		ordering.sort_unstable_by(move |a, b|
		{
			let distance = |value: usize| -> f32
			{
				let local_pos = Self::Container::index_to_pos(value);

				let GlobalPos(pos) = GlobalPos::from(local_pos) - (SIZE as i32 / 2);

				((pos.x.pow(2) + pos.y.pow(2) + pos.z.pow(2)) as f32).sqrt()
			};

			distance(*a).total_cmp(&distance(*b))
		});

		ordering.into_boxed_slice()
	}

	fn generate_missing(&mut self);

	fn position_offset(&mut self, offset: GlobalPos)
	{
		self.shift_chunks(offset);
		self.generate_missing();
	}

	fn shift_chunks(&mut self, offset: GlobalPos)
	{
		let conditional_overmap = |reversed|
		{
			let (mut start, step) = if reversed
			{
				(SIZE - 1, -1)
			} else
			{
				(0, 1)
			};

			iter::repeat_with(move ||
			{
				let return_value = start;
				start = (start as i32 + step) as usize;

				return_value
			}).take(SIZE)
		};

		conditional_overmap(offset.0.z < 0).flat_map(|z|
		{
			conditional_overmap(offset.0.y < 0).flat_map(move |y|
			{
				conditional_overmap(offset.0.x < 0).map(move |x| LocalPos::new(x, y, z))
			})
		}).for_each(|old_local|
		{
			//early return if the chunk is empty
			if self.get_local(old_local).is_none()
			{
				return;
			}

			let old_position = self.to_global(old_local);
			let position = old_position - offset;

			if let Some(local_pos) = self.to_local(position)
			{
				//move the chunk to the new position
				self.swap(old_local, local_pos);

				let is_edge_chunk =
				{
					let is_edge = |pos, offset|
					{
						if offset == 0
						{
							false
						} else if offset < 0
						{
							(pos as i32 + offset) == 0
						} else
						{
							(pos as i32 + offset) == (SIZE as i32 - 1)
						}
					};

					let x_edge = is_edge(local_pos.0.x, offset.0.x);
					let y_edge = is_edge(local_pos.0.y, offset.0.y);
					let z_edge = is_edge(local_pos.0.z, offset.0.z);

					x_edge || y_edge || z_edge
				};

				if is_edge_chunk
				{
					self.mark_ungenerated(local_pos);
				}
			} else
			{
				//chunk now outside the player range, remove it
				self.remove(old_local);
			}
		});
	}
}

pub trait OvermapIndexing<const SIZE: usize>
{
	fn player_position(&self) -> GlobalPos;

	fn to_local(&self, pos: GlobalPos) -> Option<LocalPos<SIZE>>
	{
		Self::to_local_associated(pos, self.player_position())
	}

	fn to_local_associated(
		pos: GlobalPos,
		player_position: GlobalPos
	) -> Option<LocalPos<SIZE>>
	{
		let player_distance = pos - player_position;

		let pos = player_distance + (SIZE as i32 / 2);

		LocalPos::from_global(pos, SIZE as i32)
	}

	fn to_global(&self, pos: LocalPos<SIZE>) -> GlobalPos
	{
		Self::to_global_associated(pos, self.player_position())
	}

	fn to_global_associated(
		pos: LocalPos<SIZE>,
		player_position: GlobalPos
	) -> GlobalPos
	{
		Self::player_offset(pos) + player_position
	}

	fn player_offset(pos: LocalPos<SIZE>) -> GlobalPos
	{
		GlobalPos::from(pos) - (SIZE as i32 / 2)
	}
}