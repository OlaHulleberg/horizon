//! #808: one resolved `ModelRef` drawn as N mod block entities, through the per-copy CPU route
//! and through the prepared buffer, so before and after come from one build.
//!
//! The test game has no `Renderer`, so `prepared_route` would stay false and every scene would
//! silently take the CPU route. A scene that stands `entity_bench::Bench` in for the renderer
//! gets the route the game picks with a resident upload; a scene that doesn't is what a frame
//! without an upload gets. Every row asserts the route it claims.
use super::native_entity_refs::tests::{run_fresh_cases, with_client_game_at};
use glam::{DVec3, Mat4, Vec3};
use horizon_client::gfx::{PreparedModelInstance, entity_bench::Bench};
use horizon_client::{containers::BlockEntity, mesher::Vertex};
use mod_api::{self, __private::game::Fields, prelude::*, *};
use std::{
    cell::Cell,
    collections::HashMap,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU8, Ordering},
    },
};

const SHAFT: &str = "bench:shaft";
const MODEL: &str = "minecraft:block/grass_block";
const COUNTS: [usize; 4] = [1, 1_000, 5_000, 16_000];
const WARMUP: usize = 30;
/// Timed frames of a scene, on the CPU and on the GPU, unless `MODEL_COPIES_FRAMES` says otherwise.
const FRAMES_ENV: &str = "MODEL_COPIES_FRAMES";

const KEPT: u8 = 0;
const SPIN: u8 = 1;
const ROTATED: u8 = 2;
const STILL: u8 = 3;

/// Which route a scene's frames must take.
#[derive(Clone, Copy, PartialEq)]
enum Route {
    /// No upload on the GPU: `prepared_route` is false, each copy is CPU vertices.
    NoUpload,
    /// The upload is resident and the hook decides: copies it keeps or spins stay CPU vertices.
    Uploaded,
    /// The upload is resident and a `shaders/` pack is loaded: CPU vertices with a span for each.
    Pack,
}

/// The scenes in baseline order. The baseline had no renderer, so its rows are `NoUpload`, and
/// `KEPT` and `SPIN` are CPU under either: they run as `Uploaded`, as the real game would.
const SCENES: [(&str, u8, Route); 7] = [
    ("static, kept", KEPT, Route::Uploaded),
    ("rotating, kept + spin", SPIN, Route::Uploaded),
    ("rotating, resubmitted", ROTATED, Route::NoUpload),
    ("rotating, resubmitted", ROTATED, Route::Uploaded),
    ("static, resubmitted", STILL, Route::NoUpload),
    ("static, resubmitted", STILL, Route::Uploaded),
    ("rotating, resubmitted", ROTATED, Route::Pack),
];

thread_local! {
    static STAND_IN: Cell<Option<&'static Bench>> = const { Cell::new(None) };
}

/// The renderer `Game` asks about the prepared buffer in this scene, if any.
pub(super) fn renderer_stand_in() -> Option<&'static Bench> {
    STAND_IN.get()
}

/// How the copies draw: `KEPT`, `SPIN` (kept, turning by `spin`), `ROTATED` (turned anew each
/// frame), or `STILL` (drawn anew each frame, not turned, not kept).
static MODE: AtomicU8 = AtomicU8::new(0);
static MODEL_REF: OnceLock<ModelRef> = OnceLock::new();

struct Shaft;
#[mod_api::horizon_part]
impl BlockPart for Shaft {}
#[mod_api::horizon_part]
impl BlockEntityPart for Shaft {
    fn block_entity_submit(&self, _: &dyn ClientHost, event: &mut BlockEntitySubmitEvent<'_>) {
        let model = *MODEL_REF.get().expect("model resolved");
        let draw = &mut *event.draw;
        match MODE.load(Ordering::Relaxed) {
            KEPT => draw.keep(),
            SPIN => {
                draw.keep();
                draw.spin([0.0, 1.0, 0.0], [0.5, 0.5, 0.5], 4.8, 0.0);
            }
            ROTATED => {
                let degrees = (draw.game_time() as f32 + draw.partial()) * 4.8;
                draw.rotate_about([0.0, 1.0, 0.0], degrees % 360.0, [0.5, 0.5, 0.5]);
            }
            _ => {}
        }
        draw.model(model, 0xFF_FFFF).expect("model draws");
    }
}

