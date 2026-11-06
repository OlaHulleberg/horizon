//! #808 baseline: one resolved `ModelRef` drawn as N mod block entities through the per-copy route.
use super::native_entity_refs::tests::{run_fresh_cases, with_client_game_at};
use glam::{DVec3, Mat4, Vec3};
use horizon_client::{containers::BlockEntity, mesher::Vertex};
use mod_api::{self, *};
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicU8, Ordering},
};

const SHAFT: &str = "bench:shaft";
const MODEL: &str = "minecraft:block/grass_block";
const COUNTS: [usize; 4] = [1, 1_000, 5_000, 16_000];
const WARMUP: usize = 30;
const FRAMES: usize = 200;
const GPU_FRAMES: usize = 200;

/// How the copies draw: 0 kept still, 1 kept and turning by `spin`, 2 turned anew each frame.
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
            0 => draw.keep(),
            1 => {
                draw.keep();
                draw.spin([0.0, 1.0, 0.0], [0.5, 0.5, 0.5], 4.8, 0.0);
            }
            _ => {
                let degrees = (draw.game_time() as f32 + draw.partial()) * 4.8;
                draw.rotate_about([0.0, 1.0, 0.0], degrees % 360.0, [0.5, 0.5, 0.5]);
            }
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
        event.blocks.push(horizon_mods::content::Block {
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

#[test]
#[ignore = "bench; needs prepared resources in HORIZON_TEST_RESOURCES"]
fn model_copies_bench() {
    const CHILD: &str = "HORIZON_MODEL_COPIES_BENCH_CHILD";
    if run_fresh_cases(
        "game::model_copies_bench::model_copies_bench",
        CHILD,
        &["bench"],
    ) {
        return;
    }
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
        let state = game.pack.assets.registry.default_state(SHAFT).unwrap().0;
        let quads = game
            .pack
            .render
            .resolve_model(*MODEL_REF.get().unwrap())
            .unwrap()
            .faces
            .len();
        println!(
            "model {MODEL}: {quads} faces; Vertex {} bytes",
            std::mem::size_of::<Vertex>()
        );
        let camera = DVec3::new(0.5, 100.5, 0.5);
        let proj = Mat4::perspective_infinite_reverse_rh(70f32.to_radians(), 16.0 / 9.0, 0.05);
        let view = Mat4::look_to_rh(Vec3::ZERO, Vec3::Z, Vec3::Y);
        let atlas = [128u8, 128, 128, 255].repeat(1024 * 1024);
        println!(
            "| scene | copies | CPU ms/frame p50 | p95 | allocs/frame | vertices | bytes uploaded/frame | entity draws | GPU pass ms p50 | p95 | GPU upload+pass+wait ms p50 |"
        );
        println!("|---|---|---|---|---|---|---|---|---|---|---|");
        for (mode, scene) in [
            (0u8, "static, kept"),
            (1, "rotating, kept + spin"),
            (2, "rotating, resubmitted"),
        ] {
            MODE.store(mode, Ordering::Relaxed);
            for count in COUNTS {
                game.kept_block_entities.borrow_mut().clear();
                game.world.block_entities.clear();
                let blocks: Vec<([i32; 3], u32)> = (0..count).map(|i| (cell(i), state)).collect();
                for &(pos, _) in &blocks {
                    game.world.block_entities.insert(
                        (pos[0], pos[1], pos[2]),
                        BlockEntity::Mod(Box::new(
                            horizon_client::containers::ModBlockEntity::new(
                                "bench:shaft".into(),
                                Default::default(),
                            ),
                        )),
                    );
                }
                let mut out: Vec<Vertex> = Vec::new();
                let mut times = Vec::with_capacity(FRAMES);
                let mut allocs = 0u64;
                for frame in 0..WARMUP + FRAMES {
                    game.world.game_time = 1_000 + frame as i64;
                    let partial = (frame % 7) as f32 / 7.0;
                    out.clear();
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
                let bytes = std::mem::size_of_val(out.as_slice());
                let gpu = horizon_client::gfx::entity_bench::time_frames(
                    &out,
                    [1024, 1024],
                    &atlas,
                    proj * view,
                    WARMUP,
                    GPU_FRAMES,
                );
                let (gpu50, gpu95, wall50, adapter) = match gpu {
                    Some((adapter, frames)) => {
                        let (p50, p95, _) = stats(frames.iter().map(|f| f.pass_ms).collect());
                        let (w50, _, _) = stats(frames.iter().map(|f| f.wall_ms).collect());
                        (
                            format!("{p50:.3}"),
                            format!("{p95:.3}"),
                            format!("{w50:.3}"),
                            adapter,
                        )
                    }
                    None => ("-".into(), "-".into(), "-".into(), "none".into()),
                };
                println!(
                    "| {scene} | {count} | {cpu50:.3} | {cpu95:.3} | {:.1} | {} | {bytes} | 1 | {gpu50} | {gpu95} | {wall50} |",
                    allocs as f64 / FRAMES as f64,
                    out.len(),
                );
                if mode == 0 && count == 1 {
                    println!("adapter: {adapter}");
                }
            }
        }
    });
}
