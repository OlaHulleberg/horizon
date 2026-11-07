//! GPU time of the entity stream for the model-copies bench (#808): CPU vertices uploaded whole
//! each frame, and copies drawn from the prepared buffer the way `Renderer::render` draws them.
use super::prepared_model::{
    EntityRangePass, PreparedModelDraw, PreparedModelInstance, PreparedModelVertex, PreparedModels,
    draw_ordered, fill_ordered_ranges, instance_slot_capacity,
};
use super::*;
use bytemuck::Zeroable;

const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// What one measured frame cost.
pub struct GpuFrame {
    /// The entity pass, from its timestamps.
    pub pass_ms: f64,
    /// Upload, pass and wait, on the CPU's clock.
    pub wall_ms: f64,
    /// Staging, upload, encoding and `queue.submit`, on the CPU's clock: the render thread's share.
    pub submit_ms: f64,
    /// Allocation calls on this thread over `submit_ms`.
    pub submit_allocs: u64,
}

/// One scene's timed frames and what its last frame did.
pub struct Measured {
    pub frames: Vec<GpuFrame>,
    /// Bytes written to the GPU in the last frame.
    pub uploaded_bytes: u64,
    /// Draw calls of the entity pass in the last frame.
    pub draws: u64,
    /// The last frame's colour target, tightly packed RGBA.
    pub pixels: Vec<u8>,
}

/// An offscreen 1920x1080 entity pass on the hardware adapter.
pub struct Bench {
    adapter: String,
    device: wgpu::Device,
    queue: wgpu::Queue,
    color: wgpu::Texture,
    depth: wgpu::Texture,
    bindings: wgpu::BindGroupLayout,
    group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    models: Option<PreparedModels>,
}

