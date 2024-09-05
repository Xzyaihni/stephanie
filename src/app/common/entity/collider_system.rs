use std::cell::{RefCell, RefMut};

use crate::common::{
    collider::*,
    world::World,
    entity::{
        for_each_component,
        iterate_components_with,
        ComponentWrapper,
        ClientEntities
    }
};

use resolver::ContactResolver;

mod resolver;


pub fn update(
    entities: &mut ClientEntities,
    world: &World,
    dt: f32
)
{
    macro_rules! colliding_info
    {
        ($result_variable:expr, $collider:expr, $entity:expr) =>
        {
            let mut collider: RefMut<Collider> = $collider.borrow_mut();
            {
                let mut transform = entities.transform($entity).unwrap().clone();

                let kind = collider.kind;
                if kind == ColliderType::Aabb
                {
                    transform.rotation = 0.0;
                }

                $result_variable = CollidingInfo{
                    entity: Some($entity),
                    transform,
                    collider: &mut collider
                };
            }
        }
    }

    for_each_component!(entities, collider, |_, collider: &RefCell<Collider>|
    {
        collider.borrow_mut().reset_frame();
    });

    let mut contacts = Vec::new();

    let mut pairs_fn = |&ComponentWrapper{
        entity,
        component: ref collider
    }, &ComponentWrapper{
        entity: other_entity,
        component: ref other_collider
    }|
    {
        let mut this;
        colliding_info!{this, collider, entity};

        let other;
        colliding_info!{other, other_collider, other_entity};

        this.collide(other, Some(&mut contacts));
    };

    {
        let mut colliders = entities.collider.iter().map(|(_, x)| x);

        // calls the function for each unique combination (excluding (self, self) pairs)
        colliders.clone().for_each(|a|
        {
            colliders.by_ref().next();
            colliders.clone().for_each(|b| pairs_fn(a, b));
        });
    }

    for_each_component!(entities, collider, |entity, collider: &RefCell<_>|
    {
        let mut this;
        colliding_info!{this, collider, entity};

        this.collide_with_world(world, &mut contacts);
    });

    ContactResolver::resolve(entities, contacts, dt);
}
