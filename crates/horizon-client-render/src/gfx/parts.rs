//! Animated part models for native mods (docs/native-mods.md, hook points API 42): a mod's
//! machine parts (shafts, gears) drawn as instances of a model baked once, turning on the GPU.
//!
//! A model's vertices (the entity vertex, in block units, 0..1 inside its block) go to the GPU
//! once. Each instance is a block position, a rotation axis, a speed in RPM, a starting angle and
//! the light at the block; a model's instance buffer is written again only when one of its
//! instances changes (`PartScene::revision`). The angle is `phase + rpm * 6 * seconds`, worked out
//! by the vertex shader from the frame clock, so turning parts cost nothing on the CPU and nothing
//! is uploaded while they turn. One draw per model per frame.
use crate::{gfx::upload::Uploads, mesher::Vertex};
use glam::DVec3;
use std::sync::Arc;
use wgpu::util::DeviceExt;

/// One part: where it is and how it turns.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PartInstance {
    /// The block it turns in.
    pub block: [i32; 3],
    /// The rotation axis (0 x, 1 y, 2 z), revolutions per minute (signed: positive turns
    /// counterclockwise looking down the axis), the angle at time 0 in degrees, and 0.
    pub spin: [f32; 4],
    /// Moved from the block by `offset` (blocks), and from `offset[3]` seconds of the animation
    /// clock on by `velocity` (blocks a second): an item travelling along a belt moves on the
    /// GPU, and is set again only when it turns or stops.
    pub offset: [f32; 4],
    pub velocity: [f32; 4],
    /// The light at the block, as the entity vertex packs it: sky * 16 in the low byte,
    /// block * 16 in the next.
    pub light: u32,
}

/// The parts the game keeps: each model's vertices and its instances. A model's index is its
/// handle; `revision[model]` changes whenever its instances do, and the renderer uploads only
/// then.
#[derive(Default)]
pub struct PartScene {
    pub models: Vec<Arc<[Vertex]>>,
    pub instances: Vec<Vec<PartInstance>>,
    pub revision: Vec<u64>,
    /// Changes when a model's vertices do (`clear_model`): the renderer builds them again.
    pub shapes: Vec<u64>,
    /// Seconds of the animation clock: it stops when the game pauses, as the ticks do.
    pub seconds: f32,
}

impl PartScene {
    /// Adds a model, returning its handle.
    pub fn add_model(&mut self, vertices: Vec<Vertex>) -> u32 {
        self.models.push(vertices.into());
        self.instances.push(Vec::new());
        self.revision.push(0);
        self.shapes.push(0);
        (self.models.len() - 1) as u32
    }
    /// Empties a model and its instances (a mod's structure that is gone); its handle stays
    /// taken.
    pub fn clear_model(&mut self, model: u32) {
        let i = model as usize;
        if i >= self.models.len() {
            return;
        }
        self.models[i] = Arc::from(Vec::new());
        self.instances[i].clear();
        self.revision[i] += 1;
        self.shapes[i] += 1;
    }
    /// The quads of the largest model with instances, for the shared quad index buffer.
    pub fn max_quads(&self) -> u32 {
        self.models
            .iter()
            .zip(&self.instances)
            .filter(|(_, i)| !i.is_empty())
            .map(|(m, _)| (m.len() / 4) as u32)
            .max()
            .unwrap_or(0)
    }
}

/// The frame's uniform: the camera's block and the fraction past it (so instance positions stay
/// exact integers), and the animation clock in `fraction.w`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct PartFrame {
    pub(super) block: [i32; 4],
    pub(super) fraction: [f32; 4],
}

impl PartFrame {
    pub(super) fn new(camera: DVec3, seconds: f32) -> Self {
        let block = camera.floor();
        let fraction = (camera - block).as_vec3();
        Self {
            block: [block.x as i32, block.y as i32, block.z as i32, 0],
            fraction: [fraction.x, fraction.y, fraction.z, seconds],
        }
    }
}

struct Model {
    vertices: wgpu::Buffer,
    shape: u64,
    quads: u32,
    instances: Option<wgpu::Buffer>,
    capacity: usize,
    count: u32,
    revision: Option<u64>,
}

pub(super) struct Renderer {
    models: Vec<Model>,
    pub(super) uniform: wgpu::Buffer,
    pub(super) group: wgpu::BindGroup,
}

