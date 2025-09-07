use std::{
    cmp::Ordering,
    num::FpCategory
};

use nalgebra::{vector, matrix, Unit, Matrix2, Vector2, Vector3};

use yanyaengine::Transform;

use crate::{
    debug_config::*,
    common::{
        with_z,
        cross_2d,
        direction_arrow_info,
        watcher::*,
        render_info::*,
        Physical,
        AnyEntities,
        EntityInfo,
        Entity,
        entity::ClientEntities,
        collider::Contact
    }
};


const ANGULAR_LIMIT: f32 = 0.2;
const VELOCITY_LOW: f32 = 0.002;
const ITERATIONS: usize = 50;

const PENETRATION_EPSILON: f32 = 0.0005;

fn basis_from(a: Unit<Vector2<f32>>) -> Matrix2<f32>
{
    matrix![
        a.x, -a.y;
        a.y, a.x
    ]
}

struct Inertias
{
    angular: f32,
    linear: f32
}

impl Inertias
{
    fn added(&self) -> f32
    {
        self.angular + self.linear
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WhichObject
{
    A,
    B
}

trait IteratedMoves
{
    fn inverted(self) -> Self;
}

#[derive(Debug, Clone, Copy)]
struct PenetrationMoves
{
    pub velocity_change: Vector2<f32>,
    pub angular_change: f32,
    pub inverted: bool
}

impl IteratedMoves for PenetrationMoves
{
    fn inverted(self) -> Self
    {
        Self{
            inverted: true,
            ..self
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct VelocityMoves
{
    pub velocity_change: Vector2<f32>,
    pub angular_change: f32,
    pub inverted: bool
}

impl IteratedMoves for VelocityMoves
{
    fn inverted(self) -> Self
    {
        Self{
            inverted: true,
            ..self
        }
    }
}

#[derive(Debug, Clone)]
struct AnalyzedContact
{
    pub contact: Contact,
    pub to_world: Matrix2<f32>,
    pub velocity: Vector2<f32>,
    pub desired_change: f32,
    pub a_inverse_inertia: f32,
    pub b_inverse_inertia: Option<f32>,
    pub a_relative: Vector2<f32>,
    pub b_relative: Option<Vector2<f32>>
}

impl AnalyzedContact
{
    fn calculate_desired_change(
        &mut self,
        entities: &ClientEntities,
        dt: f32
    )
    {
        self.desired_change = self.contact.calculate_desired_change(entities, &self.velocity, dt);
    }

    fn get_inverse_inertia(&self, which: WhichObject) -> f32
    {
        match which
        {
            WhichObject::A => self.a_inverse_inertia,
            WhichObject::B => self.b_inverse_inertia.unwrap()
        }
    }

    fn get_relative(&self, which: WhichObject) -> Vector2<f32>
    {
        match which
        {
            WhichObject::A => self.a_relative,
            WhichObject::B => self.b_relative.unwrap()
        }
    }

    fn get_entity(&self, which: WhichObject) -> Entity
    {
        match which
        {
            WhichObject::A => self.contact.a,
            WhichObject::B => self.contact.b.unwrap()
        }
    }

    fn inertias(&self, entities: &ClientEntities, which: WhichObject) -> Inertias
    {
        let angular_inertia_world = Contact::direction_apply_inertia(
            self.get_inverse_inertia(which),
            self.get_relative(which),
            *self.contact.normal
        );

        let physical = entities.physical(self.get_entity(which)).unwrap();

        Inertias{
            linear: physical.inverse_mass,
            angular: angular_inertia_world.dot(&self.contact.normal)
        }
    }

    fn apply_moves(
        &self,
        entities: &ClientEntities,
        inertias: Inertias,
        inverse_inertia: f32,
        which: WhichObject
    ) -> PenetrationMoves
    {
        let penetration = match which
        {
            WhichObject::A => self.contact.penetration,
            WhichObject::B => -self.contact.penetration
        } * inverse_inertia;

        let mut angular_amount = penetration * inertias.angular;
        let mut velocity_amount = penetration * inertias.linear;

        let contact_relative = self.get_relative(which);
        let entity = self.get_entity(which);
        let physical = entities.physical(entity).unwrap();

        let mut transform = if physical.target_non_lazy
        {
            entities.transform_mut(entity).unwrap()
        } else
        {
            entities.target(entity).unwrap()
        };

        let angular_projection = contact_relative
            + *self.contact.normal * (-contact_relative).dot(&self.contact.normal);

        let angular_limit = ANGULAR_LIMIT * angular_projection.magnitude();

        if angular_amount.abs() > angular_limit
        {
            let pre_limit = angular_amount;

            angular_amount = angular_amount.clamp(-angular_limit, angular_limit);

            velocity_amount += pre_limit - angular_amount;
        }

        let fixed = physical.fixed;

        let velocity_change = velocity_amount * *self.contact.normal;

        let position_change = velocity_change;

        let position_change = if physical.target_non_lazy
        {
            position_change
        } else
        {
            if let Some(parent) = entities.parent(entity)
            {
                let parent_scale = &entities.transform(parent.entity()).unwrap().scale.xy();
                position_change.component_div(parent_scale)
            } else
            {
                position_change
            }
        };

        transform.position += with_z(position_change, 0.0);

        let angular_change = if !fixed.rotation && (inertias.angular.classify() != FpCategory::Zero)
        {
            let impulse_torque = cross_2d(
                contact_relative.xy(),
                self.contact.normal.xy()
            ) * self.get_inverse_inertia(which);

            let angular_change = impulse_torque * (angular_amount / inertias.angular);
            transform.rotation += angular_change;

            angular_change
        } else
        {
            0.0
        };

        PenetrationMoves{
            velocity_change,
            angular_change,
            inverted: false
        }
    }

    fn resolve_penetration(
        &self,
        entities: &ClientEntities
    ) -> Option<(PenetrationMoves, Option<PenetrationMoves>)>
    {
        let inertias = self.inertias(entities, WhichObject::A);
        let mut total_inertia = inertias.added();

        let b_inertias = self.contact.b.map(|_|
        {
            self.inertias(entities, WhichObject::B)
        });

        if let Some(ref b_inertias) = b_inertias
        {
            total_inertia += b_inertias.added();
        }

        if total_inertia.classify() == FpCategory::Zero
        {
            return None;
        }

        let inverse_inertia = total_inertia.recip();

        let a_moves = self.apply_moves(entities, inertias, inverse_inertia, WhichObject::A);

        let b_moves = b_inertias.map(|b_inertias|
        {
            self.apply_moves(
                entities,
                b_inertias,
                inverse_inertia,
                WhichObject::B
            )
        });

        Some((a_moves, b_moves))
    }

    fn velocity_change(
        &self,
        which: WhichObject
    ) -> Matrix2<f32>
    {
        let v = self.get_relative(which);

        let e = -v.y;
        let g = v.x;

        let c = self.get_inverse_inertia(which);

        let cg = c * g;
        let ce = c * e;

        matrix![
            e * ce, e * cg;
            g * ce, g * cg
        ]
    }

    fn apply_impulse(
        &self,
        entities: &ClientEntities,
        impulse: Vector2<f32>,
        which: WhichObject
    ) -> VelocityMoves
    {
        let contact_relative = self.get_relative(which);

        let impulse_torque = cross_2d(contact_relative, impulse);

        let mut physical = entities.physical_mut_no_change(self.get_entity(which)).unwrap();

        let angular_change = impulse_torque * self.get_inverse_inertia(which);
        let velocity_change = impulse * physical.inverse_mass;

        if physical.fixed.rotation
        {
            debug_assert!(
                angular_change == 0.0,
                "angular_change: {angular_change}, impulse_torque: {impulse_torque}, impulse: {impulse}, contact_relative: {contact_relative}"
            );
        }

        physical.add_velocity_raw(with_z(velocity_change, 0.0));
        physical.add_angular_velocity_raw(angular_change);

        VelocityMoves{
            angular_change,
            velocity_change,
            inverted: false
        }
    }

    fn resolve_velocity(
        &self,
        entities: &ClientEntities
    ) -> Option<(VelocityMoves, Option<VelocityMoves>)>
    {
        let mut velocity_change_world = self.velocity_change(WhichObject::A);

        if self.contact.b.is_some()
        {
            let b_velocity_change_world = self.velocity_change(WhichObject::B);

            velocity_change_world += b_velocity_change_world;
        }

        if velocity_change_world.magnitude() == 0.0
        {
            return None;
        }

        let mut velocity_change: Matrix2<f32> = (self.to_world.transpose() * velocity_change_world) * self.to_world;

        let mut total_inverse_mass = entities.physical(self.contact.a).unwrap().inverse_mass;
        if let Some(b) = self.contact.b
        {
            total_inverse_mass += entities.physical(b).unwrap().inverse_mass;
        }

        (0..2).for_each(|i|
        {
            *velocity_change.index_mut((i, i)) += total_inverse_mass;
        });

        let impulse_local_matrix = velocity_change.try_inverse()?;

        let velocity_stop = Vector2::new(
            self.desired_change,
            -self.velocity.y
        );

        let impulse_local = impulse_local_matrix * velocity_stop;

        let impulse = self.to_world * impulse_local;

        let a_moves = self.apply_impulse(entities, impulse, WhichObject::A);

        let b_moves = self.contact.b.map(|_|
        {
            self.apply_impulse(entities, -impulse, WhichObject::B)
        });

        Some((a_moves, b_moves))
    }
}

impl Contact
{
    pub fn to_world_matrix(&self) -> Matrix2<f32>
    {
        if self.normal.x.abs() > self.normal.y.abs()
        {
            basis_from(self.normal)
        } else
        {
            basis_from(self.normal)
        }
    }

    fn direction_apply_inertia(
        inverse_inertia: f32,
        direction: Vector2<f32>,
        normal: Vector2<f32>
    ) -> Vector2<f32>
    {
        let angular_inertia = cross_2d(direction, normal) * inverse_inertia;

        vector![-angular_inertia * direction.y, angular_inertia * direction.x]
    }

    fn velocity_from_angular(angular: f32, contact_local: Vector2<f32>) -> Vector2<f32>
    {
        vector![-angular * contact_local.y, angular * contact_local.x]
    }

    fn velocity_closing(
        physical: &Physical,
        to_world: &Matrix2<f32>,
        contact_relative: Vector2<f32>
    ) -> Vector2<f32>
    {
        let relative_velocity = Self::velocity_from_angular(
            physical.angular_velocity(),
            contact_relative
        ) + physical.velocity().xy();

        to_world.transpose() * relative_velocity
    }

    fn restitution(&self, entities: &ClientEntities) -> f32
    {
        self.average_physical(entities, |x| x.restitution)
    }

    fn average_physical(
        &self,
        entities: &ClientEntities,
        f: impl Fn(&Physical) -> f32
    ) -> f32
    {
        let mut a = f(&entities.physical(self.a).unwrap());
        if let Some(b) = self.b
        {
            a = (a + f(&entities.physical(b).unwrap())) / 2.0;
        }

        a
    }

    fn calculate_desired_change(
        &self,
        entities: &ClientEntities,
        velocity_local: &Vector2<f32>,
        dt: f32
    ) -> f32
    {
        let mut acceleration_velocity = (entities.physical(self.a).unwrap().last_acceleration().xy() * dt)
            .dot(&self.normal);

        if let Some(b) = self.b
        {
            acceleration_velocity -= (entities.physical(b).unwrap().last_acceleration().xy() * dt)
                .dot(&self.normal);
        }

        let restitution = if velocity_local.x.abs() < VELOCITY_LOW
        {
            0.0
        } else
        {
            self.restitution(entities)
        };

        -velocity_local.x - restitution * (velocity_local.x - acceleration_velocity)
    }

    fn inverse_inertia_of(entities: &ClientEntities, entity: Entity) -> f32
    {
        entities.collider(entity).unwrap().inverse_inertia(
            &entities.physical(entity).unwrap(),
            &entities.transform(entity).as_ref().unwrap().scale
        )
    }

    fn analyze(self, entities: &ClientEntities, dt: f32) -> Option<AnalyzedContact>
    {
        let to_world = self.to_world_matrix();

        let a_relative = self.point - entities.transform(self.a)?.position.xy();
        let b_relative = self.b.and_then(|b| Some(self.point - entities.transform(b)?.position.xy()));

        let mut velocity = Self::velocity_closing(
            &*entities.physical(self.a)?,
            &to_world,
            a_relative
        );

        let a_inverse_inertia = Self::inverse_inertia_of(entities, self.a);

        let b_inverse_inertia = self.b.and_then(|b|
        {
            let b_velocity = Self::velocity_closing(
                &*entities.physical(b)?,
                &to_world,
                b_relative?
            );

            velocity -= b_velocity;

            Some(Self::inverse_inertia_of(entities, b))
        });

        if self.b.is_some() && b_inverse_inertia.is_none()
        {
            return None;
        }

        let desired_change = self.calculate_desired_change(entities, &velocity, dt);
        debug_assert!(!desired_change.is_nan());

        Some(AnalyzedContact{
            to_world,
            velocity,
            desired_change,
            a_inverse_inertia,
            b_inverse_inertia,
            a_relative,
            b_relative,
            contact: self
        })
    }
}

pub struct ContactResolver;

impl ContactResolver
{
    fn update_iterated<Moves: IteratedMoves + Copy>(
        entities: &ClientEntities,
        contacts: &mut [AnalyzedContact],
        moves: (Moves, Option<Moves>),
        bodies: (Entity, Option<Entity>),
        mut handle: impl FnMut(&ClientEntities, &mut AnalyzedContact, Moves, Vector2<f32>)
    )
    {
        let (a_move, b_move) = moves;
        let (a_id, b_id) = bodies;

        contacts.iter_mut().for_each(|x|
        {
            let point = x.contact.point;
            let relative = |entity: Entity|
            {
                point - entities.transform(entity).unwrap().position.xy()
            };

            let this_contact_a = x.contact.a;
            let this_contact_b = x.contact.b;

            let mut handle = |move_info, contact_relative|
            {
                handle(entities, x, move_info, contact_relative);
            };

            if this_contact_a == a_id
            {
                handle(a_move, relative(this_contact_a));
            }

            if Some(this_contact_a) == b_id
            {
                handle(b_move.unwrap(), relative(this_contact_a));
            }

            if this_contact_b == Some(a_id)
            {
                handle(a_move.inverted(), relative(this_contact_b.unwrap()));
            }

            #[allow(clippy::unnecessary_unwrap)]
            if this_contact_b.is_some() && this_contact_b == b_id
            {
                handle(b_move.unwrap().inverted(), relative(this_contact_b.unwrap()));
            }
        });
    }

    fn resolve_iterative<Moves: IteratedMoves + Copy>(
        entities: &ClientEntities,
        contacts: &mut [AnalyzedContact],
        epsilon: f32,
        compare: impl Fn(&AnalyzedContact) -> f32,
        mut resolver: impl FnMut(&ClientEntities, &mut AnalyzedContact) -> Option<(Moves, Option<Moves>)>,
        mut updater: impl FnMut(&ClientEntities, &mut AnalyzedContact, Moves, Vector2<f32>)
    )
    {
        fn contact_selector<'a, Compare: Fn(&AnalyzedContact) -> f32>(
            compare: &'a Compare,
            epsilon: f32
        ) -> impl for<'b> FnMut(&'b mut AnalyzedContact) -> Option<(f32, &'b mut AnalyzedContact)> + use<'a, Compare>
        {
            move |contact|
            {
                let change = compare(contact);

                (change > epsilon).then_some((change, contact))
            }
        }

        fn contact_handler<Moves: IteratedMoves + Copy>(
            entities: &ClientEntities,
            mut resolver: impl FnMut(&ClientEntities, &mut AnalyzedContact) -> Option<(Moves, Option<Moves>)>,
            contact: &mut AnalyzedContact
        ) -> Option<((Moves, Option<Moves>), (Entity, Option<Entity>))>
        {
            resolver(entities, contact).map(|moves|
            {
                let bodies = (contact.contact.a, contact.contact.b);

                debug_assert!(moves.1.is_some() == contact.contact.b.is_some());

                (moves, bodies)
            })
        }

        for i in 0..contacts.len()
        {
            if let Some((_, contact)) = contact_selector(&compare, epsilon)(&mut contacts[i])
            {
                if let Some((moves, bodies)) = contact_handler(entities, &mut resolver, contact)
                {
                    ContactResolver::update_iterated::<Moves>(
                        entities,
                        contacts,
                        moves,
                        bodies,
                        &mut updater
                    );
                }
            }
        }

        for _ in 0..ITERATIONS
        {
            if let Some((_, contact)) = contacts.iter_mut()
                .filter_map(contact_selector(&compare, epsilon))
                .max_by(|(a, _), (b, _)|
                {
                    a.partial_cmp(b).unwrap_or(Ordering::Less)
                })
            {
                if let Some((moves, bodies)) = contact_handler(entities, &mut resolver, contact)
                {
                    ContactResolver::update_iterated::<Moves>(
                        entities,
                        contacts,
                        moves,
                        bodies,
                        &mut updater
                    );
                }
            } else
            {
                break;
            }
        }
    }

    fn display_contact(entities: &ClientEntities, contact: &Contact)
    {
        let z = entities.transform(contact.a).unwrap().position.z;
        let color = if contact.b.is_some()
        {
            [0.0, 1.0, 0.0, 1.0]
        } else
        {
            [1.0, 0.0, 0.0, 1.0]
        };

        entities.push(true, EntityInfo{
            transform: Some(Transform{
                position: with_z(contact.point, z),
                scale: Vector3::repeat(contact.penetration),
                ..Default::default()
            }),
            render: Some(RenderInfo{
                object: Some(RenderObjectKind::Texture{
                    name: "circle.png".to_owned()
                }.into()),
                above_world: true,
                mix: Some(MixColor{color, amount: 1.0, keep_transparency: true}),
                ..Default::default()
            }),
            watchers: Some(Watchers::simple_one_frame()),
            ..Default::default()
        });

        if let Some(info) = direction_arrow_info(with_z(contact.point, z), with_z(*contact.normal, 0.0), 0.01, [color[0], color[1], color[2]])
        {
            entities.push(true, info);
        }
    }

    pub fn resolve(
        entities: &ClientEntities,
        contacts: Vec<Contact>,
        dt: f32
    )
    {
        if DebugConfig::is_enabled(DebugTool::Contacts)
        {
            contacts.iter().for_each(|contact| Self::display_contact(entities, contact));
        }

        if DebugConfig::is_enabled(DebugTool::PrintContactsCount)
        {
            eprintln!("resolving {} contacts", contacts.len());
        }

        if DebugConfig::is_enabled(DebugTool::NoResolve)
        {
            return;
        }

        let mut analyzed_contacts: Vec<_> = contacts.into_iter().filter_map(|contact|
        {
            contact.analyze(entities, dt)
        }).collect();

        Self::resolve_iterative(
            entities,
            &mut analyzed_contacts,
            PENETRATION_EPSILON,
            |contact| contact.contact.penetration,
            |entities, contact| contact.resolve_penetration(entities),
            |_obejcts, contact, move_info, contact_relative|
            {
                let contact_change = Contact::velocity_from_angular(
                    move_info.angular_change,
                    contact_relative
                ) + move_info.velocity_change;

                let change = contact_change.dot(&contact.contact.normal);

                debug_assert!(!change.is_nan());
                if move_info.inverted
                {
                    contact.contact.penetration += change;
                } else
                {
                    contact.contact.penetration -= change;
                }
            }
        );

        Self::resolve_iterative(
            entities,
            &mut analyzed_contacts,
            0.0,
            |contact| contact.desired_change,
            |entities, contact| contact.resolve_velocity(entities),
            |entities, contact, move_info, contact_relative|
            {
                let contact_change = Contact::velocity_from_angular(
                    move_info.angular_change,
                    contact_relative
                ) + move_info.velocity_change;

                let change = contact.to_world.transpose() * contact_change;

                if move_info.inverted
                {
                    contact.velocity -= change;
                } else
                {
                    contact.velocity += change;
                }

                contact.calculate_desired_change(entities, dt);
            }
        );
    }
}
