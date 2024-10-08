use nalgebra::Vector3;

use yanyaengine::{
    Transform,
    TransformContainer,
    OccludingPlane as OccludingPlaneInner,
    game_object::*
};

use crate::{
    debug_config::*,
    client::{VisibilityChecker, RenderCreateInfo},
    common::ServerToClient
};


pub type OccludingPlaneServer = ();

pub struct OccludingCaster(Vector3<f32>);

impl From<Vector3<f32>> for OccludingCaster
{
    fn from(value: Vector3<f32>) -> Self
    {
        Self(value)
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

    pub fn visible(&self, visibility: &VisibilityChecker) -> bool
    {
        self.visible_with(visibility, self.0.transform_ref())
    }

    pub fn visible_with(&self, visibility: &VisibilityChecker, transform: &Transform) -> bool
    {
        visibility.visible_occluding_plane(transform)
    }

    pub fn update_buffers(
        &mut self,
        info: &mut UpdateBuffersInfo,
        caster: &OccludingCaster
    )
    {
        self.0.update_buffers(caster.0, info);
    }

    pub fn draw(
        &self,
        info: &mut DrawInfo
    )
    {
        if DebugConfig::is_enabled(DebugTool::NoOcclusion)
        {
            return;
        }

        self.0.draw(info);
    }
}