impl Renderer {
    pub(super) fn new(device: &wgpu::Device, layout: &wgpu::BindGroupLayout) -> Self {
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Part frame"),
            size: std::mem::size_of::<PartFrame>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Part frame"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 2,
                resource: uniform.as_entire_binding(),
            }],
        });
        Self {
            models: Vec::new(),
            uniform,
            group,
        }
    }

    /// Uploads new models, and the instances of models whose revision changed.
    pub(super) fn prepare(
        &mut self,
        device: &wgpu::Device,
        uploads: &mut Uploads,
        encoder: &mut wgpu::CommandEncoder,
        scene: &PartScene,
        camera: DVec3,
    ) {
        uploads.buffer(
            encoder,
            &self.uniform,
            0,
            bytemuck::bytes_of(&PartFrame::new(camera, scene.seconds)),
        );
        // A scene with fewer models is a new one (another world): start over.
        if scene.models.len() < self.models.len() {
            self.models.clear();
        }
        let buffer = |vertices: &[Vertex]| {
            // wgpu refuses an empty buffer: an emptied model keeps one vertex's worth, unused.
            let zero = [<Vertex as bytemuck::Zeroable>::zeroed()];
            let contents: &[Vertex] = if vertices.is_empty() { &zero } else { vertices };
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Part model"),
                contents: bytemuck::cast_slice(contents),
                usage: wgpu::BufferUsages::VERTEX,
            })
        };
        for (i, model) in self.models.iter_mut().enumerate() {
            if model.shape != scene.shapes[i] {
                model.shape = scene.shapes[i];
                model.vertices = buffer(&scene.models[i]);
                model.quads = (scene.models[i].len() / 4) as u32;
            }
        }
        for (i, vertices) in scene.models.iter().enumerate().skip(self.models.len()) {
            self.models.push(Model {
                vertices: buffer(vertices),
                shape: scene.shapes[i],
                quads: (vertices.len() / 4) as u32,
                instances: None,
                capacity: 0,
                count: 0,
                revision: None,
            });
        }
        for (i, model) in self.models.iter_mut().enumerate() {
            let revision = scene.revision[i];
            if model.revision == Some(revision) {
                continue;
            }
            model.revision = Some(revision);
            let list = &scene.instances[i];
            model.count = list.len() as u32;
            if list.is_empty() {
                model.instances = None;
                model.capacity = 0;
                continue;
            }
            if list.len() > model.capacity || list.len() < model.capacity / 4 {
                model.capacity = list.len().next_power_of_two();
                model.instances = Some(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Part instances"),
                    size: (model.capacity * std::mem::size_of::<PartInstance>()) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
            }
            uploads.buffer(
                encoder,
                model.instances.as_ref().expect("made above"),
                0,
                bytemuck::cast_slice(list),
            );
        }
    }

    /// One instanced draw per model with instances. The pass holds the part pipeline, the
    /// entity bind group at 0 and the quad index buffer.
    pub(super) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_bind_group(1, &self.group, &[]);
        for model in &self.models {
            let Some(instances) = &model.instances else {
                continue;
            };
            pass.set_vertex_buffer(0, model.vertices.slice(..));
            pass.set_vertex_buffer(1, instances.slice(..));
            pass.draw_indexed(0..model.quads * 6, 0, 0..model.count);
        }
    }

    /// Whether anything is drawn.
    pub(super) fn any(&self) -> bool {
        self.models.iter().any(|m| m.instances.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cleared_model_drops_its_vertices_and_instances() {
        let mut scene = PartScene::default();
        let m = scene.add_model(vec![<Vertex as bytemuck::Zeroable>::zeroed(); 8]);
        scene.instances[m as usize].push(PartInstance::default());
        let (rev, shape) = (scene.revision[0], scene.shapes[0]);
        scene.clear_model(m);
        assert!(scene.models[0].is_empty() && scene.instances[0].is_empty());
        assert!(scene.revision[0] > rev && scene.shapes[0] > shape);
        assert_eq!(
            scene.add_model(vec![<Vertex as bytemuck::Zeroable>::zeroed(); 4]),
            1,
            "its handle stays taken"
        );
    }

    #[test]
    fn the_instance_layout_matches_the_shader() {
        // entity.wgsl `vs_part`: 7 block, 8 spin, 9 light (at 60), 10 offset (28), 11 velocity (44).
        assert_eq!(std::mem::size_of::<PartInstance>(), 64);
        assert_eq!(std::mem::offset_of!(PartInstance, offset), 28);
        assert_eq!(std::mem::offset_of!(PartInstance, velocity), 44);
        assert_eq!(std::mem::offset_of!(PartInstance, light), 60);
    }

    #[test]
    fn the_frame_keeps_instance_positions_exact() {
        let f = PartFrame::new(DVec3::new(-1.25, 64.5, 30_000_000.75), 12.0);
        assert_eq!(f.block, [-2, 64, 30_000_000, 0]);
        assert_eq!(f.fraction, [0.75, 0.5, 0.75, 12.0]);
    }

    #[test]
    fn the_largest_drawn_model_sizes_the_index_buffer() {
        let mut scene = PartScene::default();
        let small = scene.add_model(vec![bytemuck::Zeroable::zeroed(); 8]);
        let large = scene.add_model(vec![bytemuck::Zeroable::zeroed(); 40]);
        assert_eq!(scene.max_quads(), 0, "no instances, nothing drawn");
        scene.instances[small as usize].push(PartInstance::default());
        assert_eq!(scene.max_quads(), 2);
        scene.instances[large as usize].push(PartInstance::default());
        assert_eq!(scene.max_quads(), 10);
    }
}
