use std::{
	slice::{IterMut, Iter},
	iter::{self, Enumerate},
	marker::PhantomData,
	ops::{Index, IndexMut}
};

use chunk::{
	Pos3,
	GlobalPos,
	LocalPos
};

pub mod chunk;
pub mod visual_chunk;


pub trait Overmap<const SIZE: usize, T>: OvermapIndexing<SIZE>
{
	type Container: ChunkIndexing<SIZE>;

	fn chunk_ordering(&self) -> &[usize];

	fn request_chunk(&self, pos: GlobalPos);
	fn player_moved(&mut self, player_position: Pos3<f32>);
	fn remove(&mut self, pos: LocalPos<SIZE>);

	fn swap(
		&mut self,
		a: LocalPos<SIZE>,
		b: LocalPos<SIZE>
	);

	fn get(&self, pos: LocalPos<SIZE>) -> &Option<T>;
	fn mark_ungenerated(&self, pos: LocalPos<SIZE>);

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

	fn generate_missing(&mut self)
	{
		let player_pos = self.player_position();

		self.chunk_ordering()
			.iter()
			.map(|index| Self::Container::index_to_pos(*index))
			.filter(|pos| self.get(*pos).is_none())
			.for_each(|pos|
			{
				let global_pos = Self::to_global_associated(pos, player_pos);

				self.request_chunk(global_pos);
			});
	}

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
			if self.get(old_local).is_none()
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

pub trait ChunkIndexing<const SIZE: usize>
{
	fn to_index(pos: LocalPos<SIZE>) -> usize;
	fn index_to_pos(index: usize) -> LocalPos<SIZE>;
}

pub type ValuePair<const SIZE: usize, T> = (LocalPos<SIZE>, T);

pub struct ChunksIter<'a, const SIZE: usize, Indexer, T>
{
	chunks: Enumerate<Iter<'a, T>>,
	indexer: PhantomData<*const Indexer>
}

impl<'a, const SIZE: usize, Indexer, T> ChunksIter<'a, SIZE, Indexer, T>
{
	pub fn new(chunks: Enumerate<Iter<'a, T>>) -> Self
	{
		Self{chunks, indexer: PhantomData}
	}
}

impl<'a, const SIZE: usize, Indexer, T> Iterator for ChunksIter<'a, SIZE, Indexer, T>
where
	Indexer: ChunkIndexing<SIZE>
{
	type Item = ValuePair<SIZE, &'a T>;

	fn next(&mut self) -> Option<Self::Item>
	{
		self.chunks.next().map(|(index, item)| (Indexer::index_to_pos(index), item))
	}
}

pub struct ChunksIterMut<'a, const SIZE: usize, Indexer, T>
{
	chunks: Enumerate<IterMut<'a, T>>,
	indexer: PhantomData<*const Indexer>
}

impl<'a, const SIZE: usize, Indexer, T> ChunksIterMut<'a, SIZE, Indexer, T>
{
	pub fn new(chunks: Enumerate<IterMut<'a, T>>) -> Self
	{
		Self{chunks, indexer: PhantomData}
	}
}

impl<'a, const SIZE: usize, Indexer, T> Iterator for ChunksIterMut<'a, SIZE, Indexer, T>
where
	Indexer: ChunkIndexing<SIZE>
{
	type Item = ValuePair<SIZE, &'a mut T>;

	fn next(&mut self) -> Option<Self::Item>
	{
		self.chunks.next().map(|(index, item)| (Indexer::index_to_pos(index), item))
	}
}

#[derive(Debug)]
pub struct ChunksContainer<const SIZE: usize, T>
{
	chunks: Box<[T]>
}

impl<const SIZE: usize, T> ChunksContainer<SIZE, T>
{
	pub fn new<F: FnMut() -> T>(mut default_function: F) -> Self
	{
		let chunks = (0..(SIZE * SIZE * SIZE)).map(|_| default_function())
			.collect::<Vec<_>>().into_boxed_slice();

		Self{chunks}
	}

