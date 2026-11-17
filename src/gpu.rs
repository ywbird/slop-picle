//! Headless wgpu rendering (spec/evaluation.md#gpu).

use crate::surface::Surface;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pub adapter_name: String,
    pipelines: Mutex<HashMap<String, Arc<Pipeline>>>,
}

struct Pipeline {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
}

static GPU: OnceLock<Result<Gpu, String>> = OnceLock::new();

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

const PASSTHROUGH: &str = "
@group(0) @binding(0) var<uniform> p: vec4<u32>;
@group(0) @binding(1) var t: texture_2d<f32>;
fn shade(px: vec2<i32>) -> vec4<f32> { return textureLoad(t, px, 0); }
";

/// Vertex stage and fragment entry shared by every shader node.
pub const ENTRY: &str = "
@vertex fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    return shade(vec2<i32>(pos.xy));
}
";

impl Gpu {
    /// The process-wide GPU, opened on first use. `SLOPC_ADAPTER` picks an adapter by name substring.
    pub fn get() -> Result<&'static Gpu, String> {
        GPU.get_or_init(open).as_ref().map_err(Clone::clone)
    }

    /// Render `shader` (a WGSL module defining `shade`, bindings at group 0: uniform at 0, textures from 1).
    pub fn render(&self, key: &str, shader: &str, uniforms: &[u8], textures: &[&Surface], width: u32, height: u32) -> Surface {
        let pipe = self.pipeline(key, shader, textures.len());
        let mut ubytes = uniforms.to_vec();
        ubytes.resize(ubytes.len().max(16).next_multiple_of(16), 0);
        let ubuf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: ubytes.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&ubuf, 0, &ubytes);
        let views: Vec<wgpu::TextureView> = textures.iter().map(|s| self.upload(s).create_view(&Default::default())).collect();
        let mut entries = vec![wgpu::BindGroupEntry { binding: 0, resource: ubuf.as_entire_binding() }];
        for (i, v) in views.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry { binding: 1 + i as u32, resource: wgpu::BindingResource::TextureView(v) });
        }
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &pipe.layout, entries: &entries });

        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&pipe.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
        }
        let padded = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (padded * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        enc.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(height) },
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        self.queue.submit([enc.finish()]);
        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |r| r.expect("map readback"));
        self.device.poll(wgpu::PollType::wait_indefinitely()).expect("gpu poll");
        let mapped = slice.get_mapped_range().expect("mapped range");
        let mut data = Vec::with_capacity((width * height * 4) as usize);
        for row in mapped.chunks(padded as usize).take(height as usize) {
            data.extend_from_slice(&row[..(width * 4) as usize]);
        }
        drop(mapped);
        readback.unmap();
        Surface { width, height, data }
    }

    /// Upload and read back unchanged (GPU-03).
    pub fn passthrough(&self, s: &Surface) -> Surface {
        let src = format!("{PASSTHROUGH}{ENTRY}");
        self.render("__passthrough", &src, &[0; 16], &[s], s.width, s.height)
    }

    fn upload(&self, s: &Surface) -> wgpu::Texture {
        let size = wgpu::Extent3d { width: s.width, height: s.height, depth_or_array_layers: 1 };
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            tex.as_image_copy(),
            &s.data,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(s.width * 4), rows_per_image: Some(s.height) },
            size,
        );
        tex
    }

    fn pipeline(&self, key: &str, shader: &str, n_textures: usize) -> Arc<Pipeline> {
        let mut cache = self.pipelines.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(p) = cache.get(key) {
            return p.clone();
        }
        let module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(key),
            source: wgpu::ShaderSource::Wgsl(shader.into()),
        });
        let mut entries = vec![wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        }];
        for i in 0..n_textures {
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: 1 + i as u32,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
        }
        let layout = self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &entries });
        let pl = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(key),
            layout: Some(&pl),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs_main"), buffers: &[], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState { format: FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL })],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let p = Arc::new(Pipeline { pipeline, layout });
        cache.insert(key.into(), p.clone());
        p
    }
}

fn open() -> Result<Gpu, String> {
    let backends = wgpu::Backends::VULKAN | wgpu::Backends::METAL | wgpu::Backends::DX12;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends, ..wgpu::InstanceDescriptor::new_without_display_handle() });
    let adapter = match std::env::var("SLOPC_ADAPTER") {
        Ok(want) => pollster::block_on(instance.enumerate_adapters(backends))
            .into_iter()
            .find(|a| a.get_info().name.contains(&want))
            .ok_or_else(|| format!("no GPU adapter whose name contains '{want}'"))?,
        Err(_) => pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .map_err(|e| format!("no GPU adapter: {e}"))?,
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).map_err(|e| format!("cannot open GPU device: {e}"))?;
    Ok(Gpu { device, queue, adapter_name: adapter.get_info().name, pipelines: Mutex::new(HashMap::new()) })
}
