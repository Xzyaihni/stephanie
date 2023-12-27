use std::ops::{Index, IndexMut};

use serde::{Serialize, Deserialize};

use nalgebra::Vector3;

use tile::Tile;

use crate::common::Transform;
pub use pos::*;

pub mod tile;
pub mod pos;


pub const CHUNK_SIZE: usize = 16;
const CHUNK_VOLUME: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;

pub const CHUNK_VISUAL_SIZE: f32 = CHUNK_SIZE as f32  * TILE_SIZE;

pub const TILE_SIZE: f32 = 0.1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ChunkLocal(pub LocalPos);

impl PartialEq for ChunkLocal
{
	fn eq(&self, other: &Self) -> bool
	{
		self.0.pos == other.0.pos
	}
}

impl ChunkLocal
{
	pub fn new(x: usize, y: usize, z: usize) -> Self
	{
		let size = Pos3::new(CHUNK_SIZE, CHUNK_SIZE, CHUNK_SIZE);
		let local_pos = LocalPos::new(Pos3::new(x, y, z), size);

		Self(local_pos)
	}

	pub fn maybe_group(self) -> MaybeGroup<Self>
	{
		self.0.maybe_group().map(Self)
	}

	pub fn overflow(&self, direction: PosDirection) -> Self
	{
		let local_pos = self.0.overflow(direction);

		Self(local_pos)
	}

	#[allow(dead_code)]
	pub fn offset(&self, direction: PosDirection) -> Option<Self>
	{
		let local_pos = self.0.offset(direction);

		local_pos.map(Self)
	}

	pub fn pos(&self) -> Pos3<usize>
	{
		self.0.pos
	}

	#[allow(dead_code)]
	pub fn size(&self) -> Pos3<usize>
	{
		self.0.size
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk
{
	tiles: Box<[Tile]>
}

impl Chunk
{
	pub fn new() -> Self
	{
		let tiles = vec![Tile::none(); CHUNK_VOLUME].into_boxed_slice();

		Self{tiles}
	}

	pub fn transform_of_chunk(pos: GlobalPos) -> Transform
	{
		let chunk_pos = Pos3::<f32>::from(pos.0) * CHUNK_VISUAL_SIZE;

		Transform{
		    position: Vector3::from(chunk_pos),
            ..Default::default()
        }
	}

	fn index_of(pos: Pos3<usize>) -> usize
	{
		pos.z * CHUNK_SIZE * CHUNK_SIZE + pos.y * CHUNK_SIZE + pos.x
	}
}

impl From<Box<[Tile]>> for Chunk
{
	fn from(value: Box<[Tile]>) -> Self
	{
		Self{tiles: value}
	}
}

impl Index<Pos3<usize>> for Chunk
{
	type Output = Tile;

	fn index(&self, index: Pos3<usize>) -> &Self::Output
	{
		&self.tiles[Self::index_of(index)]
	}
}

impl IndexMut<Pos3<usize>> for Chunk
{
	fn index_mut(&mut self, index: Pos3<usize>) -> &mut Self::Output
	{
		&mut self.tiles[Self::index_of(index)]
	}
}

impl Index<ChunkLocal> for Chunk
{
	type Output = Tile;

	fn index(&self, index: ChunkLocal) -> &Self::Output
	{
		&self.tiles[Self::index_of(index.pos())]
	}
}

impl IndexMut<ChunkLocal> for Chunk
{
	fn index_mut(&mut self, index: ChunkLocal) -> &mut Self::Output
	{
		&mut self.tiles[Self::index_of(index.pos())]
	}
}
