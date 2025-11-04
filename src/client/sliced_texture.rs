use std::path::Path;

use serde::{Serialize, Deserialize};

use nalgebra::vector;

use yanyaengine::{
    ResourceUploader,
    Assets,
    TextureId,
    object::{texture::SimpleImage, Texture}
};

use crate::common::with_error;


pub struct PartCreator<'a, 'b>
{
    pub resource_uploader: &'a mut ResourceUploader<'b>,
    pub assets: &'a mut Assets
}

impl PartCreator<'_, '_>
{
    pub fn create(&mut self, image: impl Into<SimpleImage>) -> TextureId
    {
        let texture = Texture::new(
            self.resource_uploader,
            image.into().into()
        );

        self.assets.push_texture(texture)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SlicedTexture
{
    pub id: TextureId,
    pub width: f32,
    pub height: f32
}

impl SlicedTexture
{
    pub fn new(
        part_creator: &mut PartCreator,
        path: &Path
    ) -> Option<(String, Self)>
    {
        let stem = path.file_stem()?.to_string_lossy();

        let image = with_error(image::open(path))?;

        let (size, name) = {
            let index = stem.chars().position(|x| x == '_')?;

            let size_string = stem.chars().take(index).collect::<String>();
            let mut sizes = size_string.split('x');

            let mut n = || -> Option<u32>
            {
                with_error(sizes.next()?.parse())
            };

            let size = vector![n()?, n()?];

            (size, stem.chars().skip(index + 1).collect::<String>())
        };

        let width = image.width();
        let height = image.height();

        if size.x >= width || size.y >= height
        {
            eprintln!("({}) image size must be bigger than {} by {} (image is {width} by {height})", path.display(), size.x, size.y);
            return None;
        }

        let f = |size, small|
        {
            ((size - small) / 2) as f32 / size as f32
        };

        let this = Self{
            width: f(width, size.x),
            height: f(height, size.y),
            id: part_creator.create(image)
        };

        Some((name, this))
    }
}
