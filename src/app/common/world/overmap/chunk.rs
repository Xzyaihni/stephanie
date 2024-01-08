use std::ops::{Index, IndexMut};

use serde::{Serialize, Deserialize};

use nalgebra::Vector3;

use tile::Tile;

use crate::{impl_directionals, common::Transform};
pub use pos::*;

pub mod tile;
pub mod pos;


pub const CHUNK_SIZE: usize = 16;
const CHUNK_VOLUME: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;

pub const CHUNK_VISUAL_SIZE: f32 = CHUNK_SIZE as f32  * TILE_SIZE;

pub const TILE_SIZE: f32 = 0.1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ChunkLocal(Pos3<usize>);

impl PartialEq for ChunkLocal
{
	fn eq(&self, other: &Self) -> bool
	{
		self.0 == other.0
	}
}

impl From<Pos3<usize>> for ChunkLocal
{
    fn from(value: Pos3<usize>) -> Self
    {
        let this = Self(value);

        debug_assert!(this.in_bounds());

        this
    }
}

impl_directionals!{ChunkLocal}

impl ChunkLocal
{
	pub fn new(x: usize, y: usize, z: usize) -> Self
	{
	    Self::from(Pos3::new(x, y, z))
	}

    fn moved(&self, x: usize, y: usize, z: usize) -> Self
    {
        Self::new(x, y, z)
    }

    fn size(&self) -> Pos3<usize>
    {
        Pos3::repeat(CHUNK_SIZE)
    }

    fn pos_mut(&mut self) -> &mut Pos3<usize>
    {
        &mut self.0
    }

    pub fn pos(&self) -> &Pos3<usize>
    {
        &self.0
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

	fn index_of(pos: ChunkLocal) -> usize
	{
        let pos = pos.0;

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

impl Index<ChunkLocal> for Chunk
{
	type Output = Tile;

	fn index(&self, index: ChunkLocal) -> &Self::Output
	{
		&self.tiles[Self::index_of(index)]
	}
}

impl IndexMut<ChunkLocal> for Chunk
{
	fn index_mut(&mut self, index: ChunkLocal) -> &mut Self::Output
	{
		&mut self.tiles[Self::index_of(index)]
	}
}