struct BenchMod;
#[mod_api::__subscriptions]
impl Hooks for BenchMod {
    fn register(&self, registrar: &mut dyn Registrar, _: &mod_api::RegistrationContext) {
        let kind = registrar
            .block_entity_type("shaft")
            .blocks(&[SHAFT])
            .build();
        registrar
            .block_entity_part(&kind, Shaft)
            .expect("native type");
    }
    fn blocks(&self, _: &mod_api::RegistrationContext, event: &mut BlocksEvent<'_>) {
        event.blocks.push(horizon_mod_package::content::Block {
            id: SHAFT.into(),
            template: "minecraft:stone".into(),
            item: false,
            ..Default::default()
        });
    }
}

/// Copy `i` of a 32x25x20 grid 1.5 blocks apart, in view of a camera looking down +z.
fn cell(i: usize) -> [i32; 3] {
    let (x, y, z) = (i % 32, (i / 32) % 25, i / (32 * 25));
    [
        (x as f32 * 1.5 - 24.0).floor() as i32,
        (y as f32 * 1.5 - 18.0).floor() as i32 + 100,
        (z as f32 * 1.5 + 24.0).floor() as i32,
    ]
}

fn stats(mut v: Vec<f64>) -> (f64, f64, f64) {
    v.sort_by(f64::total_cmp);
    let at = |q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
    (at(0.5), at(0.95), v.iter().sum::<f64>() / v.len() as f64)
}

/// The world's copies: `count` mod block entities, nothing kept from the scene before.
fn place_copies(game: &mut super::Game<'_>, count: usize, state: u32) -> Vec<([i32; 3], u32)> {
    game.kept_block_entities.borrow_mut().clear();
    game.world.block_entities.clear();
    let blocks: Vec<([i32; 3], u32)> = (0..count).map(|i| (cell(i), state)).collect();
    for &(pos, _) in &blocks {
        game.world.block_entities.insert(
            (pos[0], pos[1], pos[2]),
            BlockEntity::Mod(Box::new(horizon_client::containers::ModBlockEntity::new(
                "bench:shaft".into(),
                Default::default(),
            ))),
        );
    }
    blocks
}

/// Pixels the entity pass covered.
fn covered(pixels: &[u8]) -> usize {
    pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[..3] != [0, 0, 0])
        .count()
}

/// Pixels that differ, and the widest difference of any channel.
fn difference(a: &[u8], b: &[u8]) -> (usize, u8) {
    let (mut pixels, mut worst) = (0, 0);
    for (a, b) in a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0) {
        let widest = (0..4).map(|i| a[i].abs_diff(b[i])).max().unwrap();
        pixels += usize::from(widest > 0);
        worst = worst.max(widest);
    }
    (pixels, worst)
}

