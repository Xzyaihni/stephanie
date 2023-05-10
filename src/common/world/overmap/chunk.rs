use std::{
	ops::{Index, IndexMut, Sub, Add, Mul}
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

	pub fn map<F: FnMut(T) -> V, V>(self, mut f: F) -> Pos3<V>
	{
		Pos3::<V>{x: f(self.x), y: f(self.y), z: f(self.z)}
	}
}

impl Pos3<f32>
{
	pub fn rounded(self) -> GlobalPos
	{
		GlobalPos(self.map(|value|
		{
			let size = CHUNK_SIZE as f32 * TILE_SIZE;
			let value = value / size;

			if value < 0.0
			{
				value as i32 - 1
			} else
			{
				value as i32
			}
		}))
	}

	pub fn modulo(self, divisor: f32) -> Pos3<f32>
	{
		self.map(|value|
		{
			if value < 0.0
			{
				divisor + (value % divisor)
			} else
			{
				value % divisor
			}
		})
	}
}

impl<T: Copy> From<Vector3<T>> for Pos3<T>
{
	fn from(value: Vector3<T>) -> Self
	{
		Self{x: value[0], y: value[1], z: value[2]}
	}
}

impl<T: Mul<Output=T> + Copy> Mul<T> for Pos3<T>
{
	type Output = Self;

	fn mul(self, rhs: T) -> Self::Output
	{
		Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
	}
}

impl<T: Sub<Output=T>> Sub for Pos3<T>
{
	type Output = Self;

	fn sub(self, rhs: Self) -> Self::Output
	{
		Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
	}
}

impl<T: Sub<Output=T> + Copy> Sub<T> for Pos3<T>
{
	type Output = Self;

	fn sub(self, rhs: T) -> Self::Output
	{
		Self::new(self.x - rhs, self.y - rhs, self.z - rhs)
	}
}

impl<T: Add<Output=T>> Add for Pos3<T>
{
	type Output = Self;

	fn add(self, rhs: Self) -> Self::Output
	{
		Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
	}
}

impl From<GlobalPos> for Pos3<f32>
{
	fn from(value: GlobalPos) -> Self
	{
		let GlobalPos(pos) = value;

		Self{x: pos.x as f32, y: pos.y as f32, z: pos.z as f32}
	}
}

impl<const T: usize> From<LocalPos<T>> for Pos3<f32>
{
	fn from(value: LocalPos<T>) -> Self
	{
		let LocalPos(pos) = value;

		Self{x: pos.x as f32, y: pos.y as f32, z: pos.z as f32}
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
}

impl Sub for GlobalPos
{
	type Output = Self;

	fn sub(self, rhs: Self) -> Self::Output
	{
		let Self(pos) = self;
		let Self(rhs) = rhs;

		Self::new(pos.x - rhs.x, pos.y - rhs.y, pos.z - rhs.z)
	}
}

impl Sub<i32> for GlobalPos
{
	type Output = Self;

	fn sub(self, rhs: i32) -> Self::Output
	{
		let Self(pos) = self;

		Self::new(pos.x - rhs, pos.y - rhs, pos.z - rhs)
	}
}

impl Add for GlobalPos
{
	type Output = Self;

	fn add(self, rhs: Self) -> Self::Output
	{
		let Self(pos) = self;
		let Self(rhs) = rhs;

		Self::new(pos.x + rhs.x, pos.y + rhs.y, pos.z + rhs.z)
	}
}

impl Add<i32> for GlobalPos
{
	type Output = Self;

	fn add(self, rhs: i32) -> Self::Output
	{
		let Self(pos) = self;

		Self::new(pos.x + rhs, pos.y + rhs, pos.z + rhs)
	}
}

impl<const T: usize> From<LocalPos<T>> for GlobalPos
{
	fn from(value: LocalPos<T>) -> Self
	{
		let LocalPos(pos) = value;

		Self::new(
			pos.x as i32,
			pos.y as i32,
			pos.z as i32
		)
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

#[derive(Debug, Serialize, Deserialize)]
pub struct DirectionsGroup<T>
{
	pub right: T,
	pub left: T,
	pub up: T,
	pub down: T
}

impl<T> DirectionsGroup<T>
{
	pub fn map<D, F>(self, mut direction_map: F) -> DirectionsGroup<D>
	where
		F: FnMut(T) -> D
	{
		DirectionsGroup{
			right: direction_map(self.right),
			left: direction_map(self.left),
			up: direction_map(self.up),
			down: direction_map(self.down)
		}
	}
}

impl<T> Index<PosDirection> for DirectionsGroup<T>
{
	type Output = T;

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

#[derive(Debug, Serialize, Deserialize)]
pub struct MaybeGroup<T>
{
	pub this: T,
	pub other: DirectionsGroup<Option<T>>
}

impl<T> MaybeGroup<T>
{
	pub fn map<D, F>(self, mut direction_map: F) -> MaybeGroup<D>
	where
		F: FnMut(T) -> D
	{
		MaybeGroup{
			this: direction_map(self.this),
			other: self.other.map(|direction| direction.map(&mut direction_map))
		}
	}
}

impl<T> Index<PosDirection> for MaybeGroup<T>
{
	type Output = Option<T>;

	fn index(&self, index: PosDirection) -> &Self::Output
	{
		&self.other[index]
	}
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AlwaysGroup<T>
{
	pub this: T,
	pub other: DirectionsGroup<T>
}

impl<T> AlwaysGroup<T>
{
	pub fn map<D, F>(self, mut direction_map: F) -> AlwaysGroup<D>
	where
		F: FnMut(T) -> D
	{
		AlwaysGroup{
			this: direction_map(self.this),
			other: self.other.map(direction_map)
		}
	}
}

impl<T> Index<PosDirection> for AlwaysGroup<T>
{
	type Output = T;

	fn index(&self, index: PosDirection) -> &Self::Output
	{
		&self.other[index]
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

	#[allow(dead_code)]
	pub fn directions_group(self) -> DirectionsGroup<Option<LocalPos<EDGE>>>
	{
		DirectionsGroup{
			right: self.right(),
			left: self.left(),
			up: self.up(),
			down: self.down()
		}
	}

	pub fn maybe_group(self) -> MaybeGroup<LocalPos<EDGE>>
	{
		MaybeGroup{
			this: self,
			other: self.directions_group()
		}
	}

	pub fn always_group(self) -> Option<AlwaysGroup<LocalPos<EDGE>>>
	{
		let directions = self.directions_group();

		let any_none =
			directions.right.is_none()
			|| directions.left.is_none()
			|| directions.up.is_none()
			|| directions.down.is_none();

		if any_none
		{
			return None;
		}

		let other = directions.map(|direction| direction.unwrap());

		Some(AlwaysGroup{
			this: self,
			other
		})
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

pub const VISUAL_TILE_HEIGHT: f32 = 1.0 / CHUNK_SIZE as f32;

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

	pub fn transform_of_chunk(pos: GlobalPos) -> Transform
	{
		let GlobalPos(pos) = pos;

		let chunk_pos = Pos3::from(pos).map(|v| v as f32) * CHUNK_VISUAL_SIZE;

		let mut transform = Transform::new();
		transform.position = Vector3::new(
			chunk_pos.x,
			chunk_pos.y,
			chunk_pos.z
		);

		transform
	}

	fn index_of(pos: ChunkLocal) -> usize
	{
		let LocalPos(pos) = pos;

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