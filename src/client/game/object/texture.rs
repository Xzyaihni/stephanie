use std::{
    fmt,
    path::Path,
    ops::Deref,
    sync::Arc
};

use vulkano::{
    format::Format,
    image::{
        MipmapsCount,
        ImmutableImage,
        ImageDimensions,
        view::ImageView
    },
    descriptor_set::{
        PersistentDescriptorSet,
        WriteDescriptorSet
    }
};

use image::{
    ColorType,
    DynamicImage,
    error::ImageError
};

use super::resource_uploader::{DescriptorSetUploader, ResourceUploader};


#[derive(Debug, Clone, Copy)]
pub struct Color
{
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8
}

impl Color
{
    pub fn new(r: u8, g: u8, b: u8, a: u8) -> Self
    {
        Self{r, g, b, a}
    }
}

#[derive(Debug, Clone)]
pub struct SimpleImage
{
    pub colors: Vec<Color>,
    pub width: usize,
    pub height: usize
}

#[allow(dead_code)]
impl SimpleImage
{
    pub fn new(colors: Vec<Color>, width: usize, height: usize) -> Self
    {
        Self{colors,  width, height}
    }

    pub fn load(filepath: impl AsRef<Path>) -> Result<Self, ImageError>
    {
        let image = image::open(filepath)?;

        Self::try_from(image)
    }

    pub fn get_pixel(&self, x: usize, y: usize) -> Color
    {
        self.colors[self.index_of(x, y)]
    }

    pub fn set_pixel(&mut self, color: Color, x: usize, y: usize)
    {
        let index = self.index_of(x, y);
        self.colors[index] = color;
    }

    fn index_of(&self, x: usize, y: usize) -> usize
    {
        y * self.width + x
    }
}

impl TryFrom<DynamicImage> for SimpleImage
{
    type Error = ImageError;

    fn try_from(other: DynamicImage) -> Result<Self, Self::Error>
    {
        let width = other.width() as usize;
        let height = other.height() as usize;

        let colors = other.into_rgba8().into_raw().chunks(4).map(|bytes: &[u8]|
        {
            Color::new(bytes[0], bytes[1], bytes[2], bytes[3])
        }).collect();

        Ok(Self{colors, width, height})
    }
}

#[derive(Debug, Clone)]
pub struct RgbaImage
{
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32
}

#[allow(dead_code)]
impl RgbaImage
{
    pub fn new(data: Vec<u8>, width: u32, height: u32) -> Self
    {
        Self{data, width, height}
    }

    pub fn load(filepath: impl AsRef<Path>) -> Result<Self, ImageError>
    {
        let image = image::open(filepath)?;

        let width = image.width();
        let height = image.height();

        let data = image.into_rgba8().into_raw();

        Ok(Self{data, width, height})
    }

    pub fn save(&self, filename: impl AsRef<Path>) -> Result<(), ImageError>
    {
        image::save_buffer(filename, &self.data, self.width, self.height, ColorType::Rgba8)
    }
}

impl From<SimpleImage> for RgbaImage
{
    fn from(other: SimpleImage) -> Self
    {
        let data = other.colors.into_iter().flat_map(|color| [color.r, color.g, color.b, color.a])
            .collect();

        Self::new(data, other.width as u32, other.height as u32)
    }
}

#[derive(Clone)]
pub struct Texture
{
    image: RgbaImage,
    view: Arc<ImageView<ImmutableImage>>,
    descriptor_set: Arc<PersistentDescriptorSet>
}

impl Texture
{
    pub fn new(
        resource_uploader: &mut ResourceUploader,
        image: RgbaImage
    ) -> Self
    {
        let view = Self::calculate_descriptor_set(resource_uploader, &image);
        let descriptor_set = Self::calculate_decriptor_set(
            view.clone(),
            &resource_uploader.descriptor
        );

        Self{image, view, descriptor_set}
    }

    fn calculate_descriptor_set(
        resource_uploader: &mut ResourceUploader,
        image: &RgbaImage
    ) -> Arc<ImageView<ImmutableImage>>
    {
        let image = ImmutableImage::from_iter(
            &resource_uploader.allocator,
            image.data.iter().cloned(),
            ImageDimensions::Dim2d{
                width: image.width,
                height: image.height,
                array_layers: 1
            },
            MipmapsCount::Log2,
            Format::R8G8B8A8_SRGB,
            resource_uploader.builder
        ).unwrap();

        ImageView::new_default(image).unwrap()
    }

    pub fn swap_pipeline(&mut self, uploader: &DescriptorSetUploader)
    {
        self.descriptor_set = Self::calculate_decriptor_set(self.view.clone(), uploader);
    }

    fn calculate_decriptor_set(
        view: Arc<ImageView<ImmutableImage>>,
        uploader: &DescriptorSetUploader
    ) -> Arc<PersistentDescriptorSet>
    {
        PersistentDescriptorSet::new(
            &uploader.allocator,
            uploader.layout.clone(),
            [
                WriteDescriptorSet::image_view_sampler(
                    0, view, uploader.sampler.clone()
                )
            ]
        ).unwrap()
    }

    pub fn descriptor_set(&self) -> Arc<PersistentDescriptorSet>
    {
        self.descriptor_set.clone()
    }
}

impl Deref for Texture
{
    type Target = RgbaImage;

    fn deref(&self) -> &Self::Target
    {
        &self.image
    }
}

impl fmt::Debug for Texture
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result
    {
        f.debug_struct("Texture")
            .field("image", &self.image)
            .field("view", &self.view)
            .finish()
    }
}