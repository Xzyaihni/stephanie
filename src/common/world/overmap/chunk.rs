use std::{
	fmt::{self, Display},
	ops::{Index, IndexMut, Sub, Add, Mul, Div}
};

use serde::{Serialize, Deserialize};

use num_enum::TryFromPrimitive;

use strum_macros::EnumIter;

use enum_amount::EnumCount;

use nalgebra::Vector3;

use tile::Tile;

use crate::common::Transform;

pub mod tile;


#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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

impl<T: Mul<Output=T> + Add<Output=T> + Copy> Pos3<T>
{
    pub fn to_rectangle(self, x: T, y: T) -> T
    {
		self.x + self.y * x + self.z * x * y
    }
}

impl Pos3<f32>
{
	pub fn tile_height(self) -> usize
	{
		(self.modulo(1.0).z / VISUAL_TILE_HEIGHT) as usize
	}

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

impl<T: Display> Display for Pos3<T>
{
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
	{
		write!(f, "[{}, {}, {}]", self.x, self.y, self.z)
	}
}

impl<T: Copy> From<Vector3<T>> for Pos3<T>
{
	fn from(value: Vector3<T>) -> Self
	{
		Self{x: value[0], y: value[1], z: value[2]}
	}
}

impl<T: Mul<Output=T> + Copy> Mul for Pos3<T>
{
	type Output = Self;

	fn mul(self, rhs: Self) -> Self::Output
	{
		Self::new(self.x * rhs.x, self.y * rhs.y, self.z * rhs.z)
	}
}

impl<T: Mul<Output=T> + Copy> Mul<T> for Pos3<T>
{
	type Output = Self;

