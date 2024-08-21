use std::cell::{RefCell, RefMut};

use nalgebra::Vector3;

use crate::common::{
    collider::*,
    AnyEntities,
    world::World,
    entity::{
        for_each_component,
        iterate_components_with,
        ComponentWrapper,
        ClientEntities
    }
};


pub fn update(
    entities: &mut ClientEntities,
    world: &World
)
{
    macro_rules! colliding_info
    {
        ($result_variable:expr, $physical:expr, $collider:expr, $entity:expr) =>
        {
            let mut collider: RefMut<Collider> = $collider.borrow_mut();
            let target_non_lazy = collider.target_non_lazy;
            {
                let mut transform = entities.transform($entity).unwrap().clone();
                if collider.kind == ColliderType::Aabb
                {
                    transform.rotation = 0.0;
                }

                $result_variable = CollidingInfo{
                    entity: Some($entity),
                    physical: $physical.as_deref_mut(),
                    target: |mut offset: Vector3<f32>, rotation: Option<f32>|
                    {
                        let mut target = if target_non_lazy
                        {
                            entities.transform_mut($entity).unwrap()
                        } else
                        {
                            let target = entities.target($entity).unwrap();

                            if let Some(parent) = entities.parent($entity)
                            {
                                let parent_scale = entities.transform(parent.entity)
                                    .unwrap()
                                    .scale;

                                offset = offset.component_div(&parent_scale);
                            }

                            target
                        };

                        target.position += offset;

                        if let Some(rotation) = rotation
                        {
                            target.rotation += rotation;
                        }

                        target.position
                    },
                    basic: BasicCollidingInfo{
                        transform,
                        collider: &mut collider
                    }
                };
            }
        }
    }

    for_each_component!(entities, collider, |_, collider: &RefCell<Collider>|
    {
        collider.borrow_mut().reset_frame();
    });

    let pairs_fn = |&ComponentWrapper{
        entity,
        component: ref collider
    }, &ComponentWrapper{
        entity: other_entity,
        component: ref other_collider
    }|
    {
        let mut physical = entities.physical_mut(entity);
        let mut this;
        colliding_info!{this, physical, collider, entity};

        let mut other_physical = entities.physical_mut(other_entity);
        let other;
        colliding_info!{other, other_physical, other_collider, other_entity};

        this.resolve(other);
    };

    {
        let mut colliders = entities.collider.iter().map(|(_, x)| x);

        // calls the function for each unique combination (excluding (entities, entities) pairs)
        colliders.clone().for_each(|a|
        {
            colliders.by_ref().next();
            colliders.clone().for_each(|b| pairs_fn(a, b));
        });
    }

    for_each_component!(entities, collider, |entity, collider: &RefCell<_>|
    {
        let mut physical = entities.physical_mut(entity);
        let mut this;
        colliding_info!{this, physical, collider, entity};

        this.resolve_with_world(world);
    });
}
