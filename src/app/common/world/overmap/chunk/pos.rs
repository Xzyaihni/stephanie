use std::{
	fmt::{self, Display},
	ops::{Index, Sub, Add, Mul, Div, Neg}
};

use serde::{Serialize, Deserialize};

use num_enum::TryFromPrimitive;

use enum_amount::EnumCount;

use nalgebra::{Vector3, Point3, Scalar};

use super::{CHUNK_VISUAL_SIZE, TILE_SIZE};


#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct Pos3<T>
{
	pub x: T,
	pub y: T,
	pub z: T
}

impl<T, V> Pos3<(T, V)>
{
    pub fn unzip(self) -> (Pos3<T>, Pos3<V>)
    {
        (Pos3{
            x: self.x.0,
            y: self.y.0,
            z: self.z.0
        }, Pos3{
            x: self.x.1,
            y: self.y.1,
            z: self.z.1
        })
    }
}

impl<T> Pos3<T>
{
	pub fn new(x: T, y: T, z: T) -> Self
	{
		Self{x, y, z}
	}

    pub fn new_with(mut f: impl FnMut() -> T) -> Self
    {
        Self{x: f(), y: f(), z: f()}
    }

    pub fn plane_of(&self, direction: PosDirection) -> &T
    {
        match direction
        {
            PosDirection::Right | PosDirection::Left => &self.x,
            PosDirection::Up | PosDirection::Down => &self.y,
            PosDirection::Forward | PosDirection::Back => &self.z
        }
    }

    pub fn plane_of_mut(&mut self, direction: PosDirection) -> &mut T
    {
        match direction
        {
            PosDirection::Right | PosDirection::Left => &mut self.x,
            PosDirection::Up | PosDirection::Down => &mut self.y,
            PosDirection::Forward | PosDirection::Back => &mut self.z
        }
    }

	pub fn map<F: FnMut(T) -> V, V>(self, mut f: F) -> Pos3<V>
	{
		Pos3::<V>{x: f(self.x), y: f(self.y), z: f(self.z)}
	}

    pub fn all<F: FnMut(T) -> bool>(self, mut f: F) -> bool
    {
        f(self.x) && f(self.y) && f(self.z)
    }

    pub fn any<F: FnMut(T) -> bool>(self, mut f: F) -> bool
    {
        f(self.x) || f(self.y) || f(self.z)
    }

    pub fn zip<V>(self, other: Pos3<V>) -> Pos3<(T, V)>
    {
        Pos3{
            x: (self.x, other.x),
            y: (self.y, other.y),
            z: (self.z, other.z)
        }
    }

    pub fn product(self) -> T
    where
        T: Mul<T, Output=T>
    {
        self.x * self.y * self.z
    }

    #[allow(dead_code)]
    pub fn cast<V: From<T>>(self) -> Pos3<V>
    {
        self.map(|value| V::from(value))
    }
}

impl<T: Copy> Pos3<T>
{
    pub fn repeat(v: T) -> Self
    {
        Self{x: v, y: v, z: v}
    }
}

impl<T: Mul<Output=T> + Add<Output=T> + Copy> Pos3<T>
{
    pub fn to_rectangle(self, x: T, y: T) -> T
    {
		self.x + self.y * x + self.z * x * y
    }
}

impl From<Pos3<f32>> for Vector3<f32>
{
    fn from(value: Pos3<f32>) -> Self
    {
        Self::new(value.x, value.y, value.z)
    }
}

impl From<GlobalPos> for Pos3<f32>
{
    fn from(value: GlobalPos) -> Self
    {
        value.0.map(|value|
        {
            value as f32 * CHUNK_VISUAL_SIZE
        })
    }
}

impl Pos3<f32>
{
	pub fn tile_height(self) -> usize
	{
        self.to_tile().z
	}

    pub fn to_tile(self) -> Pos3<usize>
    {
        (self.modulo(CHUNK_VISUAL_SIZE) / TILE_SIZE).map(|x| x as usize)
    }

