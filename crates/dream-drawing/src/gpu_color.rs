use crate::effects::Effect;

pub struct ColorFilter {
    pipeline: wgpu::RenderPipeline,
}
impl ColorFilter {
    pub fn new(device: &wgpu::Device) -> Self {
        Self::pipeline(device, wgpu::TextureFormat::Rgba8Unorm, "fragment")
    }

    #[cfg(windows)]
    pub(super) fn copy(device: &wgpu::Device) -> Self {
        Self::pipeline(device, wgpu::TextureFormat::Bgra8Unorm, "copy")
    }

    fn pipeline(device: &wgpu::Device, format: wgpu::TextureFormat, fragment: &str) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Dream color effects"),
            source: wgpu::ShaderSource::Wgsl(include_str!("color_filter.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Dream color effects"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some(fragment),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        Self { pipeline }
    }
    pub fn encode(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        input: &wgpu::Texture,
        output: &wgpu::Texture,
        parameters: &wgpu::Buffer,
        effect: &Effect,
    ) -> wgpu::CommandBuffer {
        let mut encoder = device.create_command_encoder(&Default::default());
        let bytes = effect
            .parameters()
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>();
        queue.write_buffer(parameters, 0, &bytes);
        let source = input.create_view(&Default::default());
        let target = output.create_view(&Default::default());
        let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Dream color effect input"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: parameters.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Dream color adjustment"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &bindings, &[]);
        pass.draw(0..3, 0..1);
        drop(pass);
        encoder.finish()
    }
}
