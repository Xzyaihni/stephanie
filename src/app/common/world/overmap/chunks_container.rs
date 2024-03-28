use std::{
    fmt,
	slice::{
        IterMut as SliceIterMut,
        Iter as SliceIter
    },
	iter::Enumerate,
	ops::{Index, IndexMut}
};

use serde::{Serialize, Deserialize};

use crate::common::world::{
	Pos3,
	LocalPos
};


macro_rules! implement_common
{
    ($name:ident, $indexer_name:ident) =>
    {
        impl<T: Default> $name<T>
        {
            pub fn new(size: Pos3<usize>) -> Self
            {
                Self::new_with(size, |_| Default::default())
            }
        }

        impl<T> $name<T>
        {
            pub fn from_raw(size: Pos3<usize>, chunks: Box<[T]>) -> Self
            {
                let indexer = $indexer_name::new(size);

                Self{chunks, indexer}
            }

            pub fn new_with<F: FnMut(LocalPos) -> T>(size: Pos3<usize>, mut default_function: F) -> Self
            {
                let indexer = Indexer::new(size);

                Self::new_indexed(size, |index| default_function(indexer.index_to_pos(index)))
            }

            pub fn new_indexed<F: FnMut(usize) -> T>(
                size: Pos3<usize>,
                mut default_function: F
            ) -> Self
            {
                let data = (0..(size.x * size.y * size.z)).map(|index|
                {
                    default_function(index)
                }).collect::<Box<[_]>>();

                Self::from_raw(size, data)
            }

            pub fn map<F, U>(&self, f: F) -> $name<U>
            where
                F: FnMut(&T) -> U
            {
                $name{
                    chunks: self.chunks.iter().map(f).collect(),
                    indexer: self.indexer.clone()
                }
            }

            #[allow(dead_code)]
            fn to_index(&self, pos: Pos3<usize>) -> usize
            {
                self.indexer.to_index(pos)
            }

            #[allow(dead_code)]
            fn index_to_pos(&self, index: usize) -> LocalPos
            {
                self.indexer.index_to_pos(index)
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
            pub fn len(&self) -> usize
            {
                self.chunks.len()
            }

            pub fn get_two_mut(&mut self, one: LocalPos, two: LocalPos) -> (&mut T, &mut T)
            {
                let one = self.indexer.to_index(one.pos);
                let two = self.indexer.to_index(two.pos);

                if one > two
                {
                    let (left, right) = self.chunks.split_at_mut(one);

                    (&mut right[0], &mut left[two])
                } else
                {
                    let (left, right) = self.chunks.split_at_mut(two);

                    (&mut left[one], &mut right[0])
                }
            }

            pub fn positions(&self) -> impl Iterator<Item=LocalPos>
            {
                let indexer = self.indexer.clone();

                (0..self.chunks.len()).map(move |index| indexer.index_to_pos(index))
            }

            pub fn iter(&self) -> Iter<$indexer_name, T>
            {
                Iter::new(self.chunks.iter(), self.indexer.clone())
            }

            pub fn iter_mut(&mut self) -> IterMut<$indexer_name, T>
            {
                IterMut::new(self.chunks.iter_mut(), self.indexer.clone())
            }
        }

        impl<T> Index<Pos3<usize>> for $name<T>
        {
            type Output = T;

            fn index(&self, value: Pos3<usize>) -> &Self::Output
            {
                &self.chunks[self.indexer.to_index(value)]
            }
        }

        impl<T> IndexMut<Pos3<usize>> for $name<T>
        {
            fn index_mut(&mut self, value: Pos3<usize>) -> &mut Self::Output
            {
                &mut self.chunks[self.indexer.to_index(value)]
            }
        }

        impl<T> Index<LocalPos> for $name<T>
        {
            type Output = T;

            fn index(&self, value: LocalPos) -> &Self::Output
            {
                &self.chunks[self.indexer.to_index(value.pos)]
            }
        }

        impl<T> IndexMut<LocalPos> for $name<T>
        {
            fn index_mut(&mut self, value: LocalPos) -> &mut Self::Output
            {
                &mut self.chunks[self.indexer.to_index(value.pos)]
            }
        }
    }
}

pub trait ChunkIndexing
{
	fn to_index(&self, pos: Pos3<usize>) -> usize;
	fn index_to_pos(&self, index: usize) -> LocalPos;
}

pub type ValuePair<T> = (LocalPos, T);

macro_rules! impl_iter
{
    ($name:ident, $other_iter:ident) =>
    {
        pub struct $name<'a, I, T>
        {
            data: Enumerate<$other_iter<'a, T>>,
            indexer: I
        }

        impl<'a, I, T> $name<'a, I, T>
        {
            pub fn new(data: $other_iter<'a, T>, indexer: I) -> Self
            {
                Self{data: data.enumerate(), indexer}
            }
        }

        impl<'a, I, T> Iterator for $name<'a, I, T>
        where
            I: ChunkIndexing
        {
            type Item = ValuePair<<$other_iter<'a, T> as Iterator>::Item>;

            fn next(&mut self) -> Option<Self::Item>
            {
                self.data.next().map(|(index, value)| (self.indexer.index_to_pos(index), value))
            }
        }
    }
}

impl_iter!{Iter, SliceIter}
impl_iter!{IterMut, SliceIterMut}

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

    pub fn size(&self) -> &Pos3<usize>
    {
        &self.size
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

implement_common!{ChunksContainer, Indexer}

impl<T> ChunksContainer<T>
{
    pub fn map_ref<U, F>(&self, f: F) -> ChunksContainer<U>
    where
        F: FnMut(&T) -> U
    {
        ChunksContainer{
            chunks: self.chunks.iter().map(f).collect(),
            indexer: self.indexer.clone()
        }
    }

    fn flat_slice_range(&self, z: usize) -> (usize, usize)
    {
        let size = self.indexer.size();
        let step = size.x * size.y;

        let start = z * step;
        let end = (z + 1) * step;

        (start, end)
    }

    pub fn flat_slice(&self, z: usize) -> &[T]
    {
        let (start, end) = self.flat_slice_range(z);

        &self.chunks[start..end]
    }

    pub fn flat_slice_mut(&mut self, z: usize) -> &mut [T]
    {
        let (start, end) = self.flat_slice_range(z);

        &mut self.chunks[start..end]
    }

    #[allow(dead_code)]
    pub fn flat_slice_iter(&self, z: usize) -> Iter<FlatIndexer, T>
    {
        let s = self.flat_slice(z).iter();

		Iter::new(s, FlatIndexer::from(self.indexer.clone()).with_z(z))
    }

    pub fn flat_slice_iter_mut(&mut self, z: usize) -> IterMut<FlatIndexer, T>
    {
        let indexer = FlatIndexer::from(self.indexer.clone()).with_z(z);
        let s = self.flat_slice_mut(z).iter_mut();

		IterMut::new(s, indexer)
    }

    pub fn map_slice_ref<U, F>(&self, z: usize, f: F) -> FlatChunksContainer<U>
    where
        F: FnMut((LocalPos, &T)) -> U
    {
        FlatChunksContainer{
            chunks: self.flat_slice_iter(z).map(f).collect(),
            indexer: self.indexer.clone().into()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlatIndexer
{
	size: Pos3<usize>,
    z: usize
}

impl FlatIndexer
{
	pub fn new(mut size: Pos3<usize>) -> Self
	{
        size.z = 1;

		Self{size, z: 0}
	}

    pub fn with_z(mut self, z: usize) -> Self
    {
        self.z = z;

        self
    }
}

impl From<Indexer> for FlatIndexer
{
    fn from(value: Indexer) -> Self
    {
        Self{size: value.size, z: 0}
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

		LocalPos::new(Pos3::new(x, y, self.z), self.size)
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlatChunksContainer<T>
{
	chunks: Box<[T]>,
	indexer: FlatIndexer
}

implement_common!{FlatChunksContainer, FlatIndexer}

impl<T> FlatChunksContainer<T>
{
    pub fn pretty_print_with<F>(&self, mut f: F) -> String
    where
        F: FnMut(&T) -> String
    {
        let longest_value = self.chunks.iter()
            .map(&mut f)
            .map(|s| s.len())
            .max()
            .unwrap_or(1);

        let row = self.indexer.size.x;

        self.chunks.iter().enumerate().map(|(index, value)|
        {
            let mut output = String::new();

            if index != 0 && (index % row) == 0
            {
                output.push('\n');
            }

            if index % row != 0
            {
                output.push(' ');
            }

            output += &format!("{:^1$}", f(value), longest_value);

            output
        }).reduce(|acc, value|
        {
            acc + &value
        }).unwrap_or_default()
    }
}

impl<T: fmt::Display> FlatChunksContainer<T>
{
    pub fn pretty_print(&self) -> String
    {
        self.pretty_print_with(T::to_string)
    }
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn flat_slice_z()
    {
        let size = Pos3{
            x: fastrand::usize(10..20),
            y: fastrand::usize(10..20),
            z: fastrand::usize(10..20)
        };

        let mut value = 0;
        let container = ChunksContainer::new_with(size, |_| { value += 1; value });

        let random_z = fastrand::usize(0..size.z);

        let manual_flat_slice = container.iter().filter(|(pos, _value)|
        {
            pos.pos.z == random_z
        });

        let flat_slice_iter = container.flat_slice_iter(random_z);

        flat_slice_iter.zip(manual_flat_slice).for_each(|(a, b)|
        {
            assert_eq!(a, b);
        });
    }
}
