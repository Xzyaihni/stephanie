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
	object::{
		Object,
		model::Model,
		texture::Texture
	}
};


#[derive(Debug)]
pub struct ObjectFactory
{
	device: Arc<Device>,
	layout: Arc<PipelineLayout>,
	camera: Arc<RwLock<Camera>>,
	textures: Vec<Arc<RwLock<Texture>>>,
	square: Arc<Model>
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
		let square = Arc::new(Model::rectangle(0.1));

		Self{device, layout, camera, textures, square}
	}

	pub fn swap_pipeline(&mut self, uploader: &DescriptorSetUploader)
	{
		self.textures.iter_mut().for_each(|texture|
		{
			texture.write().swap_pipeline(uploader)
		});
	}

	pub fn create(&self, texture_id: usize) -> Object
	{
		let allocator = FastMemoryAllocator::new_default(self.device.clone());

		Object::new(
			allocator,
			self.layout.clone(),
			self.camera.clone(),
			self.square.clone(),
			self.textures[texture_id].clone()
		)
	}
}