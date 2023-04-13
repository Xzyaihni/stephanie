use std::{
	collections::HashMap,
	sync::Arc
};

use parking_lot::RwLock;

use vulkano::memory::allocator::StandardMemoryAllocator;

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


#[derive(Debug)]
pub struct ObjectFactory
{
	allocator: StandardMemoryAllocator,
	camera: Arc<RwLock<Camera>>,
	texture_ids: HashMap<String, usize>,
	textures: Vec<Arc<RwLock<Texture>>>
}

impl ObjectFactory
{
	pub fn new(
		allocator: StandardMemoryAllocator,
		camera: Arc<RwLock<Camera>>,
		textures: HashMap<String, Arc<RwLock<Texture>>>
	) -> Self
	{
		let (texture_ids, textures) = textures.into_iter().enumerate()
			.map(|(index, (name, texture))|
			{
				((name, index), texture)
			}).unzip();

		Self{allocator, camera, texture_ids, textures}
	}

	pub fn new_with_ids(
		allocator: StandardMemoryAllocator,
		camera: Arc<RwLock<Camera>>,
		textures: Vec<Arc<RwLock<Texture>>>
	) -> Self
	{
		let texture_ids = HashMap::new();

		Self{allocator, camera, texture_ids, textures}
	}

	pub fn swap_pipeline(&mut self, uploader: &DescriptorSetUploader)
	{
		self.textures.iter_mut().for_each(|texture|
		{
			texture.write().swap_pipeline(uploader)
		});
	}

	fn texture_by_name(&self, name: &str) -> Arc<RwLock<Texture>>
	{
		self.textures[self.texture_ids[name]].clone()
	}

	pub fn create(&self, model: Arc<Model>, transform: Transform, texture: &str) -> Object
	{
		self.create_with_texture(model, transform, self.texture_by_name(texture))
	}

	pub fn create_id(&self, model: Arc<Model>, transform: Transform, id: usize) -> Object
	{
		self.create_with_texture(model, transform, self.textures[id].clone())
	}

	fn create_with_texture(
		&self,
		model: Arc<Model>,
		transform: Transform,
		texture: Arc<RwLock<Texture>>
	) -> Object
	{
		let object_transform = ObjectTransform::new_transformed(transform);

		Object::new(
			&self.allocator,
			self.camera.clone(),
			model,
			texture,
			object_transform
		)
	}
}