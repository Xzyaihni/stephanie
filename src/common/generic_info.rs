use std::{
    path::Path,
    collections::HashMap
};

use serde::Deserialize;

use yanyaengine::{Assets, TextureId};

pub use crate::{
    define_info_id,
    common::normalize_path
};


#[macro_export]
macro_rules! define_info_id
{
    ($name:ident) =>
    {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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

pub fn load_texture_path(root: impl AsRef<Path>, name: &str) -> String
{
    let formatted_name = name.replace(' ', "_") + ".png";
    let path = root.as_ref().join(formatted_name);

    normalize_path(path)
}

pub fn load_texture(assets: &Assets, root: &Path, name: &str) -> TextureId
{
    let name = load_texture_path(root, name);

    assets.texture_id(&name)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Symmetry
{
    None,
    Horizontal,
    Vertical,
    Both,
    All
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
