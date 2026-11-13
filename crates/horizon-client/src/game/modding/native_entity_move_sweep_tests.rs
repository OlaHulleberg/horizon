//! The `entity_move` sweep (#2414, Ari's 1b): one entity of every walk kind,
//! one full family round each, and a counting listener that must hear exactly
//! one self move per kind per round - the backstop the clippy ban cannot
//! give, because a forgotten family moves without any resolve call.
use super::native_entity_refs::tests::with_game;
use mod_api::{EntityMoveEvent, Flow, Hooks, LoopContext, MoveCause};
use std::sync::{Arc, Mutex};

/// `kind -> (fires, causes)` heard during the round.
type Heard = Vec<(String, MoveCause)>;

struct Counter(Arc<Mutex<Heard>>);

#[mod_api::__subscriptions]
impl Hooks for Counter {
    fn entity_move(&self, _: &dyn LoopContext, event: &mut EntityMoveEvent<'_>) -> Flow {
        self.0
            .lock()
            .unwrap()
            .push((event.kind.to_owned(), event.cause));
        Flow::Continue
    }
}

/// The walk kinds the sweep spawns, each moving once in its family's round:
/// a zombie falling (mobs), a dropped item with speed (items tick every round
/// while it has horizontal speed), a primed TNT, a boat, a minecart, an armor
/// stand falling, a firework rocket in free flight.
const KINDS: &[(&str, &str)] = &[
    ("zombie", "minecraft:zombie"),
    ("item", "minecraft:item"),
    ("tnt", "minecraft:tnt"),
    ("oak_boat", "minecraft:oak_boat"),
    ("minecart", "minecraft:minecart"),
    ("armor_stand", "minecraft:armor_stand"),
    ("firework_rocket", "minecraft:firework_rocket"),
];

/// Spawn one of every walk kind high over a stone floor, run one round of
/// each family's tick, and require exactly one self-cause fire per kind: the
/// counting backstop for «a forgotten family cannot move around the gate».
#[test]
#[ignore = "needs prepared resources in HORIZON_TEST_RESOURCES"]
fn every_walk_kind_fires_entity_move_once_per_round() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
        "counter",
        Box::new(Counter(log.clone())),
    )]
    .into();
    mod_api::testing::with_mods(mods, || {
        with_game(|game| {
            game.build_registries();
            game.set_allow_commands(true);
            let registry = &game.pack.assets.registry;
            let stone = registry.default_state("minecraft:stone").unwrap();
            let mut chunk = horizon_client::chunk::Chunk::empty(0, 0);
            for x in 0..16 {
                for z in 0..16 {
                    chunk.set(x, 8, z, stone.0);
                }
            }
            game.world.chunks.insert((0, 0), std::sync::Arc::new(chunk));
            // One of each kind, airborne with a small horizontal speed so
            // every family's own tick moves it.
            for (name, kind) in KINDS {
                let nbt = match *name {
                    "item" => "{Item:{id:\"minecraft:stone\",Count:1b},Motion:[0.5d,0.0d,0.0d]}",
                    _ => "{Motion:[0.5d,0.0d,0.0d]}",
                };
                game.summon_with_nbt(kind, [5.5, 40.5, 5.5], nbt)
                    .unwrap_or_else(|e| panic!("summon {name}: {e:?}"));
            }
            log.lock().unwrap().clear();
            let mut round = game.native_tick_round();
            game.tick_dropped_items(&mut round);
            let _ = game.tick_creatures();
            game.tick_boats();
            game.tick_carts();
            game.tick_rockets();
            game.tick_one_tnt(0, &[]).unwrap();
            let _ = game.tick_armor_stands();
            let heard = std::mem::take(&mut *log.lock().unwrap());
            let mut by_kind: std::collections::BTreeMap<&str, Vec<MoveCause>> = Default::default();
            for (kind, cause) in &heard {
                by_kind.entry(kind).or_default().push(*cause);
            }
            for (name, kind) in KINDS {
                let causes = by_kind.get(*kind).map(Vec::as_slice).unwrap_or(&[]);
                assert_eq!(
                    causes,
                    &[MoveCause::SelfMovement],
                    "{name} ({kind}): exactly one self move, got {causes:?}"
                );
            }
        });
    });
}

