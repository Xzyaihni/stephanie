use std::collections::HashMap;

pub use crate::define_info_id;


#[macro_export]
macro_rules! define_info_id
{
    ($name:ident) =>
    {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        pub struct $name(usize);

        impl From<usize> for $name
        {
            fn from(value: usize) -> Self
            {
                Self(value)
            }
        }

        impl From<$name> for usize
        {
            fn from(value: $name) -> Self
            {
                value.0
            }
        }
    }
}

pub trait GenericItem
{
    fn name(&self) -> String;
}

pub struct GenericInfo<Id, Item>
{
    mapping: HashMap<String, Id>,
    items: Vec<Item>
}

impl<Id, Item> GenericInfo<Id, Item>
where
    Id: From<usize> + Copy,
    usize: From<Id>,
    Item: GenericItem
{
    pub fn new(items: Vec<Item>) -> Self
    {
        let mapping = items.iter().enumerate().map(|(index, item)|
        {
            (item.name(), Id::from(index))
        }).collect();

        Self{mapping, items}
    }

    pub fn id(&self, name: &str) -> Id
    {
        self.get_id(name).unwrap_or_else(||
        {
            panic!("item named {name} doesnt exist")
        })
    }

    pub fn get_id(&self, name: &str) -> Option<Id>
    {
        self.mapping.get(name).copied()
    }

    pub fn get(&self, id: Id) -> &Item
    {
        &self.items[usize::from(id)]
    }

    pub fn items(&self) -> &[Item]
    {
        &self.items
    }

    pub fn random(&self) -> Id
    {
        Id::from(fastrand::usize(0..self.items.len()))
    }
}
