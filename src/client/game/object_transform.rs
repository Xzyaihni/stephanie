use nalgebra::base::Matrix4;

use crate::common::{Transform, TransformContainer};


#[derive(Debug, Clone)]
pub struct ObjectTransform
{
    transform: Transform,
    matrix: Matrix4<f32>
}

impl ObjectTransform
{
    pub fn new() -> Self
    {
        let transform = Transform::new();

        let matrix = Self::calculate_matrix(&transform);

        Self{transform, matrix}
    }

    pub fn recalculate_matrix(&mut self)
    {
        self.matrix = Self::calculate_matrix(&self.transform);
    }

    fn calculate_matrix(
        transform: &Transform
    ) -> Matrix4<f32>
    {
        let mut matrix = Matrix4::from_axis_angle(&transform.rotation_axis, transform.rotation);

        matrix.append_translation_mut(&transform.position);
        matrix.prepend_nonuniform_scaling_mut(&transform.scale);

        matrix
    }

    pub fn matrix(&self) -> Matrix4<f32>
    {
        self.matrix
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

    fn callback(&mut self)
    {
        self.recalculate_matrix();
    }
}