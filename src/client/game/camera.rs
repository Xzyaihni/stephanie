use std::{
    f32
};

use nalgebra::{
    geometry::Orthographic3,
    Vector3,
    Matrix4
};

use super::object_transform::ObjectTransform;

use crate::common::{Transform, OnTransformCallback, TransformContainer};


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
        let size = Self::aspect_size(aspect);
        let projection = Self::create_projection(size);

        let view = ObjectTransform::new(Transform::new(), Self::origin(size));

        let projection_view = Self::create_projection_view(projection, view.matrix());

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

    fn create_projection(size: (f32, f32)) -> Matrix4<f32>
    {
        let identity = Matrix4::identity();
        let mut projection = Orthographic3::from_matrix_unchecked(identity);

        projection.set_left_and_right(0.0, size.0);
        projection.set_bottom_and_top(0.0, size.1);

        projection.to_homogeneous()
    }

    fn recreate_projection(&mut self, size: (f32, f32))
    {
        self.size = size;
        self.view.set_origin(Self::origin(size));

        self.projection = Self::create_projection(size);

        self.regenerate_projection_view();
    }

    pub fn regenerate_projection_view(&mut self)
    {
        self.projection_view =
            Self::create_projection_view(self.projection, self.view.matrix());
    }

    pub fn create_projection_view(projection: Matrix4<f32>, view: Matrix4<f32>) -> Matrix4<f32>
    {
        projection * view
    }

    pub fn projection_view(&self) -> Matrix4<f32>
    {
        self.projection_view
    }

    pub fn resize(&mut self, aspect: f32)
    {
        //this one just changes the aspect ratio
        self.recreate_projection(Self::aspect_size(aspect));
    }

    pub fn rescale(&mut self, scale: f32)
    {
        //this one actually scales the view
        let size = self.normalized_aspect();
        self.recreate_projection((size.0 * scale, size.1 * scale));
    }

    fn origin(size: (f32, f32)) -> Vector3<f32>
    {
        Vector3::new(size.0 / 2.0, size.1 / 2.0, 0.0)
    }

    pub fn aspect(&self) -> (f32, f32)
    {
        self.size
    }

    pub fn normalized_aspect(&self) -> (f32, f32)
    {
        let lowest = self.size.0.min(self.size.1);
        let normalized = (self.size.0 / lowest, self.size.1 / lowest);

        normalized
    }
}

impl OnTransformCallback for Camera
{
    fn callback(&mut self)
    {
        self.view.callback();
        self.regenerate_projection_view();
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
        self.position_callback(position);
    }

    fn set_scale(&mut self, scale: Vector3<f32>)
    {
        self.transform_mut().scale = -scale;
        self.scale_callback(scale);
    }

    fn set_rotation(&mut self, rotation: f32)
    {
        self.transform_mut().rotation = -rotation;
        self.rotation_callback(rotation);
    }
}