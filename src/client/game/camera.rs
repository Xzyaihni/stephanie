use std::{
    f32
};

use nalgebra::{
    geometry::Orthographic3,
    base::{
        Vector3,
        Matrix4
    }
};

use super::object_transform::ObjectTransform;

use crate::common::{Transform, TransformContainer};


#[derive(Debug, Clone)]
pub struct Camera
{
    projection: Matrix4<f32>,
    view: ObjectTransform,
    projection_view: Matrix4<f32>,
    size: (f32, f32)
}

impl Camera
{
    pub fn new(aspect: f32) -> Self
    {
        let projection = Self::create_projection(aspect);
        let view = ObjectTransform::new();

        let projection_view = Self::calculate_projection_view(projection, view.matrix());

        let size = Self::aspect_size(aspect);

        Self{projection, view, projection_view, size}
    }

    fn aspect_size(aspect: f32) -> (f32, f32)
    {
        if aspect < 1.0
        {
            (1.0, 1.0 + (1.0 - aspect))
        } else
        {
            (aspect, 1.0)
        }
    }

    fn create_projection(aspect: f32) -> Matrix4<f32>
    {
        let (width, height) = Self::aspect_size(aspect);

        let identity = Matrix4::identity();
        let mut projection = Orthographic3::from_matrix_unchecked(identity);

        projection.set_left_and_right(0.0, width);
        projection.set_bottom_and_top(0.0, height);

        projection.to_homogeneous()
    }

    pub fn regenerate_projection_view(&mut self)
    {
        self.projection_view =
            Self::calculate_projection_view(self.projection, self.view.matrix());
    }

    pub fn calculate_projection_view(projection: Matrix4<f32>, view: Matrix4<f32>) -> Matrix4<f32>
    {
        projection * view
    }

    pub fn projection_view(&self) -> Matrix4<f32>
    {
        self.projection_view
    }

    pub fn resize(&mut self, aspect: f32)
    {
        self.projection = Self::create_projection(aspect);
        self.size = Self::aspect_size(aspect);

        self.regenerate_projection_view();
    }

    pub fn origin(&self) -> Vector3<f32>
    {
        Vector3::new(self.size.0 / 2.0, self.size.1 / 2.0, 0.0)
    }
}

impl TransformContainer for Camera
{
    fn transform_ref(&self) -> &Transform
    {
        self.view.transform_ref()
    }

    fn transform_mut(&mut self) -> &mut Transform
    {
        self.view.transform_mut()
    }

    fn set_position(&mut self, position: Vector3<f32>)
    {
        self.transform_mut().position = -position;
        self.callback();
    }

    fn translate(&mut self, position: Vector3<f32>)
    {
        self.transform_mut().position -= position;
        self.callback();
    }

    fn set_scale(&mut self, scale: Vector3<f32>)
    {
        self.transform_mut().scale = -scale;
        self.callback();
    }

    fn grow(&mut self, scale: Vector3<f32>)
    {
        self.transform_mut().scale -= scale;
        self.callback();
    }

    fn set_rotation(&mut self, rotation: f32)
    {
        self.transform_mut().rotation = -rotation;
        self.callback();
    }

    fn rotate(&mut self, radians: f32)
    {
        self.transform_mut().rotation -= radians;
        self.callback();
    }

    fn callback(&mut self)
    {
        self.view.callback();
        self.regenerate_projection_view();
    }
}