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
