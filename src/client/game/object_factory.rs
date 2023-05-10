use std::{
	collections::HashMap,
	sync::Arc
};

use parking_lot::RwLock;

use super::{
	super::{DescriptorSetUploader, ObjectAllocator},
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
	camera: Arc<RwLock<Camera>>,
	default_model: Arc<RwLock<Model>>,
	texture_ids: HashMap<String, usize>,
	textures: Vec<Arc<RwLock<Texture>>>,
	allocator: ObjectAllocator
}

impl ObjectFactory
{
	pub fn new(
		camera: Arc<RwLock<Camera>>,
		allocator: ObjectAllocator,
		textures: HashMap<String, Arc<RwLock<Texture>>>
	) -> Self
	{
		let default_model = Arc::new(RwLock::new(Model::square(1.0)));

		let (texture_ids, textures) = textures.into_iter().enumerate()
			.map(|(index, (name, texture))|
			{
				((name, index), texture)
			}).unzip();

		Self{camera, allocator, default_model, texture_ids, textures}
	}

	pub fn new_with_ids(
		camera: Arc<RwLock<Camera>>,
		allocator: ObjectAllocator,
		textures: Vec<Arc<RwLock<Texture>>>
	) -> Self
	{
		let default_model = Arc::new(RwLock::new(Model::square(1.0)));

		let texture_ids = HashMap::new();

		Self{camera, allocator, default_model, texture_ids, textures}
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

	pub fn default_model(&self) -> Arc<RwLock<Model>>
	{
		self.default_model.clone()
	}

	pub fn create(&self, model: Arc<RwLock<Model>>, transform: Transform, texture: &str) -> Object
	{
		self.create_with_texture(model, transform, self.texture_by_name(texture))
	}

	pub fn create_id(&self, model: Arc<RwLock<Model>>, transform: Transform, id: usize) -> Object
	{
		self.create_with_texture(model, transform, self.textures[id].clone())
	}

	fn create_with_texture(
		&self,
		model: Arc<RwLock<Model>>,
		transform: Transform,
		texture: Arc<RwLock<Texture>>
	) -> Object
	{
		let object_transform = ObjectTransform::new_transformed(transform);

		Object::new(
			self.camera.clone(),
			model,
			texture,
			object_transform,
			&self.allocator
		)
	}
}