#[test]
#[ignore = "bench; needs prepared resources in HORIZON_TEST_RESOURCES and a hardware adapter"]
fn model_copies_bench() {
    const CHILD: &str = "HORIZON_MODEL_COPIES_BENCH_CHILD";
    if run_fresh_cases(
        "game::model_copies_bench_tests::model_copies_bench",
        CHILD,
        &["bench"],
    ) {
        return;
    }
    let frames: usize = std::env::var(FRAMES_ENV).map_or(200, |v| v.parse().unwrap());
    horizon_client::models::register_standalone(vec![MODEL.into()]);
    let mods: Mods = Arc::from([ModEntry::new("bench", Box::new(BenchMod))]);
    mod_api::__private::install(mods.clone());
    mod_api::__private::install_parts(&mods).unwrap();
    let base = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
    let root = super::mods::content_root(&base, &Default::default())
        .unwrap()
        .prepared()
        .to_path_buf();
    with_client_game_at(root, |game| {
        let id = Identifier::parse(MODEL).unwrap();
        let _ = MODEL_REF.set(game.render_assets().model(&id).unwrap());
        let model = *MODEL_REF.get().unwrap();
        let state = game.pack.assets.registry.default_state(SHAFT).unwrap().0;
        let quads = game.pack.render.resolve_model(model).unwrap().faces.len();
        println!(
            "model {MODEL}: {quads} faces; Vertex {} bytes; instance {} bytes",
            std::mem::size_of::<Vertex>(),
            std::mem::size_of::<PreparedModelInstance>(),
        );
        // `HORIZON_STILL=1`: a frozen level at partial tick 1 (`FixedStep::still`), so every frame
        // of a scene, and both routes of a scene, draw the same picture.
        let still = game.clock.still();
        println!("HORIZON_STILL: {still}; {frames} timed frames after {WARMUP} warm-up frames");
        let camera = DVec3::new(0.5, 100.5, 0.5);
        let proj = Mat4::perspective_infinite_reverse_rh(70f32.to_radians(), 16.0 / 9.0, 0.05);
        let view = Mat4::look_to_rh(Vec3::ZERO, Vec3::Z, Vec3::Y);
        let atlas = [128u8, 128, 128, 255].repeat(1024 * 1024);
        let mut bench = Bench::open([1024, 1024], &atlas, proj * view)
            .expect("a hardware Vulkan adapter with timestamp queries");
        println!("adapter: {}", bench.adapter());
        let generation = game.standalone_geometry.generation();
        let (vertices, ranges) = game.standalone_geometry.prepared_upload();
        let slot_range = ranges[model.fields().slot as usize];
        println!(
            "prepared buffer: generation {generation}, {} vertices, this model's index range {slot_range:?}",
            vertices.len(),
        );
        bench
            .install_prepared_models(generation, &vertices, ranges)
            .unwrap();
        let bench: &'static Bench = Box::leak(Box::new(bench));
        let allocations = horizon_threads::alloc_count::thread_allocations;
        println!(
            "| scene | copies | CPU ms/frame p50 | p95 | allocs/frame | vertices | bytes uploaded/frame | entity draws | GPU pass ms p50 | p95 | GPU upload+pass+wait ms p50 | route | instances | render-thread submit ms p50 | submit allocs/frame | covered px | px differing from CPU route (worst channel) |"
        );
        println!("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|");
        let mut cpu_pixels: HashMap<(u8, usize), Vec<u8>> = HashMap::new();
        for (scene, mode, route) in SCENES {
            MODE.store(mode, Ordering::Relaxed);
            STAND_IN.set((route != Route::NoUpload).then_some(bench));
            game.pack_ids =
                (route == Route::Pack).then(super::shader_packs::PackIds::without_properties);
            for count in COUNTS {
                let blocks = place_copies(game, count, state);
                let mut out: Vec<Vertex> = Vec::new();
                let mut times = Vec::with_capacity(frames);
                let mut allocs = 0u64;
                for frame in 0..WARMUP + frames {
                    game.world.game_time = if still { 1_000 } else { 1_000 + frame as i64 };
                    let partial = if still { 1.0 } else { (frame % 7) as f32 / 7.0 };
                    out.clear();
                    // As `render_with` and `entity_geometry` do before a frame.
                    game.prepared_model_draws.borrow_mut().clear();
                    game.entity_spans.borrow_mut().clear();
                    let a = horizon_threads::alloc_count::thread_allocations();
                    let start = std::time::Instant::now();
                    game.mod_block_entity_geometry(blocks.iter(), camera, partial, &mut out);
                    let elapsed = start.elapsed().as_secs_f64() * 1e3;
                    let b = horizon_threads::alloc_count::thread_allocations();
                    std::hint::black_box(&out);
                    if frame >= WARMUP {
                        times.push(elapsed);
                        allocs += b - a;
                    }
                }
                let (cpu50, cpu95, _) = stats(times);
                let allocs = allocs as f64 / frames as f64;
                let draws = game.prepared_model_draws.borrow().clone();
                let prepared = route == Route::Uploaded && matches!(mode, ROTATED | STILL);
                if prepared {
                    assert_eq!(draws.len(), count, "{scene}: every copy is an instance");
                    assert!(out.is_empty(), "{scene}: no copy is in the vertex stream");
                } else {
                    assert!(draws.is_empty(), "{scene}: no copy is an instance");
                    assert_eq!(out.len(), count * quads * 4, "{scene}: CPU vertices");
                }
                let route_name = match route {
                    _ if prepared => "prepared",
                    Route::Pack => "cpu, pack on",
                    _ => "cpu",
                };
                if route == Route::Pack {
                    // Pack spans still cover every copy. What a pack draws needs its renderer.
                    assert_eq!(game.entity_spans.borrow().len(), count);
                    println!(
                        "| {scene} | {count} | {cpu50:.3} | {cpu95:.3} | {allocs:.1} | {} | - | - | - | - | - | {route_name} | 0 | - | - | - | - |",
                        out.len(),
                    );
                    continue;
                }
                let gpu = bench.time_frames(&out, &draws, WARMUP, frames, allocations);
                let expected = if prepared { count as u64 } else { 1 };
                assert_eq!(gpu.draws, expected, "{scene}: draws made");
                let quantile = |f: fn(&horizon_client::gfx::entity_bench::GpuFrame) -> f64| {
                    stats(gpu.frames.iter().map(f).collect())
                };
                let (gpu50, gpu95, _) = quantile(|f| f.pass_ms);
                let (wall50, _, _) = quantile(|f| f.wall_ms);
                let (submit50, _, _) = quantile(|f| f.submit_ms);
                let submit_allocs =
                    gpu.frames.iter().map(|f| f.submit_allocs).sum::<u64>() as f64 / frames as f64;
                let pixels = covered(&gpu.pixels);
                // Copy 0 sits just below the view: y/z is 17.5/24.5, tan(35 degrees) is 0.70.
                assert!(pixels > 0 || count == 1, "{scene}: the pass drew nothing");
                let against_cpu = match cpu_pixels.get(&(mode, count)) {
                    Some(cpu) if prepared => {
                        let (differ, worst) = difference(cpu, &gpu.pixels);
                        format!("{differ} ({worst})")
                    }
                    _ => "-".into(),
                };
                if !prepared && matches!(mode, ROTATED | STILL) {
                    cpu_pixels.insert((mode, count), gpu.pixels);
                }
                println!(
                    "| {scene} | {count} | {cpu50:.3} | {cpu95:.3} | {allocs:.1} | {} | {} | {} | {gpu50:.3} | {gpu95:.3} | {wall50:.3} | {route_name} | {} | {submit50:.3} | {submit_allocs:.1} | {pixels} | {against_cpu} |",
                    out.len(),
                    gpu.uploaded_bytes,
                    gpu.draws,
                    draws.len(),
                );
            }
        }
        game.pack_ids = None;
        // The scenes above have no CPU vertices beside the copies. A real frame does, and `render`
        // finds each copy's cut by walking the stream from its start.
        println!(
            "| extra: copies after CPU vertices | prefix quads | render-thread submit ms p50 | submit allocs/frame | bytes uploaded/frame | entity draws |"
        );
        println!("|---|---|---|---|---|---|");
        MODE.store(STILL, Ordering::Relaxed);
        STAND_IN.set(None);
        let blocks = place_copies(game, 1_000, state);
        let mut prefix: Vec<Vertex> = Vec::new();
        game.mod_block_entity_geometry(blocks.iter(), camera, 1.0, &mut prefix);
        STAND_IN.set(Some(bench));
        let blocks = place_copies(game, 16_000, state);
        game.prepared_model_draws.borrow_mut().clear();
        let mut none: Vec<Vertex> = Vec::new();
        game.mod_block_entity_geometry(blocks.iter(), camera, 1.0, &mut none);
        let mut draws = game.prepared_model_draws.borrow().clone();
        assert_eq!(draws.len(), 16_000);
        for draw in &mut draws {
            draw.cpu_vertices = prefix.len() as u32;
        }
        let gpu = bench.time_frames(&prefix, &draws, WARMUP, frames, allocations);
        let (submit50, _, _) = stats(gpu.frames.iter().map(|f| f.submit_ms).collect());
        let submit_allocs =
            gpu.frames.iter().map(|f| f.submit_allocs).sum::<u64>() as f64 / frames as f64;
        println!(
            "| 16000 | {} | {submit50:.3} | {submit_allocs:.1} | {} | {} |",
            prefix.len() / 4,
            gpu.uploaded_bytes,
            gpu.draws,
        );
        STAND_IN.set(None);
    });
}
