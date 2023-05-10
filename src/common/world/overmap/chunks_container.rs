use std::{
	marker::PhantomData,
	slice::{IterMut, Iter},
	iter::Enumerate,
	ops::{Index, IndexMut}
};

use crate::common::world::{
	LocalPos
};


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
	pub fn new<F: FnMut(LocalPos<SIZE>) -> T>(mut default_function: F) -> Self
	{
		let chunks = (0..(SIZE * SIZE * SIZE)).map(|index|
		{
			default_function(Self::index_to_pos(index))
		}).collect::<Vec<_>>().into_boxed_slice();

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
	pub fn new<F: FnMut(LocalPos<SIZE>) -> T>(mut default_function: F) -> Self
	{
		let chunks = (0..(SIZE * SIZE)).map(|index|
		{
			default_function(Self::index_to_pos(index))
		}).collect::<Vec<_>>().into_boxed_slice();

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