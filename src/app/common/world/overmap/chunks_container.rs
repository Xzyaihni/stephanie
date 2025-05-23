use std::{
    fmt::{self, Display, Debug},
    vec::IntoIter as VecIntoIter,
    slice::{
        IterMut as SliceIterMut,
        Iter as SliceIter
    },
    iter::Enumerate,
    ops::{Index, IndexMut}
};

use serde::{Serialize, Deserialize};

use crate::common::{
    get_two_mut,
    world::{
        Pos3,
        LocalPos
    }
};


#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Axis
{
    X,
    Y,
    Z
}

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

        impl<T> IntoIterator for $name<T>
        {
            type Item = (LocalPos, T);
            type IntoIter = IntoIter<$indexer_name, T>;

            fn into_iter(self) -> Self::IntoIter
            {
                IntoIter::new(IntoIterator::into_iter(self.chunks), self.indexer.clone())
            }
        }

        impl<T> $name<T>
        {
            pub fn from_raw(size: Pos3<usize>, chunks: Box<[T]>) -> Self
            {
                let indexer = $indexer_name::new(size);

                Self::from_raw_indexer(chunks, indexer)
            }

            fn from_raw_indexer(chunks: Box<[T]>, indexer: $indexer_name) -> Self
            {
                debug_assert!(
                    indexer.size.product() == chunks.len(),
                    "indexer: {indexer:?}, len: {}",
                    chunks.len()
                );

                Self{chunks, indexer}
            }

            pub fn new_with<F: FnMut(LocalPos) -> T>(size: Pos3<usize>, mut default_function: F) -> Self
            {
                let indexer = $indexer_name::new(size);

                Self::new_indexed(indexer.clone(), |index| default_function(indexer.index_to_pos(index)))
            }

            pub fn new_indexed<F: FnMut(usize) -> T>(
                indexer: $indexer_name,
                mut default_function: F
            ) -> Self
            {
                let data = (0..indexer.size.product()).map(|index|
                {
                    default_function(index)
                }).collect::<Box<[_]>>();

                Self::from_raw_indexer(data, indexer)
            }

            pub fn map<F, U>(&self, f: F) -> $name<U>
            where
                F: FnMut(&T) -> U
            {
                $name::from_raw_indexer(
                    self.chunks.iter().map(f).collect(),
                    self.indexer.clone()
                )
            }

            pub fn clear(&mut self)
            where
                T: Default
            {
                self.chunks.iter_mut().for_each(|x| *x = Default::default());
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

                get_two_mut(&mut self.chunks, one, two)
            }

            pub fn positions(&self) -> impl Iterator<Item=LocalPos>
            {
                let indexer = self.indexer.clone();

                indexer.positions()
            }

            pub fn positions_2d(&self) -> impl Iterator<Item=LocalPos>
            {
                let size = self.indexer.size().clone();
                (0..size.y).flat_map(move |y|
                {
                    (0..size.x).map(move |x| LocalPos::new(Pos3::new(x, y, 0), size))
                })
            }

            pub fn iter(&self) -> Iter<$indexer_name, T>
            {
                Iter::new(self.chunks.iter(), self.indexer.clone())
            }

            pub fn iter_mut(&mut self) -> IterMut<$indexer_name, T>
            {
                IterMut::new(self.chunks.iter_mut(), self.indexer.clone())
            }

            fn verify_pos(&self, pos: LocalPos)
            {
                debug_assert!(pos.size == self.indexer.size, "{} != {}", pos.size, self.indexer.size);
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
                self.verify_pos(value);
                &self.chunks[self.indexer.to_index(value.pos)]
            }
        }

        impl<T> IndexMut<LocalPos> for $name<T>
        {
            fn index_mut(&mut self, value: LocalPos) -> &mut Self::Output
            {
                self.verify_pos(value);
                &mut self.chunks[self.indexer.to_index(value.pos)]
            }
        }
    }
}

pub trait CommonIndexing
{
    fn size(&self) -> Pos3<usize>;

    fn to_index(&self, pos: Pos3<usize>) -> usize
    {
        let size = self.size();

        pos.to_rectangle(size.x, size.y)
    }

    fn index_to_pos(&self, index: usize) -> LocalPos
    {
        let size = self.size();

        LocalPos::new(Pos3::from_rectangle(size, index), size)
    }

    fn positions(self) -> impl Iterator<Item=LocalPos>
    where
        Self: Sized
    {
        (0..self.size().product()).map(move |index| self.index_to_pos(index))
    }
}

pub type ValuePair<T> = (LocalPos, T);

