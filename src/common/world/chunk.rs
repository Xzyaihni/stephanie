use std::{
	ops::{Index, IndexMut, Sub, Add}
};

use serde::{Serialize, Deserialize};

use num_enum::TryFromPrimitive;

use strum_macros::EnumIter;

use enum_amount::EnumCount;

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
	pub fn to_world<const T: usize>(
		self,
		local: LocalPos<T>,
		side: f32,
		tile_size: f32
	) -> Pos3<f32>
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
pub struct LocalPos<const EDGE: usize>(pub Pos3<usize>);

#[repr(u8)]
#[derive(Debug, Clone, Copy, TryFromPrimitive, EnumCount, EnumIter)]
pub enum PosDirection
{
	Right,
	Left,
	Up,
	Down
}

pub struct InclusiveGroup<T>
{
	pub this: T,
	pub right: Option<T>,
	pub left: Option<T>,
	pub up: Option<T>,
	pub down: Option<T>
}

impl<T> Index<PosDirection> for InclusiveGroup<T>
{
	type Output = Option<T>;

	fn index(&self, index: PosDirection) -> &Self::Output
	{
		match index
		{
			PosDirection::Right => &self.right,
			PosDirection::Left => &self.left,
			PosDirection::Up => &self.up,
			PosDirection::Down => &self.down
		}
	}
}

impl<const EDGE: usize> LocalPos<EDGE>
{
	pub fn new(x: usize, y: usize, z: usize) -> Self
	{
		Self(Pos3::new(x, y, z))
	}

	#[allow(dead_code)]
	pub fn directions(&self) -> impl Iterator<Item=Option<Self>>
	{
		[self.right(), self.left(), self.up(), self.down()].into_iter()
	}

	pub fn directions_inclusive(self) -> impl Iterator<Item=Option<Self>>
	{
		[Some(self), self.right(), self.left(), self.up(), self.down()].into_iter()
	}

	pub fn directions_inclusive_group<T, F>(self, mut map_function: F) -> InclusiveGroup<T>
	where
		F: FnMut(Self) -> T
	{
		InclusiveGroup{
			this: map_function(self),
			right: self.right().map(&mut map_function),
			left: self.left().map(&mut map_function),
			up: self.up().map(&mut map_function),
			down: self.down().map(&mut map_function)
		}
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

	pub fn overflow(&self, direction: PosDirection) -> Self
	{
		let Self(pos) = self;

		match direction
		{
			PosDirection::Right => Self::new(0, pos.y, pos.z),
			PosDirection::Left => Self::new(EDGE - 1, pos.y, pos.z),
			PosDirection::Up => Self::new(pos.x, 0, pos.z),
			PosDirection::Down => Self::new(pos.x, EDGE - 1, pos.z)
		}
	}

	pub fn offset(&self, direction: PosDirection) -> Option<Self>
	{
		match direction
		{
			PosDirection::Right => self.right(),
			PosDirection::Left => self.left(),
			PosDirection::Up => self.up(),
			PosDirection::Down => self.down()
		}
	}

	pub fn right(&self) -> Option<Self>
	{
		let Self(pos) = self;

		(!self.right_edge()).then(|| Self::new(pos.x + 1, pos.y, pos.z))
	}

	pub fn left(&self) -> Option<Self>
	{
		let Self(pos) = self;

		(!self.left_edge()).then(|| Self::new(pos.x - 1, pos.y, pos.z))
	}

	pub fn up(&self) -> Option<Self>
	{
		let Self(pos) = self;

		(!self.top_edge()).then(|| Self::new(pos.x, pos.y + 1, pos.z))
	}

	pub fn down(&self) -> Option<Self>
	{
		let Self(pos) = self;

		(!self.bottom_edge()).then(|| Self::new(pos.x, pos.y - 1, pos.z))
	}

	pub fn top_edge(&self) -> bool
	{
		self.0.y == (EDGE - 1)
	}

	pub fn bottom_edge(&self) -> bool
	{
		self.0.y == 0
	}

	pub fn right_edge(&self) -> bool
	{
		self.0.x == (EDGE - 1)
	}

	pub fn left_edge(&self) -> bool
	{
		self.0.x == 0
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

pub type ChunkLocal = LocalPos<CHUNK_SIZE>;

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

	fn index_of(pos: ChunkLocal) -> usize
	{
		let LocalPos(pos) = pos;

		pos.z * CHUNK_SIZE * CHUNK_SIZE + pos.y * CHUNK_SIZE + pos.x
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