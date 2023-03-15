use std::{
    fmt,
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

use image::error::ImageError;

use super::resource_uploader::{DescriptorSetUploader, ResourceUploader};


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
    pub fn new(
        data: Vec<u8>,
        width: u32,
        height: u32
    ) -> Self
    {
        Self{data, width, height}
    }

    pub fn load(filepath: &str) -> Result<Self, ImageError>
    {
        let image = image::open(filepath)?;

        let width = image.width();
        let height = image.height();

        let data = image.into_rgba8().into_raw();

        Ok(Self{data, width, height})
    }
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct Texture
{
    image: RgbaImage,
    view: Arc<ImageView<ImmutableImage>>,
    descriptor_set: Arc<PersistentDescriptorSet>
}

#[allow(dead_code)]
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
            &mut resource_uploader.builder
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
                    0, view.clone(), uploader.sampler.clone()
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