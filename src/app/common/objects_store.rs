use std::{
    mem,
    iter,
    ops::{Range, Index, IndexMut}
};

use serde::{Serialize, Deserialize};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectsStore<T>
{
    data: Vec<Option<T>>,
    free_list: Vec<usize>
}

impl<T> From<Vec<T>> for ObjectsStore<T>
{
    fn from(v: Vec<T>) -> Self
    {
        Self{
            data: v.into_iter().map(Some).collect(),
            free_list: Vec::new()
        }
    }
}

impl<T> ObjectsStore<T>
{
    pub fn new() -> Self
    {
        Self{data: Vec::new(), free_list: Vec::new()}
    }

    pub fn with_capacity(capacity: usize) -> Self
    {
        Self{data: Vec::with_capacity(capacity), free_list: Vec::new()}
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item=(usize, &T)> + Clone
    {
        self.data.iter().enumerate().filter_map(|(index, value)|
        {
            value.as_ref().map(|value| (index, value))
        })
    }

    pub fn iter_mut(&mut self) -> impl DoubleEndedIterator<Item=(usize, &mut T)>
    {
        self.data.iter_mut().enumerate().filter_map(|(index, value)|
        {
            value.as_mut().map(|value| (index, value))
        })
    }

    pub fn split_at(&self, mid: usize) -> (&[Option<T>], &[Option<T>])
    {
        self.data.split_at(mid)
    }

    pub fn insert(&mut self, index: usize, value: T) -> Option<T>
    {
        self.extend_to_contain(index);

        let slot = &mut self.data[index];

        if slot.is_none()
        {
            self.free_list.retain(|id| *id != index);
        }

        mem::replace(slot, Some(value))
    }

    pub fn push(&mut self, value: T) -> usize
    {
        let id = self.take_vacant_key();

        self.insert(id, value);

        id
    }

    pub fn push_last(&mut self, value: T) -> usize
    {
        let id = self.data.len();

        self.insert(id, value);

        id
    }

    pub fn push_after(&mut self, origin: usize, value: T) -> usize
    {
        let id = self.take_after_key(origin);

        self.insert(id, value);

        id
    }

    pub fn remove(&mut self, index: usize) -> Option<T>
    {
        if self.data[index].is_some()
        {
            self.free_list.push(index);
        }

        self.data[index].take()
    }

    pub fn take_vacant_key(&mut self) -> usize
    {
        if let Some(last) = self.free_list.pop()
        {
            last
        } else
        {
            self.last_key()
        }
    }

    pub fn take_after_key(&mut self, origin: usize) -> usize
    {
        if let Some(key) = self.free_list.iter().position(|x| *x > origin)
        {
            self.free_list.swap_remove(key)
        } else
        {
            self.last_key()
        }
    }

    pub fn vacant_key(&self) -> usize
    {
        if let Some(last) = self.free_list.last()
        {
            *last
        } else
        {
            self.last_key()
        }
    }

    pub fn last_key(&self) -> usize
    {
        self.data.len()
    }

    pub fn len(&self) -> usize
    {
        self.data.len() - self.free_list.len()
    }

    pub fn is_empty(&self) -> bool
    {
        self.len() == 0
    }

    #[allow(dead_code)]
    pub fn index_range(&self) -> Range<usize>
    {
        0..self.data.len()
    }

    pub fn get(&self, index: usize) -> Option<&T>
    {
        self.data.get(index).and_then(Option::as_ref)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T>
    {
        self.data.get_mut(index).and_then(Option::as_mut)
    }

    pub fn swap(&mut self, a: usize, b: usize)
    {
        self.data.swap(a, b);
    }

    fn extend_to_contain(&mut self, index: usize)
    {
        if index < self.data.len()
        {
            return;
        }

        let want_len = index + 1;

        let amount = want_len - self.data.len();

        self.data.extend(iter::repeat_with(|| None).take(amount));
    }
}

impl<T> Index<usize> for ObjectsStore<T>
{
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output
    {
        self.get(index).unwrap_or_else(|| panic!("{index} is out of range"))
    }
}

impl<T> IndexMut<usize> for ObjectsStore<T>
{
    fn index_mut(&mut self, index: usize) -> &mut Self::Output
    {
        self.get_mut(index).unwrap_or_else(|| panic!("{index} is out of range"))
    }
}

#[cfg(test)]
mod tests
{
    use std::fmt::Debug;

    use super::*;


    fn compare<T>(store: &ObjectsStore<T>, slice: &[T])
    where
        T: Debug + PartialEq
    {
        store.iter().zip(slice.iter()).for_each(|((_, a), b)|
        {
            assert_eq!(a, b);
        });
    }

    #[test]
    fn basic_stuff()
    {
        let mut s = ObjectsStore::new();

        s.push("first");

        compare(&s, &["first"]);

        s.push("test");
        s.push("three");

        s.insert(0, "changed!");

        compare(&s, &["changed!", "test", "three"]);

        assert_eq!(s.iter().map(|(id, _)| id).collect::<Vec<_>>(), vec![0, 1, 2]);

        s.remove(1);

        compare(&s, &["changed!", "three"]);

        s.remove(0);

        compare(&s, &["three"]);

        s.insert(2, "last");

        compare(&s, &["last"]);

        assert_eq!(s.iter().map(|(id, _)| id).collect::<Vec<_>>(), vec![2]);

        s.push("before!");

        compare(&s, &["before!", "last"]);
    }
}
