use std::sync::Arc;

use vulkano::{
    format::Format,
    memory::allocator::{
        StandardMemoryAllocator,
        AllocationCreateInfo
    },
    image::{
        Image,
        ImageCreateInfo,
        ImageType,
        ImageUsage,
        view::ImageView
    }
};

use yanyaengine::Rendering;

use super::shaders::{DARKEN, SHADOW_COLOR};
use stephanie::{BACKGROUND_COLOR, app::{App, TimestampQuery}, common::lerp};


pub fn create() -> Rendering<App, TimestampQuery>
{
    Rendering{
        setup: Box::new(|device|
        {
            TimestampQuery::from(&device)
        }),
        render_pass: Box::new(|_setup, device, image_format|
        {
            vulkano::ordered_passes_renderpass!(
                device,
                attachments: {
                    color: {
                        format: Format::R8G8B8A8_SRGB,
                        samples: 1,
                        load_op: Clear,
                        store_op: DontCare
                    },
                    depth: {
                        format: Format::D16_UNORM,
                        samples: 1,
                        load_op: Clear,
                        store_op: DontCare
                    },
                    shade: {
                        format: Format::R8G8B8A8_SRGB,
                        samples: 1,
                        load_op: Clear,
                        store_op: DontCare
                    },
                    shade_depth: {
                        format: Format::D16_UNORM,
                        samples: 1,
                        load_op: Clear,
                        store_op: DontCare
                    },
                    lighting: {
                        format: Format::R8G8B8A8_UNORM,
                        samples: 1,
                        load_op: Clear,
                        store_op: DontCare
                    },
                    output: {
                        format: image_format,
                        samples: 1,
                        load_op: DontCare,
                        store_op: Store
                    }
                },
                passes: [
                    {
                        color: [color],
                        depth_stencil: {depth},
                        input: []
                    },
                    {
                        color: [shade],
                        depth_stencil: {shade_depth},
                        input: []
                    },
                    {
                        color: [lighting],
                        depth_stencil: {depth},
                        input: []
                    },
                    {
                        color: [output],
                        depth_stencil: {},
                        input: [color, shade, lighting]
                    },
                    {
                        color: [output],
                        depth_stencil: {},
                        input: []
                    }
                ]
            ).unwrap()
        }),
        attachments: Box::new(|_setup, allocator: Arc<StandardMemoryAllocator>, view: Arc<ImageView>|
        {
            let create_depth = ||
            {
                ImageView::new_default(Image::new(
                    allocator.clone(),
                    ImageCreateInfo{
                        image_type: ImageType::Dim2d,
                        format: Format::D16_UNORM,
                        extent: view.image().extent(),
                        usage: ImageUsage::TRANSIENT_ATTACHMENT | ImageUsage::DEPTH_STENCIL_ATTACHMENT,
                        ..Default::default()
                    },
                    AllocationCreateInfo::default()
                ).unwrap()).unwrap()
            };

            let depth = create_depth();
            let shade_depth = create_depth();

            let normal_image = |format|
            {
                ImageView::new_default(Image::new(
                    allocator.clone(),
                    ImageCreateInfo{
                        image_type: ImageType::Dim2d,
                        format,
                        extent: view.image().extent(),
                        usage: ImageUsage::COLOR_ATTACHMENT | ImageUsage::TRANSIENT_ATTACHMENT | ImageUsage::INPUT_ATTACHMENT,
                        ..Default::default()
                    },
                    AllocationCreateInfo::default()
                ).unwrap()).unwrap()
            };

            let color = normal_image(Format::R8G8B8A8_SRGB);
            let shade = normal_image(Format::R8G8B8A8_SRGB);
            let lighting = normal_image(Format::R8G8B8A8_UNORM);

            vec![color, depth, shade, shade_depth, lighting, view]
        }),
        clear: Box::new(|app|
        {
            let light = app.client().with_game_state(|x| x.world.sky_light()).unwrap_or_default();
            let sky_light = light.light_color();

            let darksky = BACKGROUND_COLOR.zip_map(&SHADOW_COLOR, |a, b| lerp(a, b, DARKEN));
            vec![
                Some([BACKGROUND_COLOR.x, BACKGROUND_COLOR.y, BACKGROUND_COLOR.z, 1.0].into()),
                Some(1.0.into()),
                Some([darksky.x, darksky.y, darksky.z, 1.0].into()),
                Some(1.0.into()),
                Some([sky_light[0], sky_light[1], sky_light[2], 1.0].into()),
                None
            ]
        })
    }
}