macro_rules! impl_iter
{
    ($name:ident, $other_iter:ident $(, $l:lifetime)?) =>
    {
        pub struct $name<$($l, )?I, T>
        {
            data: Enumerate<$other_iter<$($l, )?T>>,
            indexer: I
        }

        impl<$($l, )?I, T> $name<$($l, )?I, T>
        {
            pub fn new(data: $other_iter<$($l, )?T>, indexer: I) -> Self
            {
                Self{data: data.enumerate(), indexer}
            }
        }

        impl<$($l, )?I, T> Iterator for $name<$($l, )?I, T>
        where
            I: CommonIndexing
        {
            type Item = ValuePair<<$other_iter<$($l, )?T> as Iterator>::Item>;

            fn next(&mut self) -> Option<Self::Item>
            {
                self.data.next().map(|(index, value)| (self.indexer.index_to_pos(index), value))
            }
        }

        impl<$($l, )?I, T> DoubleEndedIterator for $name<$($l, )?I, T>
        where
            I: CommonIndexing
        {
            fn next_back(&mut self) -> Option<Self::Item>
            {
                self.data.next_back().map(|(index, value)| (self.indexer.index_to_pos(index), value))
            }
        }
    }
}

impl_iter!{Iter, SliceIter, 'a}
impl_iter!{IterMut, SliceIterMut, 'a}
impl_iter!{IntoIter, VecIntoIter}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

impl CommonIndexing for Indexer
{
    fn size(&self) -> Pos3<usize>
    {
        self.size
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
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
        ChunksContainer::from_raw_indexer(
            self.chunks.iter().map(f).collect(),
            self.indexer.clone()
        )
    }

    pub fn iter_axis(&self, axis: Axis, fixed: usize) -> impl Iterator<Item=&T>
    {
        let size = self.indexer.size();

        let (size_one, size_two) = match axis
        {
            Axis::X => (size.y, size.z),
            Axis::Y => (size.x, size.z),
            Axis::Z => (size.x, size.y)
        };

        (0..size_one).flat_map(move |a|
        {
            (0..size_two).map(move |b|
            {
                match axis
                {
                    Axis::X => Pos3::new(fixed, a, b),
                    Axis::Y => Pos3::new(a, fixed, b),
                    Axis::Z => Pos3::new(a, b, fixed)
                }
            })
        }).map(|pos| &self[pos])
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
        FlatChunksContainer::from_raw_indexer(
            self.flat_slice_iter(z).map(f).collect(),
            self.indexer.clone().into()
        )
    }

    pub fn display(self) -> DisplayChunksContainer<T>
    where
        T: Display
    {
        DisplayChunksContainer(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

    pub fn size(&self) -> &Pos3<usize>
    {
        &self.size
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
        Self::new(value.size)
    }
}

impl CommonIndexing for FlatIndexer
{
    fn size(&self) -> Pos3<usize>
    {
        self.size
    }

    fn to_index(&self, pos: Pos3<usize>) -> usize
    {
        debug_assert!(pos.z == self.z, "{} != {}", pos.z, self.z);

        pos.y * self.size.x + pos.x
    }

    fn index_to_pos(&self, index: usize) -> LocalPos
    {
        let x = index % self.size.x;
        let y = (index / self.size.x) % self.size.y;

        LocalPos::new(Pos3::new(x, y, self.z), self.size)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

    #[allow(dead_code)]
    pub fn with_z(mut self, z: usize) -> Self
    {
        self.indexer = self.indexer.with_z(z);

        self
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
            assert_eq!(a.0.pos, b.0.pos);
            assert_eq!(a.1, b.1);
        });
    }
}

pub fn debug_3d_slices(
    f: &mut fmt::Formatter,
    size: Pos3<usize>,
    getter: impl Fn(Pos3<usize>) -> String
) -> fmt::Result
{
    let max_len = size.positions().map(|pos| getter(pos).chars().count()).max().unwrap();

    let mut s = f.debug_struct("Chunk");

    for z in 0..size.z
    {
        struct Slice
        {
            size_y: usize,
            values: Vec<String>
        }

        impl Debug for Slice
        {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
            {
                let mut s = f.debug_struct("z slice");

                self.values.iter().enumerate().for_each(|(y, xs)|
                {
                    let name = format!("y {:1$}", y, (self.size_y - 1).to_string().len());

                    s.field(&name, xs);
                });

                s.finish()
            }
        }

        let mut sl = Slice{size_y: size.y, values: Vec::new()};
        for y in 0..size.y
        {
            let mut data = "[".to_owned();
            for x in 0..size.x
            {
                let pos = Pos3::new(x, y, z);

                if x != 0
                {
                    data += " ";
                }

                data += &format!("{:1$}", getter(pos), max_len);
            }
            data += "]";

            sl.values.push(data);
        }

        s.field(&format!("z {z}"), &sl);
    }

    s.finish()
}

pub struct DisplayChunksContainer<T>(ChunksContainer<T>);

impl<T: Display> Debug for DisplayChunksContainer<T>
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        debug_3d_slices(f, self.0.size(), |pos| self.0[pos].to_string())
    }
}

impl<T: Debug> Debug for ChunksContainer<T>
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        debug_3d_slices(f, self.size(), |pos| format!("{:?}", self[pos]))
    }
}
