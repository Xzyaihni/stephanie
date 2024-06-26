use nalgebra::Vector3;

use yanyaengine::{
    Transform,
    TransformContainer,
    OccludingPlane as OccludingPlaneInner,
    game_object::*
};

use crate::{
    client::{VisibilityChecker, RenderCreateInfo},
    common::ServerToClient
};


pub type OccludingPlaneServer = ();

pub struct OccludingCasters(Vec<Vector3<f32>>);

impl From<Vec<Vector3<f32>>> for OccludingCasters
{
    fn from(values: Vec<Vector3<f32>>) -> Self
    {
        Self(values)
    }
}

#[derive(Debug)]
pub struct OccludingPlane(OccludingPlaneInner);

impl ServerToClient<OccludingPlane> for OccludingPlaneServer
{
    fn server_to_client(
        self,
        transform: impl FnOnce() -> Transform,
        create_info: &mut RenderCreateInfo
    ) -> OccludingPlane
    {
        let inner = create_info.object_info.partial.object_factory.create_occluding(transform());

        OccludingPlane::new(inner)
    }
}

impl OccludingPlane
{
    pub fn new(inner: OccludingPlaneInner) -> Self
    {
        OccludingPlane(inner)
    }

    pub fn set_transform(&mut self, transform: Transform)
    {
        self.0.set_transform(transform);
    }

    fn visible(&self, _visibility: &VisibilityChecker) -> bool
    {
        true
    }

    pub fn update_buffers(
        &mut self,
        visibility: &VisibilityChecker,
        info: &mut UpdateBuffersInfo,
        casters: &OccludingCasters
    )
    {
        if !self.visible(visibility)
        {
            return;
        }

        if let Some(&caster) = casters.0.get(0)
        {
            self.0.update_buffers(caster, info);
        }
    }

    pub fn draw(
        &self,
        visibility: &VisibilityChecker,
        info: &mut DrawInfo
    )
    {
        if !self.visible(visibility)
        {
            return;
        }

        self.0.draw(info);
    }
}