impl Bench {
    /// `None` without a hardware adapter that has timestamp queries.
    pub fn open(atlas_size: [u32; 2], atlas: &[u8], view_proj: Mat4) -> Option<Self> {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = wgpu::Backends::VULKAN;
        let instance = wgpu::Instance::new(desc);
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .ok()?;
        let info = adapter.get_info();
        if info.device_type == wgpu::DeviceType::Cpu
            || !adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY)
        {
            return None;
        }
        let name = format!(
            "{} ({:?}, {} {})",
            info.name, info.backend, info.driver, info.driver_info
        );
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_features: wgpu::Features::TIMESTAMP_QUERY,
            ..Default::default()
        }))
        .ok()?;
        let texture = |width, height, format, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let format = FORMAT;
        let color = texture(
            WIDTH,
            HEIGHT,
            format,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        );
        let depth = texture(
            WIDTH,
            HEIGHT,
            wgpu::TextureFormat::Depth32Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let atlas_texture = texture(
            atlas_size[0],
            atlas_size[1],
            format,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        );
        queue.write_texture(
            atlas_texture.as_image_copy(),
            atlas,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(atlas_size[0] * 4),
                rows_per_image: None,
            },
            atlas_texture.size(),
        );
        let lightmap = texture(
            16,
            16,
            format,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        );
        queue.write_texture(
            lightmap.as_image_copy(),
            &[128, 192, 255, 255].repeat(16 * 16),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(64),
                rows_per_image: None,
            },
            lightmap.size(),
        );
        let sampler = device.create_sampler(&Default::default());
        let mut globals = Globals::zeroed();
        globals.view_proj = view_proj.to_cols_array_2d();
        globals.fog = [1000.0, 2000.0, 1000.0, 2000.0];
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::bytes_of(&globals),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(entity_shader().into()),
        });
        let bindings = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &entity_binding_entries(),
        });
        let atlas_view = atlas_texture.create_view(&Default::default());
        let light_view = lightmap.create_view(&Default::default());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bindings,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&light_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&bindings)],
            immediate_size: 0,
        });
        let buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ENTITY_ATTRIBUTES,
        })];
        let reuse = reuse::Reuse::default();
        let kept = reuse.on(&device);
        let pipeline = entity_pipeline_factory(&kept, &layout, &layout, &shader, &buffers, format)(
            EntityPipelineOptions {
                label: "Entities",
                cull_mode: None,
                depth_write: true,
                compare: wgpu::CompareFunction::GreaterEqual,
                bias: wgpu::DepthBiasState {
                    constant: 128,
                    slope_scale: 0.0,
                    clamp: 0.0,
                },
                blend: wgpu::BlendState::ALPHA_BLENDING,
                clouds: false,
                vertex: "vs_entity",
                fragment: "fs_entity",
                write_mask: wgpu::ColorWrites::ALL,
            },
        );
        drop(kept);
        Some(Self {
            adapter: name,
            device,
            queue,
            color,
            depth,
            bindings,
            group,
            pipeline,
            models: None,
        })
    }

    /// The adapter and driver.
    pub fn adapter(&self) -> &str {
        &self.adapter
    }

    /// `Renderer::install_prepared_models`.
    pub fn install_prepared_models(
        &mut self,
        generation: u64,
        vertices: &[PreparedModelVertex],
        ranges: Vec<Option<(u32, u32)>>,
    ) -> Result<()> {
        self.models = Some(PreparedModels::create(
            &self.device,
            &reuse::Reuse::default(),
            &self.bindings,
            FORMAT,
            generation,
            vertices,
            ranges,
        )?);
        Ok(())
    }

    /// `Renderer::prepared_generation`.
    pub fn prepared_generation(&self) -> Option<u64> {
        self.models.as_ref().map(PreparedModels::generation)
    }

    /// `Renderer::prepared_instances_fit`.
    pub fn prepared_instances_fit(&self, count: u64) -> bool {
        instance_slot_capacity(
            count,
            std::mem::size_of::<PreparedModelInstance>() as u64,
            self.device.limits().max_buffer_size,
        )
        .is_ok()
    }

    /// Times `frames` draws of the entity stream `vertices` and the prepared copies `draws`.
    /// With no copies this is the one indexed draw `Renderer::render` makes; with copies it stages,
    /// orders and draws them as it does. `allocations` reads this thread's allocation count.
    pub fn time_frames(
        &self,
        vertices: &[Vertex],
        draws: &[PreparedModelDraw],
        warmup: usize,
        frames: usize,
        allocations: fn() -> u64,
    ) -> Measured {
        let (device, queue) = (&self.device, &self.queue);
        let bytes: &[u8] = bytemuck::cast_slice(vertices);
        let vertex = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (bytes.len() as u64).max(4),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let entity_quads = vertices.len() as u32 / 4;
        // `Renderer::indices` is never empty; a stream of copies alone still binds an index buffer.
        let indices: Vec<u32> = (0..entity_quads.max(1))
            .flat_map(|i| [4 * i, 4 * i + 1, 4 * i + 2, 4 * i, 4 * i + 2, 4 * i + 3])
            .collect();
        let index = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let stride = std::mem::size_of::<PreparedModelInstance>() as u64;
        let slots =
            instance_slot_capacity(draws.len() as u64, stride, device.limits().max_buffer_size)
                .expect("the copies fit one instance buffer")
                .max(1);
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Prepared model instances"),
            size: slots * stride,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let queries = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: None,
            ty: wgpu::QueryType::Timestamp,
            count: 2,
        });
        let resolve = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 16,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 16,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let period = f64::from(queue.get_timestamp_period());
        let color_view = self.color.create_view(&Default::default());
        let depth_view = self.depth.create_view(&Default::default());
        let mut staged: Vec<PreparedModelDraw> = Vec::new();
        let mut uploaded: Vec<PreparedModelInstance> = Vec::new();
        let mut ranges: Vec<(u32, u32, Option<usize>)> = Vec::new();
        let mut uploaded_bytes = 0;
        let mut out = Vec::with_capacity(frames);
        for frame in 0..warmup + frames {
            let start = Instant::now();
            let allocs_before = allocations();
            uploaded_bytes = 0;
            if !bytes.is_empty() {
                queue.write_buffer(&vertex, 0, bytes);
                uploaded_bytes += bytes.len() as u64;
            }
            // `Renderer::stage_prepared_draws`, then the instance upload and cuts of `render`.
            staged.clear();
            staged.extend_from_slice(draws);
            ranges.clear();
            if !staged.is_empty() {
                uploaded.clear();
                uploaded.extend(staged.iter().map(|draw| draw.instance));
                let instance_bytes: &[u8] = bytemuck::cast_slice(&uploaded);
                queue.write_buffer(&instances, 0, instance_bytes);
                uploaded_bytes += instance_bytes.len() as u64;
                fill_ordered_ranges(
                    entity_quads * 4,
                    staged.iter().map(|draw| {
                        entity_batch::ordinary_vertices_before(vertices, draw.cpu_vertices, false)
                    }),
                    &mut ranges,
                );
            }
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &color_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(0.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: Some(wgpu::RenderPassTimestampWrites {
                        query_set: &queries,
                        beginning_of_pass_write_index: Some(0),
                        end_of_pass_write_index: Some(1),
                    }),
                    ..Default::default()
                });
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.group, &[]);
                pass.set_vertex_buffer(0, vertex.slice(..));
                pass.set_index_buffer(index.slice(..), wgpu::IndexFormat::Uint32);
                if staged.is_empty() {
                    if entity_quads > 0 {
                        pass.draw_indexed(0..entity_quads * 6, 0, 0..1);
                    }
                } else {
                    draw_ordered(
                        &mut pass,
                        &EntityRangePass {
                            pipeline: &self.pipeline,
                            // The bench draws with HDR off, as its baseline did.
                            hdr_pipeline: &self.pipeline,
                            vertices: &vertex,
                            indices: &index,
                            hdr: false,
                        },
                        self.models.as_ref(),
                        &instances,
                        &staged,
                        &ranges,
                    );
                }
            }
            encoder.resolve_query_set(&queries, 0..2, &resolve, 0);
            encoder.copy_buffer_to_buffer(&resolve, 0, &readback, 0, 16);
            queue.submit([encoder.finish()]);
            let submit_ms = start.elapsed().as_secs_f64() * 1e3;
            let submit_allocs = allocations() - allocs_before;
            readback.slice(..).map_async(wgpu::MapMode::Read, |_| {});
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            let wall_ms = start.elapsed().as_secs_f64() * 1e3;
            let stamps: [u64; 2] =
                bytemuck::pod_read_unaligned(&readback.slice(..).get_mapped_range().unwrap());
            readback.unmap();
            if frame >= warmup {
                out.push(GpuFrame {
                    pass_ms: stamps[1].saturating_sub(stamps[0]) as f64 * period / 1e6,
                    wall_ms,
                    submit_ms,
                    submit_allocs,
                });
            }
        }
        Measured {
            frames: out,
            uploaded_bytes,
            draws: self.draws_made(entity_quads, &staged, &ranges),
            pixels: self.read_color(),
        }
    }

    /// The draws `draw_ordered` makes: it skips a copy whose geometry it can't find without a word,
    /// so a stale generation shows up here as fewer draws rather than a faster frame.
    fn draws_made(
        &self,
        entity_quads: u32,
        staged: &[PreparedModelDraw],
        ranges: &[(u32, u32, Option<usize>)],
    ) -> u64 {
        if staged.is_empty() {
            return u64::from(entity_quads > 0);
        }
        ranges
            .iter()
            .map(|&(start, end, draw)| {
                let copy = draw.is_some_and(|index| {
                    self.models.as_ref().is_some_and(|models| {
                        models
                            .indices(staged[index].generation, staged[index].slot)
                            .is_ok()
                    })
                });
                u64::from(end / 4 > start / 4) + u64::from(copy)
            })
            .sum()
    }

    fn read_color(&self) -> Vec<u8> {
        let (device, queue) = (&self.device, &self.queue);
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(WIDTH * HEIGHT * 4),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            self.color.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(WIDTH * 4),
                    rows_per_image: None,
                },
            },
            self.color.size(),
        );
        queue.submit([encoder.finish()]);
        readback.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let pixels = readback.slice(..).get_mapped_range().unwrap().to_vec();
        pixels
    }
}
