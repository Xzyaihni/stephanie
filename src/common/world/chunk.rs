use std::{
	ops::{Sub, Add}
};

use serde::{Serialize, Deserialize};

use nalgebra::Vector3;

use tile::Tile;

use crate::common::Transform;

pub mod tile;


#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pos3<T>
{
	pub x: T,
	pub y: T,
	pub z: T
}

impl<T> Pos3<T>
{
	pub fn new(x: T, y: T, z: T) -> Self
	{
		Self{x, y, z}
	}
}

impl<T: Copy> From<Vector3<T>> for Pos3<T>
{
	fn from(value: Vector3<T>) -> Self
	{
		Self{x: value[0], y: value[1], z: value[2]}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalPos(pub Pos3<i32>);

impl GlobalPos
{
	pub fn new(x: i32, y: i32, z: i32) -> Self
	{
		Self(Pos3::new(x, y, z))
	}

	#[allow(dead_code)]
	pub fn to_world(self, local: LocalPos, side: f32, tile_size: f32) -> Pos3<f32>
	{
		let Self(chunk) = self;
		let LocalPos(local) = local;

		Pos3::new(
			chunk.x as f32 * side * tile_size + local.x as f32 * tile_size,
			chunk.y as f32 * side * tile_size + local.y as f32 * tile_size,
			chunk.z as f32 * side * tile_size + local.z as f32 * tile_size
		)
	}
}

impl Sub for GlobalPos
{
	type Output = Self;

	fn sub(self, other: Self) -> Self::Output
	{
		let Self(pos) = self;
		let Self(other) = other;

		Self::new(pos.x - other.x, pos.y - other.y, pos.z - other.z)
	}
}

impl Add for GlobalPos
{
	type Output = Self;

	fn add(self, other: Self) -> Self::Output
	{
		let Self(pos) = self;
		let Self(other) = other;

		Self::new(pos.x + other.x, pos.y + other.y, pos.z + other.z)
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalPos(pub Pos3<usize>);

impl LocalPos
{
	pub fn new(x: usize, y: usize, z: usize) -> Self
	{
		Self(Pos3::new(x, y, z))
	}

	pub fn from_global(pos: GlobalPos, side: i32) -> Option<Self>
	{
		let in_range = |value| (0..side).contains(&value);

		let GlobalPos(pos) = pos;

		if in_range(pos.x) && in_range(pos.y) && in_range(pos.z)
		{
			Some(Self::new(pos.x as usize, pos.y as usize, pos.z as usize))
		} else
		{
			None
		}
	}

	pub fn to_cube(self, side: usize) -> usize
	{
		let Self(pos) = self;

		pos.x + pos.y * side + pos.z * side * side
	}
}

pub const CHUNK_SIZE: usize = 16;
const CHUNK_VOLUME: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;

pub const CHUNK_VISUAL_SIZE: f32 = CHUNK_SIZE as f32  * TILE_SIZE;

pub const TILE_SIZE: f32 = 0.1;

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

	pub fn transform_of_chunk(x: i32, y: i32) -> Transform
	{
		let mut transform = Transform::new();
		transform.position = Vector3::new(
			x as f32 * CHUNK_SIZE as f32 * TILE_SIZE,
			y as f32 * CHUNK_SIZE as f32 * TILE_SIZE,
			0.0
		);

		transform
	}

	#[allow(dead_code)]
	pub fn set_tile(&mut self, pos: LocalPos, tile: Tile)
	{
		self.tiles[Self::index_of(pos)] = tile;
	}

	pub fn get_tile(&self, pos: LocalPos) -> Tile
	{
		self.tiles[Self::index_of(pos)]
	}

	pub fn vertical_iter(
		&self,
		x: usize,
		y: usize
	) -> impl DoubleEndedIterator<Item=Tile> + ExactSizeIterator<Item=Tile> + '_
	{
		(0..CHUNK_SIZE).map(move |z|
		{
			let pos = LocalPos::new(x, y, z);
			self.get_tile(pos)
		})
	}

	fn index_of(pos: LocalPos) -> usize
	{
		let LocalPos(pos) = pos;

		pos.z * CHUNK_SIZE * CHUNK_SIZE + pos.y * CHUNK_SIZE + pos.x
	}
}