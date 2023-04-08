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

        matrix *= Self::calculate_stretch_matrix(transform);

        matrix.prepend_translation_mut(&origin);

        matrix.prepend_nonuniform_scaling_mut(&transform.scale);
        matrix.append_translation_mut(&transform.position);

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

    fn calculate_stretch_matrix(transform: &Transform) -> Matrix4<f32>
    {
        let (s_x, s_y) = {
            let stretch = transform.stretch.1;

            (stretch.x, stretch.y)
        };

        let angle: f32 = 2.0 * transform.stretch.0;
        let (angle_sin, angle_cos) = (angle.sin(), angle.cos());

        let mut stretch_matrix = Matrix4::identity();
        stretch_matrix.m11 = (s_x + s_y + s_x * angle_cos - s_y * angle_cos) / 2.0;
        stretch_matrix.m12 = (-s_x * angle_sin + s_y * angle_sin) / 2.0;
        stretch_matrix.m21 = (-s_x * angle_sin + s_y * angle_sin) / 2.0;
        stretch_matrix.m22 = (s_x + s_y - s_x * angle_cos + s_y * angle_cos) / 2.0;

        stretch_matrix
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