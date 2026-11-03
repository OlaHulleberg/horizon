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
    /// Models drawn with their back faces culled (text, as vanilla's text render type is); the
    /// rest are drawn from both sides.
    pub culled: Vec<bool>,
}

impl PartScene {
    /// Adds a model, returning its handle.
    pub fn add_model(&mut self, vertices: Vec<Vertex>) -> u32 {
        self.models.push(vertices.into());
        self.instances.push(Vec::new());
        self.revision.push(0);
        self.shapes.push(0);
        self.culled.push(false);
        (self.models.len() - 1) as u32
    }
    /// Draws a model with its back faces culled, or from both sides again.
    pub fn set_culled(&mut self, model: u32, culled: bool) {
        if let Some(c) = self.culled.get_mut(model as usize) {
            *c = culled;
        }
    }
    /// Replaces a model's vertices (a native mod's text that changed); its instances stay.
    pub fn set_model(&mut self, model: u32, vertices: Vec<Vertex>) {
        let i = model as usize;
        if i < self.models.len() {
            self.models[i] = vertices.into();
            self.shapes[i] += 1;
        }
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
    culled: bool,
    shape: u64,
    quads: u32,
    /// How far the model reaches from its block's corner, so a section's box grows by it.
    extent: f32,
    instances: Option<wgpu::Buffer>,
    capacity: usize,
    count: u32,
    revision: Option<u64>,
    /// The uploaded instances: first the moving ones, then the still ones, each sorted by the
    /// chunk section they start in, each section one run (`moving` of them move).
    moving: u32,
    sections: Vec<([i32; 3], u32, u32)>,
    /// The moving instances as uploaded, to find each frame how far they have glided.
    gliding: Vec<PartInstance>,
    /// This frame's instance ranges: the moving ones and the sections in view.
    draws: Vec<std::ops::Range<u32>>,
}

/// The chunk section a still instance stands in: its block plus its fixed offset.
fn section_of(i: &PartInstance) -> [i32; 3] {
    let at = |a: usize| (i.block[a] as f32 + i.offset[a]).floor() as i32 >> 4;
    [at(0), at(1), at(2)]
}

/// How far a model's vertices reach outside its block, in blocks.
fn extent_of(vertices: &[Vertex]) -> f32 {
    vertices.iter().fold(0.0, |e: f32, v| {
        v.pos.iter().fold(e, |e, &c| e.max(-c).max(c - 1.0))
    })
}

/// The instances in the order they're uploaded (moving first, then still ones, each by the
/// section they start in), how many move, and each section's run.
fn sort_by_section(list: &[PartInstance]) -> (Vec<PartInstance>, u32, Vec<([i32; 3], u32, u32)>) {
    let moves = |i: &PartInstance| i.velocity[..3].iter().any(|v| *v != 0.0);
    let mut sorted: Vec<PartInstance> = list.iter().filter(|i| moves(i)).copied().collect();
    let moving = sorted.len() as u32;
    sorted.sort_by_key(section_of);
    let mut still: Vec<PartInstance> = list.iter().filter(|i| !moves(i)).copied().collect();
    still.sort_by_key(section_of);
    sorted.extend(still);
    let mut sections: Vec<([i32; 3], u32, u32)> = Vec::new();
    for (n, i) in sorted.iter().enumerate() {
        let at = section_of(i);
        // A run never spans the moving and the still ones.
        match sections.last_mut() {
            Some((s, start, count)) if *s == at && (*start < moving) == ((n as u32) < moving) => {
                *count += 1
            }
            _ => sections.push((at, n as u32, 1)),
        }
    }
    (sorted, moving, sections)
}

/// How far a gliding instance has gone from where it started, in blocks, at clock `seconds`
/// (`vs_part`: `offset` at time `offset.w`, then `velocity` blocks a second). None once the
/// clock has wrapped past its start: then it may be anywhere along its line.
fn glided(i: &PartInstance, seconds: f32) -> Option<glam::Vec3> {
    let dt = seconds - i.offset[3];
    (dt >= 0.0).then(|| glam::Vec3::new(i.velocity[0], i.velocity[1], i.velocity[2]) * dt)
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
        view: &super::Frustum,
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
            model.culled = scene.culled.get(i).copied().unwrap_or(false);
            if model.shape != scene.shapes[i] {
                model.shape = scene.shapes[i];
                model.vertices = buffer(&scene.models[i]);
                model.quads = (scene.models[i].len() / 4) as u32;
                model.extent = extent_of(&scene.models[i]);
            }
        }
        for (i, vertices) in scene.models.iter().enumerate().skip(self.models.len()) {
            self.models.push(Model {
                vertices: buffer(vertices),
                culled: scene.culled.get(i).copied().unwrap_or(false),
                shape: scene.shapes[i],
                quads: (vertices.len() / 4) as u32,
                extent: extent_of(vertices),
                instances: None,
                capacity: 0,
                count: 0,
                revision: None,
                moving: 0,
                sections: Vec::new(),
                gliding: Vec::new(),
                draws: Vec::new(),
            });
        }
        for (i, model) in self.models.iter_mut().enumerate() {
            let revision = scene.revision[i];
            if model.revision == Some(revision) {
                continue;
            }
            model.revision = Some(revision);
            let (list, moving, sections) = sort_by_section(&scene.instances[i]);
            model.count = list.len() as u32;
            model.moving = moving;
            model.sections = sections;
            model.gliding = list[..moving as usize].to_vec();
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
                bytemuck::cast_slice(&list),
            );
        }
        // The sections in view, each grown by its model's reach, joined into runs.
        for model in &mut self.models {
            model.draws.clear();
            if model.instances.is_none() {
                continue;
            }
            let reach = model.extent;
            for &(at, start, count) in &model.sections {
                let corner = DVec3::new(at[0] as f64, at[1] as f64, at[2] as f64) * 16.0 - camera;
                let mut min = corner.as_vec3() - glam::Vec3::splat(reach);
                let mut max = corner.as_vec3() + glam::Vec3::splat(16.0 + reach);
                // A moving run's box also reaches as far as its instances have glided.
                let mut anywhere = false;
                let (mut back, mut ahead) = (glam::Vec3::ZERO, glam::Vec3::ZERO);
                let run = model.gliding.get(start as usize..(start + count) as usize);
                for i in run.unwrap_or(&[]) {
                    match glided(i, scene.seconds) {
                        Some(d) => {
                            back = back.min(d);
                            ahead = ahead.max(d);
                        }
                        None => anywhere = true,
                    }
                }
                min += back;
                max += ahead;
                if !anywhere && !view.intersects(min, max) {
                    continue;
                }
                match model.draws.last_mut() {
                    Some(run) if run.end == start => run.end = start + count,
                    _ => model.draws.push(start..start + count),
                }
            }
        }
    }

    /// One instanced draw per model with instances, of the models culled or not as `culled`
    /// says. The pass holds the matching part pipeline, the entity bind group at 0 and the quad
    /// index buffer.
    pub(super) fn draw(&self, pass: &mut wgpu::RenderPass<'_>, culled: bool) {
        pass.set_bind_group(1, &self.group, &[]);
        for model in self.models.iter().filter(|m| m.culled == culled) {
            let Some(instances) = &model.instances else {
                continue;
            };
            if model.draws.is_empty() {
                continue;
            }
            pass.set_vertex_buffer(0, model.vertices.slice(..));
            pass.set_vertex_buffer(1, instances.slice(..));
            for run in &model.draws {
                pass.draw_indexed(0..model.quads * 6, 0, run.clone());
            }
        }
    }

    /// Whether anything is drawn.
    pub(super) fn any(&self) -> bool {
        self.models.iter().any(|m| m.instances.is_some())
    }

    /// Whether any culled model is drawn.
    pub(super) fn any_culled(&self) -> bool {
        self.models
            .iter()
            .any(|m| m.culled && m.instances.is_some())
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
    fn instances_sort_moving_first_then_one_run_per_section() {
        let at = |x: i32, z: i32| PartInstance {
            block: [x, 64, z],
            ..Default::default()
        };
        let mut gliding = at(0, 0);
        gliding.velocity = [0.5, 0.0, 0.0, 0.0];
        // A fixed offset moves an instance into the next section along x.
        let mut shifted = at(15, 0);
        shifted.offset = [1.5, 0.0, 0.0, 0.0];
        let list = [at(20, 0), at(1, 1), gliding, shifted, at(2, 2), at(40, 0)];
        let (sorted, moving, sections) = sort_by_section(&list);
        assert_eq!(moving, 1);
        assert_eq!(sorted[0].velocity[0], 0.5, "moving ones first");
        assert_eq!(
            sections,
            vec![
                ([0, 4, 0], 0, 1),
                ([0, 4, 0], 1, 2),
                ([1, 4, 0], 3, 2),
                ([2, 4, 0], 5, 1)
            ],
            "the moving one is its own run though it shares a section; (0..16) holds two still \
             ones, (16..32) holds 20 and the shifted 15, 40 is alone"
        );
        assert_eq!(sorted.len(), list.len());
    }

    #[test]
    fn a_gliding_instance_is_found_where_it_has_got_to() {
        let i = PartInstance {
            offset: [0.0, 0.0, 0.0, 10.0],
            velocity: [2.0, 0.0, -1.0, 0.0],
            ..Default::default()
        };
        assert_eq!(glided(&i, 13.0), Some(glam::Vec3::new(6.0, 0.0, -3.0)));
        assert_eq!(glided(&i, 10.0), Some(glam::Vec3::ZERO));
        assert_eq!(
            glided(&i, 4.0),
            None,
            "the clock wrapped: it may be anywhere on its line"
        );
    }

    #[test]
    fn a_model_reaches_as_far_as_its_vertices_leave_the_block() {
        let v = |x: f32, y: f32, z: f32| Vertex {
            pos: [x, y, z],
            ..bytemuck::Zeroable::zeroed()
        };
        assert_eq!(extent_of(&[v(0.0, 0.0, 0.0), v(1.0, 1.0, 1.0)]), 0.0);
        assert_eq!(extent_of(&[v(-2.5, 0.5, 0.5), v(0.5, 4.0, 0.5)]), 3.0);
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
