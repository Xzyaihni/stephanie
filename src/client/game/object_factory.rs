use std::{
	sync::Arc
};

use parking_lot::RwLock;

use vulkano::{
	device::Device,
	pipeline::PipelineLayout,
	memory::allocator::FastMemoryAllocator
};

use super::{
	super::DescriptorSetUploader,
	camera::Camera,
	object_transform::ObjectTransform,
	object::{
		Object,
		model::Model,
		texture::Texture
	}
};

use crate::common::{
	Transform
};


#[derive(Debug, Clone)]
pub struct ObjectFactory
{
	device: Arc<Device>,
	layout: Arc<PipelineLayout>,
	camera: Arc<RwLock<Camera>>,
	textures: Vec<Arc<RwLock<Texture>>>
}

impl ObjectFactory
{
	pub fn new(
		device: Arc<Device>,
		layout: Arc<PipelineLayout>,
		camera: Arc<RwLock<Camera>>,
		textures: Vec<Arc<RwLock<Texture>>>
	) -> Self
	{
		Self{device, layout, camera, textures}
	}

	pub fn swap_pipeline(&mut self, uploader: &DescriptorSetUploader)
	{
		self.textures.iter_mut().for_each(|texture|
		{
			texture.write().swap_pipeline(uploader)
		});
	}

	pub fn create(&self, model: Arc<Model>, transform: Transform, texture_id: usize) -> Object
	{
		let allocator = FastMemoryAllocator::new_default(self.device.clone());

		Object::new(
			allocator,
			self.layout.clone(),
			self.camera.clone(),
			model,
			self.textures[texture_id].clone(),
			ObjectTransform::new_transformed(transform)
		)
	}
}