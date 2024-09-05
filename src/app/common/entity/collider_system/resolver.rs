use std::{
    cmp::Ordering,
    num::FpCategory
};

use nalgebra::{Matrix3, Vector3};

use crate::common::{
    cross_2d,
    Physical,
    Entity,
    entity::ClientEntities,
    collider::Contact
};


fn cross_3d(a: Vector3<f32>, b: Vector3<f32>) -> Vector3<f32>
{
    Vector3::new(
        cross_2d(a.yz(), b.yz()),
        cross_2d(a.zx(), b.zx()),
        cross_2d(a.xy(), b.xy())
    )
}

fn skew_symmetric(v: Vector3<f32>) -> Matrix3<f32>
{
    Matrix3::new(
        0.0, -v.z, v.y,
        v.z, 0.0, -v.x,
        -v.y, v.x, 0.0
    )
}

struct IterativeEpsilon
{
    pub sleep: f32,
    pub general: f32
}

const ANGULAR_LIMIT: f32 = 2.0;
const VELOCITY_LOW: f32 = 0.02;

const PENETRATION_EPSILON: IterativeEpsilon = IterativeEpsilon{sleep: 0.02, general: 0.001};
const VELOCITY_EPSILON: IterativeEpsilon = IterativeEpsilon{sleep: 0.02, general: 0.001};

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
    pub velocity_change: Vector3<f32>,
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
    pub velocity_change: Vector3<f32>,
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
    pub to_world: Matrix3<f32>,
    pub velocity: Vector3<f32>,
    pub desired_change: f32,
    pub a_inertia: f32,
    pub b_inertia: Option<f32>,
    pub a_relative: Vector3<f32>,
    pub b_relative: Option<Vector3<f32>>
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

    fn get_inertia(&self, which: WhichObject) -> f32
    {
        match which
        {
            WhichObject::A => self.a_inertia,
            WhichObject::B => self.b_inertia.unwrap()
        }
    }

    fn get_relative(&self, which: WhichObject) -> Vector3<f32>
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
            self.get_inertia(which),
            self.get_relative(which),
            self.contact.normal
        );

        Inertias{
            linear: entities.physical(self.get_entity(which)).unwrap().inverse_mass,
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
        let mut transform = entities.transform_mut(self.get_entity(which)).unwrap();

        let angular_projection = contact_relative
            + self.contact.normal * (-contact_relative).dot(&self.contact.normal);

        let angular_limit = ANGULAR_LIMIT * angular_projection.magnitude();

        if angular_amount.abs() > angular_limit
        {
            let pre_limit = angular_amount;

            angular_amount = angular_amount.clamp(-angular_limit, angular_limit);

            velocity_amount += pre_limit - angular_amount;
        }

        let velocity_change = velocity_amount * self.contact.normal;
        transform.position += velocity_change;

        let angular_change = if inertias.angular.classify() != FpCategory::Zero
        {
            let impulse_torque = cross_2d(
                contact_relative.xy(),
                self.contact.normal.xy()
            ) / self.get_inertia(which);

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
    ) -> (PenetrationMoves, Option<PenetrationMoves>)
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

        (a_moves, b_moves)
    }

    fn velocity_change(
        &self,
        entities: &ClientEntities,
        which: WhichObject
    ) -> Matrix3<f32>
    {
        let impulse_to_torque = skew_symmetric(self.get_relative(which));
        -((impulse_to_torque / self.get_inertia(which)) * impulse_to_torque)
    }

    fn apply_impulse(
        &self,
        entities: &ClientEntities,
        impulse: Vector3<f32>,
        which: WhichObject
    ) -> VelocityMoves
    {
        let contact_relative = self.get_relative(which);

        let impulse_torque = cross_2d(contact_relative.xy(), impulse.xy());

        let mut physical = entities.physical_mut(self.get_entity(which)).unwrap();

        let angular_change = impulse_torque / self.get_inertia(which);
        let velocity_change = impulse * physical.inverse_mass;

        physical.add_velocity_raw(velocity_change);
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
    ) -> (VelocityMoves, Option<VelocityMoves>)
    {
        let mut velocity_change = self.velocity_change(entities, WhichObject::A);

        if let Some(_) = self.contact.b
        {
            let b_velocity_change = self.velocity_change(entities, WhichObject::B);

            velocity_change += b_velocity_change;
        }

        let mut velocity_change = (self.to_world.transpose() * velocity_change) * self.to_world;

        let mut total_inverse_mass = entities.physical(self.contact.a).unwrap().inverse_mass;
        if let Some(b) = self.contact.b
        {
            total_inverse_mass += entities.physical(b).unwrap().inverse_mass;
        }

        let dims = 3;
        (0..dims).for_each(|i|
        {
            *velocity_change.index_mut((i, i)) += total_inverse_mass;
        });

        let impulse_local_matrix = velocity_change.try_inverse().unwrap();

        let desired_change = Vector3::new(
            self.desired_change,
            -self.velocity.y,
            -self.velocity.z
        );

        let mut impulse_local = impulse_local_matrix * desired_change;

        let plane_magnitude = (1..dims)
            .map(|i| impulse_local.index(i)).map(|x| x.powi(2))
            .sum::<f32>()
            .sqrt();

        let static_friction = self.contact.static_friction(entities);
        if plane_magnitude > impulse_local.x * static_friction
        {
            let friction = self.contact.dynamic_friction(entities);

            (1..dims).for_each(|i|
            {
                *impulse_local.index_mut(i) /= plane_magnitude;
            });

            // remove friction in other axes
            impulse_local.x = self.desired_change / (velocity_change.m11
                + velocity_change.m12 * friction * impulse_local.y
                + velocity_change.m13 * friction * impulse_local.z);

            (1..dims).for_each(|i|
            {
                *impulse_local.index_mut(i) *= friction * impulse_local.x;
            });
        }

        let impulse = self.to_world * impulse_local;

        let a_moves = self.apply_impulse(entities, impulse, WhichObject::A);

        let b_moves = self.contact.b.map(|_|
        {
            self.apply_impulse(entities, -impulse, WhichObject::B)
        });

        (a_moves, b_moves)
    }
}

