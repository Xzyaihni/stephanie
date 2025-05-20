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

use crate::BACKGROUND_COLOR;


pub fn create() -> Rendering<()>
{
    Rendering{
        setup: Box::new(|_| {}),
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
                        format: Format::R8G8B8A8_SRGB,
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

            let normal_image = ||
            {
                ImageView::new_default(Image::new(
                    allocator.clone(),
                    ImageCreateInfo{
                        image_type: ImageType::Dim2d,
                        format: Format::R8G8B8A8_SRGB,
                        extent: view.image().extent(),
                        usage: ImageUsage::COLOR_ATTACHMENT | ImageUsage::TRANSIENT_ATTACHMENT | ImageUsage::INPUT_ATTACHMENT,
                        ..Default::default()
                    },
                    AllocationCreateInfo::default()
                ).unwrap()).unwrap()
            };

            let color = normal_image();
            let shade = normal_image();
            let lighting = normal_image();

            vec![color, depth, shade, shade_depth, lighting, view]
        }),
        clear: vec![
            Some([BACKGROUND_COLOR.x, BACKGROUND_COLOR.y, BACKGROUND_COLOR.z, 1.0].into()),
            Some(1.0.into()),
            Some([BACKGROUND_COLOR.x, BACKGROUND_COLOR.y, BACKGROUND_COLOR.z, 1.0].into()),
            Some(1.0.into()),
            Some([1.0, 1.0, 1.0, 0.0].into()),
            None
        ]
    }
}