/// Arrows in flight are vanilla `setPos` movers: their own tick fires
/// nothing; a piston push fires once with the piston cause (#2414 point 3).
#[test]
#[ignore = "needs prepared resources in HORIZON_TEST_RESOURCES"]
fn an_arrow_in_flight_fires_nothing_and_a_piston_push_fires_once() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
        "counter",
        Box::new(Counter(log.clone())),
    )]
    .into();
    mod_api::testing::with_mods(mods, || {
        with_game(|game| {
            game.build_registries();
            game.set_allow_commands(true);
            let registry = &game.pack.assets.registry;
            let stone = registry.default_state("minecraft:stone").unwrap();
            let mut chunk = horizon_client::chunk::Chunk::empty(0, 0);
            for x in 0..16 {
                for z in 0..16 {
                    chunk.set(x, 8, z, stone.0);
                    chunk.set(x, 20, z, stone.0);
                }
            }
            game.world.chunks.insert((0, 0), std::sync::Arc::new(chunk));
            assert!(game.run_command("summon arrow 5.5 15.5 5.5 {Motion:[1.0d,0.0d,0.0d]}"));
            log.lock().unwrap().clear();
            let _ = game.tick_projectiles();
            let heard = std::mem::take(&mut *log.lock().unwrap());
            assert!(
                heard.is_empty(),
                "an arrow's own flight is setPos, not Entity.move: {heard:?}"
            );
        });
    });
}

/// The movers `entity_handles()` never sees — the dragon and the player's
/// own server simulation — through `entity_ids_off_walk`, as the #2414
/// ruling asks: the dragon's physics-less step and the player window's
/// `move_self` both fire with the self cause. (The bobber and a LAN guest
/// need their own fixtures - a rod cast and a hosted connection - and stay
/// for the live run.)
#[test]
#[ignore = "needs prepared resources in HORIZON_TEST_RESOURCES"]
fn the_off_walk_movers_fire_too() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
        "counter",
        Box::new(Counter(log.clone())),
    )]
    .into();
    mod_api::testing::with_mods(mods, || {
        with_game(|game| {
            game.build_registries();
            game.set_allow_commands(true);
            let registry = &game.pack.assets.registry;
            let stone = registry.default_state("minecraft:stone").unwrap();
            let mut chunk = horizon_client::chunk::Chunk::empty(0, 0);
            for x in 0..16 {
                for z in 0..16 {
                    chunk.set(x, 8, z, stone.0);
                    chunk.set(x, 40, z, stone.0);
                }
            }
            game.world.chunks.insert((0, 0), std::sync::Arc::new(chunk));
            // The dragon: off the walk, moved by its own physics-less step.
            game.spawn_remote_dragon(605, [5.0, 30.0, 5.0], 0.0);
            let off_walk: std::collections::HashSet<u32> = game.entity_ids_off_walk().collect();
            let dragon_id = game.dragon.as_ref().unwrap().id;
            assert!(
                off_walk.contains(&dragon_id),
                "the dragon is an off-walk mover"
            );
            log.lock().unwrap().clear();
            // Overworld: no fight, so the fight tick runs `aiStep` directly.
            game.tick_fight();
            let heard = std::mem::take(&mut *log.lock().unwrap());
            assert!(
                heard.iter().any(|(kind, cause)| {
                    kind == "minecraft:ender_dragon" && *cause == MoveCause::SelfMovement
                }),
                "the dragon's step fires with the self cause: {heard:?}"
            );
            // The player window: also off the walk, moved by its own
            // `move_self` through the server's own simulation of it. The
            // fixture's game carries no player until one moves in.
            use horizon_client::motion::PlayerMotion;
            if game
                .player
                .as_ref()
                .is_none_or(|p| p.position.y < 20.0 || p.position.y > 45.0)
            {
                game.player = Some(
                    PlayerMotion::new(
                        game.motion,
                        horizon_core::geometry::Vec3::new(5.5, 30.0, 5.5),
                    )
                    .unwrap(),
                );
            }
            game.player.as_mut().unwrap().velocity =
                horizon_core::geometry::Vec3::new(0.5, 0.0, 0.0);
            log.lock().unwrap().clear();
            {
                use horizon_client::motion::MotionWorld;
                let mut player = game.player.take().unwrap();
                let mut world =
                    std::mem::replace(&mut game.world, horizon_client::world::World::new(stone, 0));
                MotionWorld {
                    entities: &[],
                    player: &mut player,
                    scene: &mut world,
                    assets: &game.pack.assets,
                    collision_binding: game.collision,
                    resources: game.motion,
                    health: &game.health_resources,
                    loop_context: &game.loop_context,
                    damage_hooks: &game.damage_hooks,
                    tag_items: None,
                    block_registries: None,
                }
                .server_travel()
                .unwrap();
                game.world = world;
                game.player.replace(player);
            }
            let heard = std::mem::take(&mut *log.lock().unwrap());
            assert!(
                heard.iter().any(|(kind, cause)| {
                    kind == "minecraft:player" && *cause == MoveCause::SelfMovement
                }),
                "the player window's own move fires: {heard:?}"
            );
            // Position assignments the design excludes fire nothing: a /tp
            // teleport moves its target without an Entity.move, so the
            // counting listener stays silent across one.
            log.lock().unwrap().clear();
            assert!(game.run_command("tp @s 12.5 30.0 12.5"));
            let heard = std::mem::take(&mut *log.lock().unwrap());
            assert!(
                heard.is_empty(),
                "a teleport is a position assignment, not a move: {heard:?}"
            );
        });
    });
}

