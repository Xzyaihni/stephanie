use std::sync::Arc;

use vulkano::{
	memory::allocator::StandardMemoryAllocator,
	sampler::Sampler,
	descriptor_set::{
		allocator::StandardDescriptorSetAllocator,
		layout::DescriptorSetLayout
	},
	command_buffer::{
		AutoCommandBufferBuilder,
		PrimaryAutoCommandBuffer
	}
};


pub struct DescriptorSetUploader
{
	pub allocator: StandardDescriptorSetAllocator,
	pub layout: Arc<DescriptorSetLayout>,
	pub sampler: Arc<Sampler>
}

pub struct ResourceUploader<'a>
{
	pub allocator: &'a StandardMemoryAllocator,
	pub builder: &'a mut AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>,
	pub descriptor: DescriptorSetUploader
}