impl Contact
{
    pub fn to_world_matrix(&self) -> Matrix3<f32>
    {
        let cosa = self.normal.x;
        let msina = self.normal.y;

        Matrix3::new(
            cosa, -msina, 0.0,
            msina, cosa, 0.0,
            0.0, 0.0, 1.0
        )
    }

    fn direction_apply_inertia(
        inertia: f32,
        direction: Vector3<f32>,
        normal: Vector3<f32>
    ) -> Vector3<f32>
    {
        let angular_inertia = cross_3d(
            direction,
            normal
        ) / inertia;

        cross_3d(
            angular_inertia,
            direction
        )
    }

    fn velocity_from_angular(angular: f32, contact_local: Vector3<f32>) -> Vector3<f32>
    {
        cross_3d(
            Vector3::new(0.0, 0.0, angular),
            contact_local
        )
    }

    fn velocity_closing(
        physical: &Physical,
        to_world: &Matrix3<f32>,
        contact_relative: Vector3<f32>
    ) -> Vector3<f32>
    {
        let relative_velocity = Self::velocity_from_angular(
            physical.angular_velocity(),
            contact_relative
        ) + physical.velocity();

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

    // this is not how friction works irl but i dont care
    fn dynamic_friction(&self, entities: &ClientEntities) -> f32
    {
        self.average_physical(entities, |x| x.dynamic_friction)
    }

    fn static_friction(&self, entities: &ClientEntities) -> f32
    {
        self.average_physical(entities, |x| x.static_friction)
    }

    fn calculate_desired_change(
        &self,
        entities: &ClientEntities,
        velocity_local: &Vector3<f32>,
        dt: f32
    ) -> f32
    {
        let mut acceleration_velocity = (entities.physical(self.a).unwrap().last_acceleration() * dt)
            .dot(&self.normal);

        if let Some(b) = self.b
        {
            acceleration_velocity -= (entities.physical(b).unwrap().last_acceleration() * dt)
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

    fn awaken(&self, entities: &ClientEntities)
    {
        if let Some(b) = self.b
        {
            if entities.physical(self.a).unwrap().sleeping() != entities.physical(b).unwrap().sleeping()
            {
                entities.physical_mut(self.a).unwrap().set_sleeping(false);
                entities.physical_mut(b).unwrap().set_sleeping(false);
            }
        }
    }

    fn inertia_of(entities: &ClientEntities, entity: Entity) -> f32
    {
        entities.collider(entity).unwrap().inertia(
            &entities.physical(entity).unwrap(),
            &entities.transform(entity).unwrap()
        )
    }

    fn analyze(self, entities: &ClientEntities, dt: f32) -> AnalyzedContact
    {
        let to_world = self.to_world_matrix();

        let a_relative = self.point - entities.transform(self.a).unwrap().position;
        let b_relative = self.b.map(|b| self.point - entities.transform(b).unwrap().position);

        let mut velocity = Self::velocity_closing(
            &entities.physical(self.a).unwrap(),
            &to_world,
            a_relative
        );

        let a_inertia = Self::inertia_of(entities, self.a);

        let b_inertia = self.b.map(|b|
        {
            let b_velocity = Self::velocity_closing(
                &entities.physical(b).unwrap(),
                &to_world,
                b_relative.unwrap()
            );

            velocity -= b_velocity;

            Self::inertia_of(entities, b)
        });

        let desired_change = self.calculate_desired_change(entities, &velocity, dt);

        AnalyzedContact{
            to_world,
            velocity,
            desired_change,
            a_inertia,
            b_inertia,
            a_relative,
            b_relative,
            contact: self
        }
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
        mut handle: impl FnMut(&ClientEntities, &mut AnalyzedContact, Moves, Vector3<f32>)
    )
    {
        let (a_move, b_move) = moves;
        let (a_id, b_id) = bodies;

        contacts.iter_mut().for_each(|x|
        {
            let point = x.contact.point;
            let relative = |entity: Entity|
            {
                point - entities.transform(entity).unwrap().position
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

            if this_contact_b.is_some() && this_contact_b == b_id
            {
                handle(b_move.unwrap().inverted(), relative(this_contact_b.unwrap()));
            }
        });
    }

    fn resolve_iterative<Moves: IteratedMoves + Copy>(
        entities: &ClientEntities,
        contacts: &mut [AnalyzedContact],
        iterations: usize,
        epsilon: IterativeEpsilon,
        compare: impl Fn(&AnalyzedContact) -> f32,
        mut resolver: impl FnMut(&ClientEntities, &mut AnalyzedContact) -> (Moves, Option<Moves>),
        mut updater: impl FnMut(&ClientEntities, &mut AnalyzedContact, Moves, Vector3<f32>)
    )
    {
        for _ in 0..iterations
        {
            if let Some((change, contact)) = contacts.iter_mut().map(|contact|
            {
                (compare(contact), contact)
            }).max_by(|(a, _), (b, _)|
            {
                a.partial_cmp(b).unwrap_or(Ordering::Less)
            }).filter(|(change, _contact)|
            {
                *change > epsilon.general
            })
            {
                if change > epsilon.sleep
                {
                    contact.contact.awaken(entities);
                }

                let moves = resolver(entities, contact);
                let bodies = (contact.contact.a, contact.contact.b);

                debug_assert!(moves.1.is_some() == contact.contact.b.is_some());

                Self::update_iterated::<Moves>(
                    entities,
                    contacts,
                    moves,
                    bodies,
                    &mut updater
                );
            } else
            {
                break;
            }
        }
    }

    pub fn resolve(
        entities: &ClientEntities,
        contacts: Vec<Contact>,
        dt: f32
    )
    {
        let mut analyzed_contacts: Vec<_> = contacts.into_iter().map(|contact|
        {
            contact.analyze(entities, dt)
        }).collect();

        let iterations = analyzed_contacts.len() * 2;
        Self::resolve_iterative(
            entities,
            &mut analyzed_contacts,
            iterations,
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
            iterations,
            VELOCITY_EPSILON,
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
