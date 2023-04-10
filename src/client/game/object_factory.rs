use std::{
	collections::HashMap,
	sync::Arc
};

use parking_lot::RwLock;

use vulkano::{
	device::Device,
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
	camera: Arc<RwLock<Camera>>,
	textures: HashMap<String, Arc<RwLock<Texture>>>
}

impl ObjectFactory
{
	pub fn new(
		device: Arc<Device>,
		camera: Arc<RwLock<Camera>>,
		textures: HashMap<String, Arc<RwLock<Texture>>>
	) -> Self
	{
		Self{device, camera, textures}
	}

	pub fn swap_pipeline(&mut self, uploader: &DescriptorSetUploader)
	{
		self.textures.values_mut().for_each(|texture|
		{
			texture.write().swap_pipeline(uploader)
		});
	}

	pub fn create(&self, model: Arc<Model>, transform: Transform, texture: &str) -> Object
	{
		self.create_with_texture(model, transform, self.textures[texture].clone())
	}

	pub fn create_only(&self, model: Arc<Model>, transform: Transform) -> Object
	{
		self.create_with_texture(model, transform, self.textures.values().next().unwrap().clone())
	}

	fn create_with_texture(
		&self,
		model: Arc<Model>,
		transform: Transform,
		texture: Arc<RwLock<Texture>>
	) -> Object
	{
		let allocator = FastMemoryAllocator::new_default(self.device.clone());

		let object_transform = ObjectTransform::new_transformed(transform);

		Object::new(
			allocator,
			self.camera.clone(),
			model,
			texture,
			object_transform
		)
	}
}