	fn mul(self, rhs: T) -> Self::Output
	{
		self.map(|value| value * rhs)
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
		self.map(|value| value - rhs)
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

impl<T: Add<Output=T> + Copy> Add<T> for Pos3<T>
{
	type Output = Self;

	fn add(self, rhs: T) -> Self::Output
	{
		self.map(|value| value + rhs)
	}
}

impl<T: Div<Output=T> + Copy> Div<T> for Pos3<T>
{
	type Output = Self;

	fn div(self, rhs: T) -> Self::Output
	{
		self.map(|value| value / rhs)
	}
}

impl<T: Div<Output=T> + Copy> Div for Pos3<T>
{
	type Output = Self;

	fn div(self, rhs: Self) -> Self::Output
	{
		Self::new(self.x / rhs.x, self.y / rhs.y, self.z / rhs.z)
	}
}

impl From<GlobalPos> for Pos3<f32>
{
	fn from(value: GlobalPos) -> Self
	{
		let GlobalPos(pos) = value;

		pos.map(|value| value as f32)
	}
}

impl From<Pos3<usize>> for Pos3<i32>
{
	fn from(value: Pos3<usize>) -> Self
	{
		value.map(|value| value as i32)
	}
}

impl From<LocalPos> for Pos3<f32>
{
	fn from(value: LocalPos) -> Self
	{
		let pos = value.pos;

		Self{x: pos.x as f32, y: pos.y as f32, z: pos.z as f32}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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

		Self(pos - rhs)
	}
}

impl Sub<i32> for GlobalPos
{
	type Output = Self;

	fn sub(self, rhs: i32) -> Self::Output
	{
		let Self(pos) = self;

		Self(pos - rhs)
	}
}

impl Add for GlobalPos
{
	type Output = Self;

	fn add(self, rhs: Self) -> Self::Output
	{
		let Self(pos) = self;
		let Self(rhs) = rhs;

		Self(pos + rhs)
	}
}

impl Add<i32> for GlobalPos
{
	type Output = Self;

	fn add(self, rhs: i32) -> Self::Output
	{
		let Self(pos) = self;

		Self(pos + rhs)
	}
}

impl Div<i32> for GlobalPos
{
	type Output = Self;

	fn div(self, rhs: i32) -> Self::Output
	{
		let Self(pos) = self;

		Self(pos / rhs)
	}
}

impl From<LocalPos> for GlobalPos
{
	fn from(value: LocalPos) -> Self
	{
		let LocalPos{pos, ..} = value;

		Self::new(
			pos.x as i32,
			pos.y as i32,
			pos.z as i32
		)
	}
}

impl From<Pos3<i32>> for GlobalPos
{
	fn from(value: Pos3<i32>) -> Self
	{
		Self(value)
	}
}

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
		F: FnMut(PosDirection, T) -> D
	{
		DirectionsGroup{
			right: direction_map(PosDirection::Right, self.right),
			left: direction_map(PosDirection::Left, self.left),
			up: direction_map(PosDirection::Up, self.up),
			down: direction_map(PosDirection::Down, self.down)
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
			other: self.other.map(|_direction, value|
			{
				value.map(&mut direction_map)
			})
		}
	}

	pub fn remap<D, TF, DF>(self, this_map: TF, mut direction_map: DF) -> MaybeGroup<D>
	where
		TF: FnOnce(T) -> D,
		DF: FnMut(PosDirection, Option<T>) -> Option<D>
	{
		MaybeGroup{
			this: this_map(self.this),
			other: self.other.map(&mut direction_map)
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
			other: self.other.map(|_direction, value| direction_map(value))
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LocalPos
{
	pub pos: Pos3<usize>,
	pub size: Pos3<usize>
}

impl LocalPos
{
	pub fn new(pos: Pos3<usize>, size: Pos3<usize>) -> Self
	{
		Self{pos, size}
	}

	pub fn from_global(other: GlobalPos, size: Pos3<usize>) -> Option<Self>
	{
		let in_range = |value, limit| (0..limit as i32).contains(&value);

		let GlobalPos(pos) = other;

		let in_range = in_range(pos.x, size.x)
		&& in_range(pos.y, size.y)
		&& in_range(pos.z, size.z);

		in_range.then(||
		{
			Self::new(Pos3::new(pos.x as usize, pos.y as usize, pos.z as usize), size)
		})
	}

	pub fn moved(&self, x: usize, y: usize, z: usize) -> Self
	{
		Self{pos: Pos3::new(x, y, z), size: self.size}
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
	pub fn directions_group(self) -> DirectionsGroup<Option<Self>>
	{
		DirectionsGroup{
			right: self.right(),
			left: self.left(),
			up: self.up(),
			down: self.down()
		}
	}

	pub fn maybe_group(self) -> MaybeGroup<Self>
	{
		MaybeGroup{
			this: self,
			other: self.directions_group()
		}
	}

	pub fn always_group(self) -> Option<AlwaysGroup<Self>>
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

		let other = directions.map(|_direction, value| value.unwrap());

		Some(AlwaysGroup{
			this: self,
			other
		})
	}

	pub fn overflow(&self, direction: PosDirection) -> Self
	{
		let pos = self.pos;

		match direction
		{
			PosDirection::Right => self.moved(0, pos.y, pos.z),
			PosDirection::Left => self.moved(self.size.x - 1, pos.y, pos.z),
			PosDirection::Up => self.moved(pos.x, 0, pos.z),
			PosDirection::Down => self.moved(pos.x, self.size.y - 1, pos.z)
		}
	}

	#[allow(dead_code)]
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
		let pos = self.pos;

		(0..(self.size.x - 1)).contains(&pos.x).then(|| self.moved(pos.x + 1, pos.y, pos.z))
	}

	pub fn left(&self) -> Option<Self>
	{
		let pos = self.pos;

		(1..self.size.x).contains(&pos.x).then(|| self.moved(pos.x - 1, pos.y, pos.z))
	}

	pub fn up(&self) -> Option<Self>
	{
		let pos = self.pos;

		(0..(self.size.y - 1)).contains(&pos.y).then(|| self.moved(pos.x, pos.y + 1, pos.z))
	}

	pub fn down(&self) -> Option<Self>
	{
		let pos = self.pos;

		(1..self.size.y).contains(&pos.y).then(|| self.moved(pos.x, pos.y - 1, pos.z))
	}

    #[allow(dead_code)]
	pub fn top_edge(&self) -> bool
	{
		self.pos.y == (self.size.y - 1)
	}

    #[allow(dead_code)]
	pub fn bottom_edge(&self) -> bool
	{
		self.pos.y == 0
	}

    #[allow(dead_code)]
	pub fn right_edge(&self) -> bool
	{
		self.pos.x == (self.size.x - 1)
	}

    #[allow(dead_code)]
	pub fn left_edge(&self) -> bool
	{
		self.pos.x == 0
	}

	#[allow(dead_code)]
	pub fn to_cube(self, side: usize) -> usize
	{
		self.to_rectangle(side, side)
	}

	pub fn to_rectangle(self, x: usize, y: usize) -> usize
	{
		self.pos.to_rectangle(x, y)
	}
}

pub const CHUNK_SIZE: usize = 16;
const CHUNK_VOLUME: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;

pub const CHUNK_VISUAL_SIZE: f32 = CHUNK_SIZE as f32  * TILE_SIZE;

pub const TILE_SIZE: f32 = 0.1;

pub const VISUAL_TILE_HEIGHT: f32 = 1.0 / CHUNK_SIZE as f32;

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
		self.0.maybe_group().map(|local_pos| Self(local_pos))
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

		local_pos.map(|local_pos| Self(local_pos))
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
		let GlobalPos(pos) = pos;

		let chunk_pos = Pos3::from(pos).map(|v| v as f32) * CHUNK_VISUAL_SIZE;

		let mut transform = Transform::default();
		transform.position = Vector3::new(
			chunk_pos.x,
			chunk_pos.y,
			chunk_pos.z
		);

		transform
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
