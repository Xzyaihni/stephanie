use std::cell::RefCell;

use nalgebra::{Unit, Vector3};

use yanyaengine::Transform;

use crate::{
    DEBUG_COLLISION_BOUNDS,
    DEBUG_CONTACTS,
    common::{
        unique_pairs_no_self,
        collider::*,
        render_info::*,
        watcher::*,
        Entity,
        SpatialGrid,
        Joint,
        EntityInfo,
        AnyEntities,
        world::World,
        entity::{
            for_each_component,
            iterate_components_with,
            ClientEntities
        }
    }
};

use resolver::ContactResolver;
pub use resolver::PENETRATION_EPSILON;

mod resolver;


pub fn update(
    entities: &mut ClientEntities,
    world: &World,
    space: &SpatialGrid,
    dt: f32
)
{
    macro_rules! colliding_info
    {
        ($result_variable:expr, $entity:expr) =>
        {
            let mut collider = entities.collider_mut($entity).unwrap();
            {
                let mut transform = entities.transform($entity).unwrap().clone();

                let kind = collider.kind;
                if kind == ColliderType::Aabb
                {
                    transform.rotation = 0.0;
                }

                if let Some(scale) = collider.scale
                {
                    transform.scale = scale;
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

    space.possible_pairs(|possible|
    {
        let pairs_fn = |entity: Entity, other_entity: Entity|
        {
            let mut this;
            colliding_info!{this, entity};

            let other;
            colliding_info!{other, other_entity};

            this.collide(other, |contact| contacts.push(contact));
        };

        unique_pairs_no_self(possible.iter().copied(), pairs_fn);
    });

    for_each_component!(entities, collider, |entity, _collider|
    {
        let mut this;
        colliding_info!{this, entity};

        if DEBUG_COLLISION_BOUNDS
        {
            entities.push(true, EntityInfo{
                transform: Some(Transform{
                    position: this.transform.position,
                    scale: this.bounds() * 2.0,
                    ..Default::default()
                }),
                render: Some(RenderInfo{
                    object: Some(RenderObjectKind::Texture{
                        name: "placeholder.png".to_owned()
                    }.into()),
                    z_level: ZLevel::UiMiddle,
                    ..Default::default()
                }),
                watchers: Some(Watchers::simple_one_frame()),
                ..Default::default()
            });
        }

        this.collide_with_world(world, &mut contacts);
    });

    for_each_component!(entities, joint, |entity, joint: &RefCell<Joint>|
    {
        let parent = entities.parent(entity).unwrap();
        let transform = entities.transform(entity).unwrap();

        let parent_position = entities.transform(parent.entity()).unwrap().position;

        joint.borrow().add_contacts(&transform, entity, parent_position, &mut contacts);
    });

    if DEBUG_CONTACTS
    {
        contacts.iter().for_each(|contact|
        {
            let watchers = Some(Watchers::simple_one_frame());

            let color = if contact.b.is_some()
            {
                [0.0, 1.0, 0.0]
            } else
            {
                [1.0, 0.0, 0.0]
            };

            entities.push_eager(true, EntityInfo{
                transform: Some(Transform{
                    position: contact.point,
                    scale: Vector3::repeat(0.01),
                    ..Default::default()
                }),
                render: Some(RenderInfo{
                    object: Some(RenderObjectKind::Texture{
                        name: "circle.png".to_owned()
                    }.into()),
                    z_level: ZLevel::Hat,
                    mix: Some(MixColor{color, amount: 1.0}),
                    ..Default::default()
                }),
                watchers: watchers.clone(),
                ..Default::default()
            });

            if let Some(normal_2d) = Unit::try_new(contact.normal.xy(), 0.01)
            {
                let angle = normal_2d.y.atan2(normal_2d.x);

                let arrow_scale = 0.05;

                entities.push_eager(true, EntityInfo{
                    transform: Some(Transform{
                        position: contact.point + contact.normal * arrow_scale / 2.0,
                        scale: Vector3::repeat(arrow_scale),
                        rotation: angle,
                        ..Default::default()
                    }),
                    render: Some(RenderInfo{
                        object: Some(RenderObjectKind::Texture{
                            name: "arrow.png".to_owned()
                        }.into()),
                        z_level: ZLevel::Door,
                        mix: Some(MixColor{color, amount: 1.0}),
                        aspect: Aspect::KeepMax,
                        ..Default::default()
                    }),
                    watchers: watchers.clone(),
                    ..Default::default()
                });
            }
        });
    }

    ContactResolver::resolve(entities, contacts, dt);
}