/// A mob goal's own `Entity.move` — `BreathAirGoal`'s drowning swim-up (#2414
/// «goals 1:1») — fires through the same gate `do_move` arms, with the self
/// cause: the goal's tick moves the dolphin by itself, beside travel's own
/// move, exactly the two `Entity.move` invocations vanilla makes.
#[test]
#[ignore = "needs prepared resources in HORIZON_TEST_RESOURCES"]
fn a_drowning_dolphins_goal_move_fires_too() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
        "counter",
        Box::new(Counter(log.clone())),
    )]
    .into();
    mod_api::testing::with_mods(mods, || {
        with_game(|game| {
            game.build_registries();
            game.set_allow_commands(true);
            let registry = &game.pack.assets.registry;
            let stone = registry.default_state("minecraft:stone").unwrap();
            let water = registry.default_state("minecraft:water").unwrap();
            let mut chunk = horizon_client::chunk::Chunk::empty(0, 0);
            for x in 0..16 {
                for z in 0..16 {
                    chunk.set(x, 8, z, stone.0);
                    // Water deep enough to drown in, open to the air above.
                    for y in 9..16 {
                        chunk.set(x, y, z, water.0);
                    }
                }
            }
            game.world.chunks.insert((0, 0), std::sync::Arc::new(chunk));
            game.summon_with_nbt("minecraft:dolphin", [5.5, 10.5, 5.5], "{NoAI:0b}")
                .unwrap_or_else(|e| panic!("summon dolphin: {e:?}"));
            // The air supply low enough that BreathAirGoal takes over.
            game.mobs[0].air = 0;
            log.lock().unwrap().clear();
            let _ = game.tick_creatures();
            let heard: Vec<MoveCause> = std::mem::take(&mut *log.lock().unwrap())
                .into_iter()
                .filter(|(kind, _)| kind == "minecraft:dolphin")
                .map(|(_, cause)| cause)
                .collect();
            assert_eq!(
                heard,
                vec![MoveCause::SelfMovement, MoveCause::SelfMovement],
                "the drowning dolphin's round: the goal's own move beside travel's, \
                 vanilla's two Entity.move invocations"
            );
        });
    });
}
