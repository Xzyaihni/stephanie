use std::{
    iter,
    ops::{Index, IndexMut}
};


#[derive(Debug, Clone)]
pub struct ObjectsStore<T>
{
    data: Vec<Option<T>>,
    free_list: Vec<usize>
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

    pub fn iter(&self) -> impl Iterator<Item=(usize, &T)>
    {
        self.data.iter().enumerate().filter_map(|(index, value)|
        {
            value.as_ref().map(|value| (index, value))
        })
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item=(usize, &mut T)>
    {
        self.data.iter_mut().enumerate().filter_map(|(index, value)|
        {
            value.as_mut().map(|value| (index, value))
        })
    }

    pub fn insert(&mut self, index: usize, value: T)
    {
        self.extend_to_contain(index);

        self.data[index] = Some(value);
    }

    pub fn push(&mut self, value: T) -> usize
    {
        let id = self.new_id();

        self.data.push(Some(value));

        id
    }

    pub fn remove(&mut self, index: usize) -> Option<T>
    {
        self.free_list.push(index);

        self.data[index].take()
    }

    fn new_id(&mut self) -> usize
    {
        if let Some(last) = self.free_list.pop()
        {
            last
        } else
        {
            self.data.len()
        }
    }

    pub fn vacant_key(&self) -> usize
    {
        if let Some(last) = self.free_list.last()
        {
            *last
        } else
        {
            self.data.len()
        }
    }

    pub fn len(&self) -> usize
    {
        self.data.len()
    }

    pub fn get(&self, index: usize) -> Option<&T>
    {
        self.data.get(index).map(Option::as_ref).flatten()
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T>
    {
        self.data.get_mut(index).map(Option::as_mut).flatten()
    }

    pub fn contains(&self, index: usize) -> bool
    {
        self.get(index).is_some()
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