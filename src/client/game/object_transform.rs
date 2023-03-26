use nalgebra::{
    Vector3,
    base::{
        Matrix4
    }
};

use crate::common::{Transform, OnTransformCallback, TransformContainer};


#[derive(Debug, Clone)]
pub struct ObjectTransform
{
    transform: Transform,
    origin: Vector3<f32>,
    matrix: Matrix4<f32>
}

#[allow(dead_code)]
impl ObjectTransform
{
    pub fn new_default() -> Self
    {
        let transform = Transform::new();

        Self::new_transformed(transform)
    }

    pub fn new_transformed(transform: Transform) -> Self
    {
        let origin = Vector3::zeros();

        Self::new(transform, origin)
    }

    pub fn new(transform: Transform, origin: Vector3<f32>) -> Self
    {
        let matrix = Self::calculate_matrix(&transform, &origin);

        Self{transform, origin, matrix}
    }

    pub fn recalculate_matrix(&mut self)
    {
        self.matrix = Self::calculate_matrix(&self.transform, &self.origin);
    }

    fn calculate_matrix(
        transform: &Transform,
        origin: &Vector3<f32>
    ) -> Matrix4<f32>
    {
        let mut matrix = Matrix4::from_axis_angle(&transform.rotation_axis, transform.rotation);

        matrix.append_translation_mut(&transform.position);
        matrix.append_translation_mut(origin);
        matrix.prepend_nonuniform_scaling_mut(&transform.scale);

        matrix
    }

    pub fn set_origin(&mut self, origin: Vector3<f32>)
    {
        self.origin = origin;
    }

    pub fn matrix(&self) -> Matrix4<f32>
    {
        self.matrix
    }
}

impl OnTransformCallback for ObjectTransform
{
    fn callback(&mut self)
    {
        self.recalculate_matrix();
    }
}

impl TransformContainer for ObjectTransform
{
    fn transform_ref(&self) -> &Transform
    {
        &self.transform
    }

    fn transform_mut(&mut self) -> &mut Transform
    {
        &mut self.transform
    }
}