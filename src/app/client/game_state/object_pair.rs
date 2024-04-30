use std::borrow::Borrow;

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

use crate::{
    client::DrawableEntity,
    common::{
        Damage,
        DamageDirection,
        Damageable,
        Entity,
        ChildContainer,
        EntityAny,
        EntityAnyWrappable,
        Physical,
        physics::PhysicsEntity,
        entity::{ChildEntity, EntityContainer}
    }
};


#[derive(Debug)]
pub struct ObjectPair<T>
{
    main_object: Option<Object>,
	child_objects: Vec<Object>,
    z_index: usize,
	pub entity: T
}

impl<T: EntityContainer + PhysicsEntity + ChildContainer + DrawableEntity> ObjectPair<T>
{
	pub fn new(create_info: &ObjectCreateInfo, entity: T) -> Self
	{
        let main_object = Self::object_create(create_info, &entity);

        let children = entity.children_ref();

        let child_objects = children.iter().filter_map(|child|
        {
            Self::object_create(create_info, child)
        }).collect();

        let z_index = children.iter().position(|child| child.z_level() > 0).unwrap_or(0);

		Self{main_object, child_objects, z_index, entity}
	}

	pub fn update(&mut self, dt: f32)
	{
		self.physics_update(dt);
	}

	fn object_create<E: DrawableEntity + TransformContainer>(
        create_info: &ObjectCreateInfo,
		entity: &E
	) -> Option<Object>
	{
        let partial = &create_info.partial;
        let assets = &*partial.assets.lock();

		let model = assets.default_model(DefaultModel::Square);
        entity.texture().map(|texture|
        {
            partial.object_factory.create(
                ObjectInfo{
                    model,
                    texture: assets.texture(texture),
                    transform: entity.transform_clone()
                }
            )
        })
	}
}

impl<T> Borrow<T> for ObjectPair<T>
{
    fn borrow(&self) -> &T
    {
        &self.entity
    }
}

impl<T: EntityContainer + PhysicsEntity + ChildContainer> ChildContainer for ObjectPair<T>
{
	fn children_ref(&self) -> &[ChildEntity]
    {
        self.entity.children_ref()
    }

	fn children_mut(&mut self) -> &mut Vec<ChildEntity>
    {
        self.entity.children_mut()
    }
}

impl<T: EntityAnyWrappable + Clone> EntityAnyWrappable for &ObjectPair<T>
{
    fn wrap_any(self) -> EntityAny
    {
        self.entity.clone().wrap_any()
    }
}

impl<T: PhysicsEntity + ChildContainer> GameObject for ObjectPair<T>
{
	fn update_buffers(&mut self, info: &mut UpdateBuffersInfo)
    {
        if let Some(object) = self.main_object.as_mut()
        {
            object.update_buffers(info);
        }

		self.child_objects.iter_mut().for_each(|object| object.update_buffers(info));
    }

	fn draw(&self, info: &mut DrawInfo)
    {
        if self.child_objects.is_empty()
        {
            if let Some(object) = self.main_object.as_ref()
            {
                object.draw(info);
            }
        } else
        {
		    self.child_objects.iter().enumerate().for_each(|(index, object)|
            {
                if self.z_index == index
                {
                    if let Some(object) = self.main_object.as_ref()
                    {
                        object.draw(info);
                    }
                }

                object.draw(info);
            });
        }
    }
}

impl<T: EntityContainer + PhysicsEntity + ChildContainer> OnTransformCallback for ObjectPair<T>
{
	fn transform_callback(&mut self, transform: Transform)
	{
        if let Some(object) = self.main_object.as_mut()
        {
            object.set_transform(transform);
        }

		self.child_objects.iter_mut().zip(self.entity.children_ref().iter())
            .for_each(|(object, child)|
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

impl<T: EntityContainer + PhysicsEntity + ChildContainer> TransformContainer for ObjectPair<T>
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

impl<T: EntityContainer + PhysicsEntity + ChildContainer> PhysicsEntity for ObjectPair<T>
{
	fn physical_ref(&self) -> &Physical
	{
		self.entity.physical_ref()
	}

	fn physical_mut(&mut self) -> &mut Physical
	{
		self.entity.physical_mut()
	}

	fn physics_update(&mut self, dt: f32)
    {
        self.entity.physics_update(dt);

        self.transform_callback(self.transform_clone());
    }
}

impl<T: EntityContainer> EntityContainer for ObjectPair<T>
{
    fn entity_ref(&self) -> &Entity
    {
        self.entity.entity_ref()
    }

    fn entity_mut(&mut self) -> &mut Entity
    {
        self.entity.entity_mut()
    }
}

impl<T: Damageable> Damageable for ObjectPair<T>
{
    fn damage(&mut self, direction: DamageDirection, damage: Damage)
    {
        self.entity.damage(direction, damage);
    }
}
