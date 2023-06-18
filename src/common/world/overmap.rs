use std::iter;

use chunk::{
	Pos3,
	GlobalPos,
	LocalPos
};

pub use chunks_container::{ChunkIndexing, ChunksContainer, FlatChunksContainer};

pub mod chunk;
pub mod visual_chunk;

pub mod chunks_container;


pub trait Overmap<T>: OvermapIndexing
{
	fn remove(&mut self, pos: LocalPos);

	fn swap(&mut self, a: LocalPos, b: LocalPos);

	fn get_local(&self, pos: LocalPos) -> &Option<T>;

	fn mark_ungenerated(&mut self, pos: LocalPos);

	fn get(&self, pos: GlobalPos) -> Option<&T>
	{
		self.to_local(pos).map(|local_pos| self.get_local(local_pos).as_ref()).flatten()
	}

	fn generate_missing(&mut self);

	fn position_offset(&mut self, offset: GlobalPos)
	{
		self.shift_chunks(offset);
		self.generate_missing();
	}

	fn shift_chunks(&mut self, offset: GlobalPos)
	{
		let conditional_overmap = |reversed, limit|
		{
			let (mut start, step) = if reversed
			{
				(limit - 1, -1)
			} else
			{
				(0, 1)
			};

			iter::repeat_with(move ||
			{
				let return_value = start;
				start = (start as i32 + step) as usize;

				return_value
			}).take(limit)
		};

		let size = self.size();

		// im rewriting this stuff later!!!!!!!!
		conditional_overmap(offset.0.z < 0, size.z).flat_map(|z|
		{
			conditional_overmap(offset.0.y < 0, size.y).flat_map(move |y|
			{
				conditional_overmap(offset.0.x < 0, size.x)
					.map(move |x| LocalPos::new(Pos3::new(x, y, z), size))
			})
		}).for_each(|old_local|
		{

		});
	}

	fn shift_chunk(&mut self, offset: GlobalPos, local_pos: LocalPos)
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
				let is_edge = |pos, offset, limit|
				{
					if offset == 0
					{
						false
					} else if offset < 0
					{
						(pos as i32 + offset) == 0
					} else
					{
						(pos as i32 + offset) == (limit as i32 - 1)
					}
				};

				let size = self.size();
				let x_edge = is_edge(local_pos.pos.x, offset.0.x, size.x);
				let y_edge = is_edge(local_pos.pos.y, offset.0.y, size.y);
				let z_edge = is_edge(local_pos.pos.z, offset.0.z, size.z);

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
	}
}

pub trait OvermapIndexing
{
	fn size(&self) -> Pos3<usize>;
	fn player_position(&self) -> GlobalPos;

	fn default_ordering(
		&self,
		positions: impl Iterator<Item=LocalPos>
	) -> Box<[LocalPos]>
	{
		let mut ordering = positions.collect::<Vec<_>>();

		ordering.sort_unstable_by(move |a, b|
		{
			let distance = |local_pos| -> f32
			{
				let GlobalPos(pos) = self.player_offset(local_pos);

				((pos.x.pow(2) + pos.y.pow(2) + pos.z.pow(2)) as f32).sqrt()
			};

			distance(*a).total_cmp(&distance(*b))
		});

		ordering.into_boxed_slice()
	}

	fn to_local(&self, pos: GlobalPos) -> Option<LocalPos>
	{
		let pos = self.to_local_unconverted(pos);

		LocalPos::from_global(pos, self.size())
	}

	fn to_local_unconverted(&self, pos: GlobalPos) -> GlobalPos
	{
		let player_distance = pos - self.player_position();

		player_distance + GlobalPos::from(Pos3::from(self.size())) / 2
	}

	fn to_global(&self, pos: LocalPos) -> GlobalPos
	{
		self.player_offset(pos) + self.player_position()
	}

	fn player_offset(&self, pos: LocalPos) -> GlobalPos
	{
		GlobalPos::from(pos) - GlobalPos::from(Pos3::from(self.size()))
	}
}