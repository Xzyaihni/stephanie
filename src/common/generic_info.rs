use std::{
    path::Path,
    collections::HashMap
};

use serde::Deserialize;

use nalgebra::Vector2;

use yanyaengine::{Assets, TextureId, object::texture::SimpleImage, game_object::*};

pub use crate::{
    inherit_with_fields,
    some_or_return,
    some_or_value,
    define_info_id,
    common::{texture_scale, normalize_path}
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

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Sprite
{
    pub id: TextureId,
    pub scale: Vector2<f32>
}

impl Sprite
{
    pub fn new(assets: &Assets, texture: TextureId) -> Self
    {
        Sprite{
            id: texture,
            scale: texture_scale(&assets.texture(texture).lock())
        }
    }

    pub fn aspect(&self) -> Vector2<f32>
    {
        self.scale / self.scale.max()
    }

    pub fn combine(&self, builder_wrapper: &mut BuilderWrapper, assets: &mut Assets, other: &Self) -> Self
    {
        let load_image_of = |id| -> Option<_>
        {
            let parent_path = if let Some(x) = assets.textures_path()
            {
                x
            } else
            {
                eprintln!("empty assets, cant find {id:?}");

                return None;
            };

            let name = if let Some(x) = assets.texture_name(id)
            {
                x
            } else
            {
                eprintln!("cant find name of {id:?}");

                return None;
            };

            let path = parent_path.join(name);

            match SimpleImage::load(&path)
            {
                Ok(x) => Some(x),
                Err(err) =>
                {
                    eprintln!("error loading image at {} ({err})", path.display());

                    None
                }
            }
        };

        let mut a = some_or_value!(load_image_of(self.id), *self);
        let b = some_or_value!(load_image_of(other.id), *self);

        if (a.width != b.width) || (a.height != b.height)
        {
            eprintln!("cant combine sprites of different sizes");
            return *self;
        }

        a.blit_blend(&b, 0, 0);

        let combined_texture = builder_wrapper.create_texture(a.into());

        let new_id = assets.push_texture(combined_texture);

        Self{
            id: new_id,
            scale: self.scale
        }
    }
}

pub fn load_texture_path(root: impl AsRef<Path>, name: &str) -> String
{
    let formatted_name = name.replace(' ', "_") + ".png";
    let path = root.as_ref().join(formatted_name);

    normalize_path(path)
}

pub fn load_texture(assets: &Assets, root: &Path, name: &str) -> Sprite
{
    let name = load_texture_path(root, name);

    Sprite::new(assets, assets.texture_id(&name))
}

#[macro_export]
macro_rules! inherit_with_fields
{
    ($this:expr, $other:expr, $($name:ident),+) =>
    {
        $(
            if $other.$name.is_some()
            {
                $this.$name = $other.$name.clone();
            }
        )+
    }
}

pub fn inherit_infos<T>(
    infos: &mut [T],
    inherit_name: fn(&T) -> Option<&String>,
    info_name: fn(&T) -> &str,
    info_combine: fn(&T, &T) -> T
)
{
    (0..infos.len()).for_each(|index|
    {
        let this_inherit_name = some_or_return!(inherit_name(&infos[index]));

        if let Some(inherit_index) = infos.iter().position(|x| info_name(&x) == this_inherit_name)
        {
            infos[index] = info_combine(&infos[inherit_index], &infos[index]);
        } else
        {
            eprintln!("inherit named `{this_inherit_name}` not found");
        }
    });
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
