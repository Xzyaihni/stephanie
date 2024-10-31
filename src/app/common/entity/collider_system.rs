use std::cell::RefCell;

use yanyaengine::Transform;

use crate::{
    debug_config::*,
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
            ClientEntities
        }
    }
};

use resolver::ContactResolver;

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

        if DebugConfig::is_enabled(DebugTool::CollisionBounds)
        {
            entities.push(true, EntityInfo{
                transform: Some(Transform{
                    position: this.transform.position,
                    scale: this.bounds() * 2.0,
                    ..Default::default()
                }),
                render: Some(RenderInfo{
                    object: Some(RenderObjectKind::Texture{
                        name: "ui/background.png".to_owned()
                    }.into()),
                    z_level: ZLevel::highest_non_ui(),
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

    ContactResolver::resolve(entities, contacts, dt);
}
