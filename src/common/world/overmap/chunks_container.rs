use std::{
	slice::{IterMut, Iter},
	iter::Enumerate,
	ops::{Index, IndexMut}
};

use serde::{Serialize, Deserialize};

use crate::common::world::{
	Pos3,
	LocalPos
};


pub trait ChunkIndexing
{
	fn to_index(&self, pos: Pos3<usize>) -> usize;
	fn index_to_pos(&self, index: usize) -> LocalPos;
}

pub type ValuePair<T> = (LocalPos, T);

pub struct ChunksIter<'a, I, T>
{
	chunks: Enumerate<Iter<'a, T>>,
	indexer: &'a I
}

impl<'a, I, T> ChunksIter<'a, I, T>
{
	pub fn new(chunks: Enumerate<Iter<'a, T>>, indexer: &'a I) -> Self
	{
		Self{chunks, indexer}
	}
}

impl<'a, I, T> Iterator for ChunksIter<'a, I, T>
where
	I: ChunkIndexing
{
	type Item = ValuePair<&'a T>;

	fn next(&mut self) -> Option<Self::Item>
	{
		self.chunks.next().map(|(index, item)| (self.indexer.index_to_pos(index), item))
	}
}

pub struct ChunksIterMut<'a, I, T>
{
	chunks: Enumerate<IterMut<'a, T>>,
	indexer: &'a I
}

impl<'a, I, T> ChunksIterMut<'a, I, T>
{
	pub fn new(chunks: Enumerate<IterMut<'a, T>>, indexer: &'a I) -> Self
	{
		Self{chunks, indexer}
	}
}

impl<'a, I, T> Iterator for ChunksIterMut<'a, I, T>
where
	I: ChunkIndexing
{
	type Item = ValuePair<&'a mut T>;

	fn next(&mut self) -> Option<Self::Item>
	{
		self.chunks.next().map(|(index, item)| (self.indexer.index_to_pos(index), item))
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Indexer
{
	size: Pos3<usize>
}

impl Indexer
{
	pub fn new(size: Pos3<usize>) -> Self
	{
		Self{size}
	}
}

impl ChunkIndexing for Indexer
{
	fn to_index(&self, pos: Pos3<usize>) -> usize
	{
		pos.to_rectangle(self.size.x, self.size.y)
	}

	fn index_to_pos(&self, index: usize) -> LocalPos
	{
		let x = index % self.size.x;
		let y = (index / self.size.x) % self.size.y;
		let z = index / (self.size.x * self.size.y);

		LocalPos::new(Pos3::new(x, y, z), self.size)
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunksContainer<T>
{
	chunks: Box<[T]>,
	indexer: Indexer
}

impl<T> ChunksContainer<T>
{
	pub fn new<F: FnMut(LocalPos) -> T>(size: Pos3<usize>, mut default_function: F) -> Self
	{
		let indexer = Indexer::new(size);

		let chunks = (0..(size.x * size.y * size.z)).map(|index|
		{
			default_function(indexer.index_to_pos(index))
		}).collect::<Vec<_>>().into_boxed_slice();

		Self{chunks, indexer}
	}

	pub fn swap(&mut self, a: LocalPos, b: LocalPos)
	{
		let (index_a, index_b) = (self.indexer.to_index(a.pos), self.indexer.to_index(b.pos));

		self.chunks.swap(index_a, index_b);
	}

    #[allow(dead_code)]
    pub fn size(&self) -> Pos3<usize>
    {
        self.indexer.size
    }

	#[allow(dead_code)]
	pub fn iter(&self) -> ChunksIter<Indexer, T>
	{
		ChunksIter::new(self.chunks.iter().enumerate(), &self.indexer)
	}

	#[allow(dead_code)]
	pub fn iter_mut(&mut self) -> ChunksIterMut<Indexer, T>
	{
		ChunksIterMut::new(self.chunks.iter_mut().enumerate(), &self.indexer)
	}
}

impl<T> Index<Pos3<usize>> for ChunksContainer<T>
{
	type Output = T;

	fn index(&self, value: Pos3<usize>) -> &Self::Output
	{
		&self.chunks[self.indexer.to_index(value)]
	}
}

impl<T> IndexMut<Pos3<usize>> for ChunksContainer<T>
{
	fn index_mut(&mut self, value: Pos3<usize>) -> &mut Self::Output
	{
		&mut self.chunks[self.indexer.to_index(value)]
	}
}

impl<T> Index<LocalPos> for ChunksContainer<T>
{
	type Output = T;

	fn index(&self, value: LocalPos) -> &Self::Output
	{
		&self.chunks[self.indexer.to_index(value.pos)]
	}
}

impl<T> IndexMut<LocalPos> for ChunksContainer<T>
{
	fn index_mut(&mut self, value: LocalPos) -> &mut Self::Output
	{
		&mut self.chunks[self.indexer.to_index(value.pos)]
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlatIndexer
{
	size: Pos3<usize>
}

impl FlatIndexer
{
	pub fn new(size: Pos3<usize>) -> Self
	{
		Self{size}
	}
}

impl ChunkIndexing for FlatIndexer
{
	fn to_index(&self, pos: Pos3<usize>) -> usize
	{
		pos.y * self.size.x + pos.x
	}

	fn index_to_pos(&self, index: usize) -> LocalPos
	{
		let x = index % self.size.x;
		let y = (index / self.size.x) % self.size.y;

		LocalPos::new(Pos3::new(x, y, 0), self.size)
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlatChunksContainer<T>
{
	chunks: Box<[T]>,
	indexer: FlatIndexer
}

impl<T> FlatChunksContainer<T>
{
	pub fn new<F: FnMut(LocalPos) -> T>(size: Pos3<usize>, mut default_function: F) -> Self
	{
		let indexer = FlatIndexer::new(size);

		let chunks = (0..(size.x * size.y)).map(|index|
		{
			default_function(indexer.index_to_pos(index))
		}).collect::<Vec<_>>().into_boxed_slice();

		Self{chunks, indexer}
	}

	pub fn swap(&mut self, a: LocalPos, b: LocalPos)
	{
		let (index_a, index_b) = (self.indexer.to_index(a.pos), self.indexer.to_index(b.pos));

		self.chunks.swap(index_a, index_b);
	}

    #[allow(dead_code)]
    pub fn size(&self) -> Pos3<usize>
    {
        self.indexer.size
    }

	pub fn iter(&self) -> ChunksIter<FlatIndexer, T>
	{
		ChunksIter::new(self.chunks.iter().enumerate(), &self.indexer)
	}

	pub fn iter_mut(&mut self) -> ChunksIterMut<FlatIndexer, T>
	{
		ChunksIterMut::new(self.chunks.iter_mut().enumerate(), &self.indexer)
	}
}

impl<T> Index<LocalPos> for FlatChunksContainer<T>
{
	type Output = T;

	fn index(&self, value: LocalPos) -> &Self::Output
	{
		&self.chunks[self.indexer.to_index(value.pos)]
	}
}

impl<T> IndexMut<LocalPos> for FlatChunksContainer<T>
{
	fn index_mut(&mut self, value: LocalPos) -> &mut Self::Output
	{
		&mut self.chunks[self.indexer.to_index(value.pos)]
	}
}