	pub fn rounded(self) -> GlobalPos
	{
		GlobalPos(self.map(|value|
		{
			let value = value / CHUNK_VISUAL_SIZE;

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

impl<T: Copy + Scalar + fmt::Debug> From<Point3<T>> for Pos3<T>
{
	fn from(value: Point3<T>) -> Self
	{
		Self{x: value.x, y: value.y, z: value.z}
	}
}

macro_rules! pos3_op_impl
{
    ($op_trait:ident, $op_func:ident) =>
    {
        impl<T: $op_trait<Output=T>> $op_trait for Pos3<T>
        {
            type Output = Self;

            fn $op_func(self, rhs: Self) -> Self::Output
            {
                Self::new(
                    self.x.$op_func(rhs.x),
                    self.y.$op_func(rhs.y),
                    self.z.$op_func(rhs.z)
                )
            }
        }

        impl<T: $op_trait<Output=T> + Copy> $op_trait<T> for Pos3<T>
        {
            type Output = Self;

            fn $op_func(self, rhs: T) -> Self::Output
            {
                Self::new(
                    self.x.$op_func(rhs),
                    self.y.$op_func(rhs),
                    self.z.$op_func(rhs)
                )
            }
        }
    }
}

pos3_op_impl!{Add, add}
pos3_op_impl!{Sub, sub}
pos3_op_impl!{Mul, mul}
pos3_op_impl!{Div, div}

impl<T: Neg<Output=T>> Neg for Pos3<T>
{
    type Output = Self;

    fn neg(self) -> Self
    {
        self.map(|v| -v)
    }
}

impl From<Pos3<i32>> for Pos3<f32>
{
	fn from(value: Pos3<i32>) -> Self
	{
		value.map(|value| value as f32)
	}
}

impl From<Pos3<usize>> for Pos3<f32>
{
	fn from(value: Pos3<usize>) -> Self
	{
		value.map(|value| value as f32)
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

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct GlobalPos(pub Pos3<i32>);

impl GlobalPos
{
	pub fn new(x: i32, y: i32, z: i32) -> Self
	{
		Self(Pos3::new(x, y, z))
	}
}

macro_rules! globalpos_op_impl
{
    ($op_trait:ident, $op_func:ident) =>
    {
        impl $op_trait<Pos3<i32>> for GlobalPos
        {
            type Output = Self;

            fn $op_func(self, rhs: Pos3<i32>) -> Self::Output
            {
                Self(self.0.$op_func(rhs))
            }
        }

        impl $op_trait for GlobalPos
        {
            type Output = Self;

            fn $op_func(self, rhs: Self) -> Self::Output
            {
                self.$op_func(rhs.0)
            }
        }

        impl $op_trait<i32> for GlobalPos
        {
            type Output = Self;

            fn $op_func(self, rhs: i32) -> Self::Output
            {
                Self(self.0.$op_func(rhs))
            }
        }
    }
}

globalpos_op_impl!{Add, add}
globalpos_op_impl!{Sub, sub}
globalpos_op_impl!{Mul, mul}
globalpos_op_impl!{Div, div}

impl Neg for GlobalPos
{
    type Output = Self;

    fn neg(self) -> Self
    {
        Self(-self.0)
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

impl From<Pos3<usize>> for GlobalPos
{
	fn from(value: Pos3<usize>) -> Self
	{
		Self(value.into())
	}
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, TryFromPrimitive, EnumCount)]
pub enum PosDirection
{
	Right,
	Left,
	Up,
	Down,
    Forward,
    Back
}

impl PosDirection
{
    pub fn iter_non_z() -> impl Iterator<Item=Self>
    {
        [
            PosDirection::Right,
            PosDirection::Left,
            PosDirection::Up,
            PosDirection::Down
        ].into_iter()
    }

    pub fn opposite(self) -> Self
    {
        match self
        {
            PosDirection::Right => PosDirection::Left,
            PosDirection::Left => PosDirection::Right,
            PosDirection::Up => PosDirection::Down,
            PosDirection::Down => PosDirection::Up,
            PosDirection::Forward => PosDirection::Back,
            PosDirection::Back => PosDirection::Forward
        }
    }

    pub fn is_negative(&self) -> bool
    {
        match self
        {
            PosDirection::Right | PosDirection::Up | PosDirection::Forward => false,
            PosDirection::Left | PosDirection::Down | PosDirection::Back => true
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
			PosDirection::Down => &self.down,
            PosDirection::Forward | PosDirection::Back => unreachable!()
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[macro_export]
macro_rules! impl_directionals
{
    ($name:ident) =>
    {
        impl $name
        {
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

            pub fn directions_always_group(self) -> Option<DirectionsGroup<Self>>
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

                // u cant reach this part if any of the directions r none
                Some(directions.map(|_direction, value|
                {
                    unsafe{ value.unwrap_unchecked() }
                }))
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
                self.directions_always_group().map(|other|
                {
                    AlwaysGroup{
                        this: self,
                        other
                    }
                })
            }

            pub fn overflow(&self, direction: PosDirection) -> Self
            {
                let pos = self.pos();

                match direction
                {
                    PosDirection::Right => self.moved(0, pos.y, pos.z),
                    PosDirection::Left => self.moved(self.size().x - 1, pos.y, pos.z),
                    PosDirection::Up => self.moved(pos.x, 0, pos.z),
                    PosDirection::Down => self.moved(pos.x, self.size().y - 1, pos.z),
                    PosDirection::Forward => self.moved(pos.x, pos.y, 0),
                    PosDirection::Back => self.moved(pos.x, pos.y, self.size().z - 1)
                }
            }

            #[allow(dead_code)]
            pub fn offset(&self, direction: PosDirection) -> Option<Self>
            {
                let edge = if direction.is_negative()
                {
                    0
                } else
                {
                    self.size().plane_of(direction) - 1
                };

                let is_edge = *self.pos().plane_of(direction) == edge;

                (!is_edge).then(||
                {
                    let mut value = *self;

                    *value.pos_mut().plane_of_mut(direction) += 1;
                    debug_assert!(value.in_bounds());

                    value
                })
            }

            pub fn right(&self) -> Option<Self>
            {
                (!self.right_edge()).then(||
                {
                    let mut value = *self;

                    value.pos_mut().x += 1;
                    debug_assert!(value.in_bounds());

                    value
                })
            }

            pub fn left(&self) -> Option<Self>
            {
                (!self.left_edge()).then(||
                {
                    let mut value = *self;

                    value.pos_mut().x -= 1;
                    debug_assert!(value.in_bounds());

                    value
                })
            }

            pub fn forward(&self) -> Option<Self>
            {
                (!self.forward_edge()).then(||
                {
                    let mut value = *self;

                    value.pos_mut().z += 1;
                    debug_assert!(value.in_bounds());

                    value
                })
            }

            pub fn back(&self) -> Option<Self>
            {
                (!self.back_edge()).then(||
                {
                    let mut value = *self;

                    value.pos_mut().z -= 1;
                    debug_assert!(value.in_bounds());

                    value
                })
            }

            pub fn up(&self) -> Option<Self>
            {
                (!self.top_edge()).then(||
                {
                    let mut value = *self;

                    value.pos_mut().y += 1;
                    debug_assert!(value.in_bounds());

                    value
                })
            }

            pub fn down(&self) -> Option<Self>
            {
                (!self.bottom_edge()).then(||
                {
                    let mut value = *self;

                    value.pos_mut().y -= 1;
                    debug_assert!(value.in_bounds());

                    value
                })
            }

            #[allow(dead_code)]
            pub fn top_edge(&self) -> bool
            {
                self.pos().y == (self.size().y - 1)
            }

            #[allow(dead_code)]
            pub fn bottom_edge(&self) -> bool
            {
                self.pos().y == 0
            }

            #[allow(dead_code)]
            pub fn forward_edge(&self) -> bool
            {
                self.pos().z == (self.size().z - 1)
            }

            #[allow(dead_code)]
            pub fn back_edge(&self) -> bool
            {
                self.pos().z == 0
            }

            #[allow(dead_code)]
            pub fn right_edge(&self) -> bool
            {
                self.pos().x == (self.size().x - 1)
            }

            #[allow(dead_code)]
            pub fn left_edge(&self) -> bool
            {
                self.pos().x == 0
            }

            pub fn in_bounds(&self) -> bool
            {
                self.pos().zip(self.size()).all(|(pos, size)| pos < size)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub struct LocalPos
{
	pub pos: Pos3<usize>,
	pub size: Pos3<usize>
}

impl_directionals!{LocalPos}

impl LocalPos
{
	pub fn new(pos: Pos3<usize>, size: Pos3<usize>) -> Self
	{
		Self{pos, size}
	}

	pub fn from_global(other: GlobalPos, size: Pos3<usize>) -> Option<Self>
	{
		let GlobalPos(pos) = other;

        let this = Self::new(Pos3::new(pos.x as usize, pos.y as usize, pos.z as usize), size);

        this.in_bounds().then_some(this)
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

    fn size(&self) -> Pos3<usize>
    {
        self.size
    }

    fn pos_mut(&mut self) -> &mut Pos3<usize>
    {
        &mut self.pos
    }

    fn pos(&self) -> &Pos3<usize>
    {
        &self.pos
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

macro_rules! localpos_op_impl
{
    ($op_trait:ident, $op_func:ident) =>
    {
        impl $op_trait<Pos3<usize>> for LocalPos
        {
            type Output = Self;

            fn $op_func(self, rhs: Pos3<usize>) -> Self::Output
            {
                let value = Self{
                    pos: self.pos.$op_func(rhs),
                    size: self.size
                };

                debug_assert!(
                    value.in_bounds(),
                    "{:?} out of bounds",
                    value
                );

                value
            }
        }

        impl $op_trait for LocalPos
        {
            type Output = Self;

            fn $op_func(self, rhs: Self) -> Self::Output
            {
                debug_assert!(
                    self.size == rhs.size,
                    "{:?} != {:?}",
                    self.size, rhs.size
                );

                self.$op_func(rhs.pos)
            }
        }

        impl $op_trait<usize> for LocalPos
        {
            type Output = Self;

            fn $op_func(self, rhs: usize) -> Self::Output
            {
                Self{
                    pos: self.pos.$op_func(rhs),
                    size: self.size
                }
            }
        }
    }
}

localpos_op_impl!{Add, add}
localpos_op_impl!{Sub, sub}
localpos_op_impl!{Mul, mul}
localpos_op_impl!{Div, div}
