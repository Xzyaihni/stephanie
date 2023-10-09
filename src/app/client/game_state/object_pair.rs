use std::iter;

use nalgebra::{
	Unit,
	Vector3
};

use yanyaengine::{
    DefaultModel,
    Object,
    ObjectInfo,
    Transform,
    TransformContainer,
	OnTransformCallback,
    game_object::*
};

use crate::common::{
	PlayerGet,
	ChildContainer,
	physics::PhysicsEntity,
	player::Player,
	entity::{ChildEntity, Entity}
};

use crate::client::DrawableEntity;


#[derive(Debug)]
pub struct ObjectPair<T>
{
	pub objects: Vec<Object>,
	pub entity: T
}

impl<T: PhysicsEntity + DrawableEntity + ChildContainer> ObjectPair<T>
{
	pub fn new(create_info: &ObjectCreateInfo, entity: T) -> Self
	{
        let entity_object = Self::object_create(create_info, &entity);

        let objects = entity.children_ref().iter().map(|child|
        {
            Self::child_object_create(create_info, child)
        }).chain(iter::once(entity_object)).collect();

		Self{objects, entity}
	}

	pub fn update(&mut self, dt: f32)
	{
		self.physics_update(dt);
	}

	fn object_create<E: DrawableEntity + TransformContainer>(
        create_info: &ObjectCreateInfo,
		entity: &E
	) -> Object
	{
        let partial = &create_info.partial;
        let assets = &*partial.assets.lock();

		let model = assets.default_model(DefaultModel::Square);
        let texture = assets.texture(entity.texture());

        partial.object_factory.create(
            ObjectInfo{
                model,
                texture,
                transform: entity.transform_clone()
            }
		)
	}

    fn child_object_create(
        create_info: &ObjectCreateInfo,
        entity: &ChildEntity
    ) -> Object
    {
        let mut object = Self::object_create(create_info, entity.entity_ref());

		object.set_origin(entity.origin());

        object
    }
}

impl PlayerGet for ObjectPair<Player>
{
	fn player(&self) -> Player
	{
		self.entity.clone()
	}
}

impl<T: PhysicsEntity + ChildContainer> GameObject for ObjectPair<T>
{
	fn update_buffers(&mut self, info: &mut UpdateBuffersInfo)
    {
		self.objects.iter_mut().for_each(|object| object.update_buffers(info));
    }

	fn draw(&self, info: &mut DrawInfo)
    {
		self.objects.iter().for_each(|object| object.draw(info));
    }
}

impl<T: TransformContainer + ChildContainer> OnTransformCallback for ObjectPair<T>
{
	fn transform_callback(&mut self, _transform: Transform)
	{
		self.objects.iter_mut().zip(self.entity.children_ref().iter()).for_each(|(object, child)|
        {
            object.set_transform(child.entity_ref().transform_clone())
        });
	}

	fn position_callback(&mut self, position: Vector3<f32>)
	{
		self.entity.position_callback(position);

		self.transform_callback(self.transform_clone());
	}

	fn scale_callback(&mut self, scale: Vector3<f32>)
	{
		self.entity.scale_callback(scale);

		self.transform_callback(self.transform_clone());
	}

	fn rotation_callback(&mut self, rotation: f32)
	{
		self.entity.rotation_callback(rotation);

		self.transform_callback(self.transform_clone());
	}

	fn rotation_axis_callback(&mut self, axis: Unit<Vector3<f32>>)
	{
		self.entity.rotation_axis_callback(axis);

		self.transform_callback(self.transform_clone());
	}
}

impl<T: TransformContainer + ChildContainer> TransformContainer for ObjectPair<T>
{
	fn transform_ref(&self) -> &Transform
	{
		self.entity.transform_ref()
	}

	fn transform_mut(&mut self) -> &mut Transform
	{
		self.entity.transform_mut()
	}
}

impl<T: PhysicsEntity + ChildContainer> PhysicsEntity for ObjectPair<T>
{
	fn entity_ref(&self) -> &Entity
	{
		self.entity.entity_ref()
	}

	fn entity_mut(&mut self) -> &mut Entity
	{
		self.entity.entity_mut()
	}

	fn physics_update(&mut self, dt: f32)
	{
		self.entity.physics_update(dt);

		self.transform_callback(self.transform_clone());
	}

	fn velocity_add(&mut self, velocity: Vector3<f32>)
	{
		self.entity.velocity_add(velocity);
	}
}