	pub fn swap(&mut self, a: LocalPos<SIZE>, b: LocalPos<SIZE>)
	{
		let (index_a, index_b) = (Self::to_index(a), Self::to_index(b));

		self.chunks.swap(index_a, index_b);
	}

	#[allow(dead_code)]
	pub fn iter(&self) -> ChunksIter<SIZE, Self, T>
	{
		ChunksIter::new(self.chunks.iter().enumerate())
	}

	#[allow(dead_code)]
	pub fn iter_mut(&mut self) -> ChunksIterMut<SIZE, Self, T>
	{
		ChunksIterMut::new(self.chunks.iter_mut().enumerate())
	}
}

impl<const SIZE: usize, T> ChunkIndexing<SIZE> for ChunksContainer<SIZE, T>
{
	fn to_index(pos: LocalPos<SIZE>) -> usize
	{
		pos.to_cube(SIZE)
	}

	fn index_to_pos(index: usize) -> LocalPos<SIZE>
	{
		let x = index % SIZE;
		let y = (index / SIZE) % SIZE;
		let z = index / (SIZE * SIZE);

		LocalPos::new(x, y, z)
	}
}

impl<const SIZE: usize, T> Index<LocalPos<SIZE>> for ChunksContainer<SIZE, T>
{
	type Output = T;

	fn index(&self, value: LocalPos<SIZE>) -> &Self::Output
	{
		&self.chunks[Self::to_index(value)]
	}
}

impl<const SIZE: usize, T> IndexMut<LocalPos<SIZE>> for ChunksContainer<SIZE, T>
{
	fn index_mut(&mut self, value: LocalPos<SIZE>) -> &mut Self::Output
	{
		&mut self.chunks[Self::to_index(value)]
	}
}

#[derive(Debug)]
pub struct FlatChunksContainer<const SIZE: usize, T>
{
	chunks: Box<[T]>
}

impl<const SIZE: usize, T> FlatChunksContainer<SIZE, T>
{
	pub fn new<F: FnMut() -> T>(mut default_function: F) -> Self
	{
		let chunks = (0..(SIZE * SIZE)).map(|_| default_function())
			.collect::<Vec<_>>().into_boxed_slice();

		Self{chunks}
	}

	pub fn swap(&mut self, a: LocalPos<SIZE>, b: LocalPos<SIZE>)
	{
		let (index_a, index_b) = (Self::to_index(a), Self::to_index(b));

		self.chunks.swap(index_a, index_b);
	}

	pub fn iter(&self) -> ChunksIter<SIZE, Self, T>
	{
		ChunksIter::new(self.chunks.iter().enumerate())
	}

	pub fn iter_mut(&mut self) -> ChunksIterMut<SIZE, Self, T>
	{
		ChunksIterMut::new(self.chunks.iter_mut().enumerate())
	}
}

impl<const SIZE: usize, T> ChunkIndexing<SIZE> for FlatChunksContainer<SIZE, T>
{
	fn to_index(pos: LocalPos<SIZE>) -> usize
	{
		let LocalPos(pos) = pos;

		pos.y * SIZE + pos.x
	}

	fn index_to_pos(index: usize) -> LocalPos<SIZE>
	{
		let x = index % SIZE;
		let y = (index / SIZE) % SIZE;

		LocalPos::new(x, y, 0)
	}
}

impl<const SIZE: usize, T> Index<LocalPos<SIZE>> for FlatChunksContainer<SIZE, T>
{
	type Output = T;

	fn index(&self, value: LocalPos<SIZE>) -> &Self::Output
	{
		&self.chunks[Self::to_index(value)]
	}
}

impl<const SIZE: usize, T> IndexMut<LocalPos<SIZE>> for FlatChunksContainer<SIZE, T>
{
	fn index_mut(&mut self, value: LocalPos<SIZE>) -> &mut Self::Output
	{
		&mut self.chunks[Self::to_index(value)]
	}
}