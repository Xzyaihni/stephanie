use std::sync::Arc;

use vulkano::{
    format::{Format, FormatFeatures},
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


#[derive(Clone)]
pub struct ThisSetup
{
    supported_format: Format
}

pub fn create() -> Rendering<ThisSetup>
{
    Rendering{
        setup: Box::new(|physical_device|
        {
            let supported_format = [
                Format::D32_SFLOAT_S8_UINT,
                Format::D24_UNORM_S8_UINT,
                Format::D16_UNORM_S8_UINT
            ].into_iter().find(|format|
            {
                physical_device.format_properties(*format).unwrap()
                    .optimal_tiling_features
                    .intersects(FormatFeatures::DEPTH_STENCIL_ATTACHMENT)
            }).expect("depth/stencil format must exist!!");

            ThisSetup{supported_format}
        }),
        render_pass: Box::new(|setup, device, image_format|
        {
            vulkano::single_pass_renderpass!(
                device,
                attachments: {
                    color: {
                        format: image_format,
                        samples: 1,
                        load_op: Clear,
                        store_op: Store
                    },
                    depth: {
                        format: setup.supported_format,
                        samples: 1,
                        load_op: Clear,
                        store_op: DontCare
                    }
                },
                pass: {
                    color: [color],
                    depth_stencil: {depth}
                }
            ).unwrap()
        }),
        attachments: Box::new(|setup, allocator: Arc<StandardMemoryAllocator>, view: Arc<ImageView>|
        {
            let depth_stencil_image = Image::new(
                allocator,
                ImageCreateInfo{
                    image_type: ImageType::Dim2d,
                    format: setup.supported_format,
                    extent: view.image().extent(),
                    usage: ImageUsage::TRANSIENT_ATTACHMENT | ImageUsage::DEPTH_STENCIL_ATTACHMENT,
                    ..Default::default()
                },
                AllocationCreateInfo::default()
            ).unwrap();

            let depth_stencil = ImageView::new_default(depth_stencil_image).unwrap();

            vec![view, depth_stencil]
        }),
        clear: vec![Some([0.831, 0.941, 0.988, 1.0].into()), Some((1.0, 1).into())]
    }
}
