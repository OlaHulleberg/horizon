//! Small moves through the `entity_move` gate (#2414, ruling 5): a listener's
//! cancel or replacement sends the correction whatever the distance. A walk's
//! step is under «moved wrongly»'s quarter block, and the live run watched a
//! cancelled guest walk 69 steps with no rollback — the threshold let every
//! small cancel through. Built on the real hub: a hosted guest, a packet in
//! its inbox, the hub's own collide path.
use crate::game::game_tests::tests::server_game;
use horizon_client::lan::guest::InboxItem;
use horizon_client::lan::{Joined, NetConfig, Out, PlayerSender, guest::Guest, hub::HubConfig};
use horizon_core::position::ChunkPos;
use horizon_protocol::packets::configuration::ClientInformation;
use horizon_protocol::packets::play::{Clientbound as C, Serverbound as S};
use mod_api::{EntityMoveEvent, Flow, Hooks, LoopContext};
use std::sync::Arc;
use std::sync::atomic::AtomicI32;

/// A listener that cancels every move.
struct CancelAll;

#[mod_api::__subscriptions]
impl Hooks for CancelAll {
    fn entity_move(&self, _: &dyn LoopContext, _: &mut EntityMoveEvent<'_>) -> Flow {
        Flow::Cancel
    }
}

/// A listener that replaces every move's delta with `0`'s — a step's
/// replacement, equal to the request when the move asked for the same.
struct ReplaceEach([f64; 3]);

#[mod_api::__subscriptions]
impl Hooks for ReplaceEach {
    fn entity_move(&self, _: &dyn LoopContext, event: &mut EntityMoveEvent<'_>) -> Flow {
        *event.delta = self.0;
        Flow::Continue
    }
}

fn mods(listener: impl Hooks + 'static) -> mod_api::Mods {
    vec![mod_api::ModEntry::new("ruler", Box::new(listener))].into()
}

/// A hosted guest over the stone floor at `[0.5, 100.0, 0.5]`, its client's
/// receiver, and where it stands — a boat's driver where `boat`. The harness
/// the small-move correction tests stand on.
fn hosted_guest<R>(
    net: &str,
    boat: bool,
    mods: mod_api::Mods,
    body: impl FnOnce(&mut crate::game::Game<'_>, std::sync::mpsc::Receiver<Out>, [f64; 3]) -> R,
) -> R {
    mod_platform::testing::with_mods(mods, || {
        let mut resources = None;
        let mut game = server_game(&mut resources);
        game.build_registries();
        game.set_allow_commands(true);
        let stone = game
            .pack
            .assets
            .registry
            .default_state("minecraft:stone")
            .unwrap()
            .0;
        let mut chunk = horizon_client::chunk::Chunk::empty(0, 0);
        for x in 0..16 {
            for z in 0..16 {
                chunk.set(x, 99, z, stone);
            }
        }
        game.world.chunks.insert((0, 0), Arc::new(chunk));
        game.block_ticks
            .add_empty_chunk(ChunkPos::new(0, 0))
            .unwrap();
        assert!(game.run_command("forceload add 0 0"));
        assert!(game.run_command("tick freeze"));
        let at = if boat {
            assert!(game.run_command("summon minecraft:oak_boat 0.5 100 0.5"));
            let start = game.boats[0].position;
            [start.x, start.y, start.z]
        } else {
            [0.5, 100.0, 0.5]
        };
        game.lan_host(
            crate::game::lan::LanSettings {
                port: 0,
                guest_commands: false,
                force_game_mode: false,
            },
            NetConfig {
                listen: false,
                announce: false,
                ..NetConfig::loopback(0, net.into(), 2)
            },
            HubConfig {
                default_game_mode: 0,
                guest_commands: false,
                force_game_mode: false,
                hardcore: false,
                view_distance: 2,
                simulation_distance: 2,
                dimension: "minecraft:overworld".into(),
                allow_flight: false,
                seed: 0,
                max_players: 2,
                integrated: false,
                levels: Default::default(),
            },
        )
        .unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let guest_entity_id = game.take_entity_id() as i32;
        let mut guest = Guest::new(
            Joined {
                conn: 1,
                uuid: 1,
                name: "Walker".into(),
                properties: Vec::new(),
                info: ClientInformation::new(2),
                brand: "unit".into(),
                sender: PlayerSender::new(tx, Arc::default()),
                latency: Arc::new(AtomicI32::new(0)),
                address: "127.0.0.1:1".parse().unwrap(),
                expected: false,
            },
            guest_entity_id,
            at,
            2,
        );
        guest.placed = true;
        guest.loaded = true;
        guest.load_timeout = 0;
        guest.dimension = "minecraft:overworld".into();
        if boat {
            // Seated as the driver (the first seat, `vehicle_order` 1), its
            // client the one that drives the boat.
            (guest.vehicle, guest.vehicle_order) = (Some(game.boats[0].id), 1);
            guest.vehicle_good = Some(at);
        }
        game.lan.as_mut().unwrap().hub.guests.push(guest);
        game.sync_tick_chunks();
        game.ticked_entities_through = game.next_entity;
        body(&mut game, rx, at)
    })
}

/// The guest's client walks `by` blocks east: one `MovePlayerPos` in its
/// inbox, through the hub's own packet tick.
fn guest_walks(game: &mut crate::game::Game<'_>, by: f64) {
    let from = game.lan.as_ref().unwrap().hub.guests[0].position;
    game.lan.as_mut().unwrap().hub.guests.update(0, |g| {
        g.inbox.push_back(InboxItem::Packet(
            S::MovePlayerPos(horizon_protocol::packets::play::MovePlayerPos {
                x: from[0] + by,
                y: from[1],
                z: from[2],
                flags: 1,
            }),
            None,
        ));
    });
    game.tick_lan_packets();
}

/// The driver's client moves its vehicle `by` blocks east: one
/// `MoveVehicle` in its inbox, through the hub's own packet tick.
fn driver_moves_vehicle(game: &mut crate::game::Game<'_>, by: f64) {
    let from = game.lan.as_ref().unwrap().hub.guests[0].position;
    game.lan.as_mut().unwrap().hub.guests.update(0, |g| {
        g.inbox.push_back(InboxItem::Packet(
            S::MoveVehicle(horizon_protocol::packets::play::MoveVehicleRequest {
                position: [from[0] + by, from[1], from[2]],
                yaw: 0.0,
                pitch: 0.0,
                on_ground: true,
            }),
            None,
        ));
    });
    game.tick_lan_packets();
}

/// Whether the guest's client was sent a correction: the teleport the client
/// must acknowledge (`awaiting_teleport`), for the guest's own packet.
fn guest_corrected(game: &crate::game::Game<'_>) -> bool {
    game.lan.as_ref().unwrap().hub.guests[0]
        .awaiting_teleport
        .is_some()
}

/// Whether the flushed packets to the guest's client hold a vehicle put-back
/// (`MoveVehicle` back to where the vehicle was). The tick holds what it
/// sends a guest until its end, so the flush hands the batch over first —
/// one packet or the batch, whichever form it takes.
fn vehicle_put_back(game: &crate::game::Game<'_>, rx: &std::sync::mpsc::Receiver<Out>) -> bool {
    horizon_client::lan::flush_all(
        game.lan
            .as_ref()
            .unwrap()
            .hub
            .guests
            .iter()
            .map(|g| &g.sender),
    );
    rx.try_iter().any(|out| {
        let bodies = match out {
            Out::Packet(body) => vec![body],
            Out::Packets(bodies) => bodies,
            _ => return false,
        };
        bodies.iter().any(|body| {
            let mut r = horizon_protocol::buf::Reader::new(body);
            let id = r.varint().unwrap();
            matches!(C::decode(id, r.rest()), Ok(C::MoveVehicle(_)))
        })
    })
}

/// A walk's step (0.2, under «moved wrongly»'s quarter block) a listener
/// cancels: the guest keeps its place and the correction goes out — the
/// threshold let the small cancel through before, and the guest's client
/// walked on.
#[test]
#[ignore = "needs prepared resources in HORIZON_TEST_RESOURCES"]
fn a_cancelled_small_step_sends_the_guest_a_correction() {
    hosted_guest(
        "entity-move-small-cancel",
        false,
        mods(CancelAll),
        |game, _rx, at| {
            guest_walks(game, 0.2);
            let g = &game.lan.as_ref().unwrap().hub.guests[0];
            assert_eq!(
                g.position, at,
                "the cancelled small step did not move the guest"
            );
            assert!(
                guest_corrected(game),
                "the cancelled small step sent the guest a correction"
            );
        },
    );
}

/// A walk's step (0.2) a listener replaces with half (0.1): the decision is
/// not the request, so the correction goes out however small the difference.
#[test]
#[ignore = "needs prepared resources in HORIZON_TEST_RESOURCES"]
fn a_replaced_small_step_sends_the_guest_a_correction() {
    hosted_guest(
        "entity-move-small-replace",
        false,
        mods(ReplaceEach([0.1, 0.0, 0.0])),
        |game, _rx, at| {
            guest_walks(game, 0.2);
            let g = &game.lan.as_ref().unwrap().hub.guests[0];
            assert_eq!(
                g.position, at,
                "the replaced small step did not move the guest"
            );
            assert!(
                guest_corrected(game),
                "the replaced small step sent the guest a correction"
            );
        },
    );
}

/// A replacement equal to the request is no override: the walk's own check
/// decides, and a small step on open floor goes through with no correction.
/// The step is 0.125 — exactly representable, so the listener's write is
/// the request bit for bit; a decimal like 0.2 differs in the last bit of
/// the packet's arithmetic and would be a real replacement.
#[test]
#[ignore = "needs prepared resources in HORIZON_TEST_RESOURCES"]
fn a_replacement_equal_to_the_request_sends_no_correction() {
    hosted_guest(
        "entity-move-small-equal",
        false,
        mods(ReplaceEach([0.125, 0.0, 0.0])),
        |game, _rx, at| {
            assert!(
                !guest_corrected(game),
                "nothing corrected the guest before its step"
            );
            guest_walks(game, 0.125);
            let g = &game.lan.as_ref().unwrap().hub.guests[0];
            assert_eq!(
                g.position,
                [at[0] + 0.125, at[1], at[2]],
                "the equal replacement's step went through"
            );
            assert!(
                !guest_corrected(game),
                "a replacement equal to the request sends no correction"
            );
        },
    );
}

/// A small step with no listener: vanilla's own check, no correction — the
/// fix must not correct ordinary walking.
#[test]
#[ignore = "needs prepared resources in HORIZON_TEST_RESOURCES"]
fn a_small_step_with_no_listener_sends_no_correction() {
    hosted_guest(
        "entity-move-small-plain",
        false,
        Vec::new().into(),
        |game, _rx, at| {
            assert!(
                !guest_corrected(game),
                "nothing corrected the guest before its step"
            );
            guest_walks(game, 0.2);
            let g = &game.lan.as_ref().unwrap().hub.guests[0];
            assert_eq!(
                g.position,
                [at[0] + 0.2, at[1], at[2]],
                "the plain small step went through"
            );
            assert!(
                !guest_corrected(game),
                "a plain small step sends no correction"
            );
        },
    );
}

/// A boat's small move (0.2 east) a listener cancels: the boat keeps its
/// place and the driver's client gets the vehicle put-back — the same ruling
/// at the vehicle Player site, under the quarter-block threshold.
#[test]
#[ignore = "needs prepared resources in HORIZON_TEST_RESOURCES"]
fn a_cancelled_small_vehicle_move_sends_the_driver_a_correction() {
    hosted_guest(
        "entity-move-boat-small-cancel",
        true,
        mods(CancelAll),
        |game, rx, at| {
            driver_moves_vehicle(game, 0.2);
            assert!(
                [
                    game.boats[0].position.x,
                    game.boats[0].position.y,
                    game.boats[0].position.z
                ] == at,
                "the cancelled small vehicle move did not move the boat"
            );
            assert!(
                vehicle_put_back(game, &rx),
                "the cancelled small vehicle move sent the driver a correction"
            );
        },
    );
}

/// A boat's small move a listener replaces with half: the put-back goes to
/// the driver whatever the difference.
#[test]
#[ignore = "needs prepared resources in HORIZON_TEST_RESOURCES"]
fn a_replaced_small_vehicle_move_sends_the_driver_a_correction() {
    hosted_guest(
        "entity-move-boat-small-replace",
        true,
        mods(ReplaceEach([0.1, 0.0, 0.0])),
        |game, rx, at| {
            driver_moves_vehicle(game, 0.2);
            assert!(
                [
                    game.boats[0].position.x,
                    game.boats[0].position.y,
                    game.boats[0].position.z
                ] == at,
                "the replaced small vehicle move did not move the boat"
            );
            assert!(
                vehicle_put_back(game, &rx),
                "the replaced small vehicle move sent the driver a correction"
            );
        },
    );
}

/// A vehicle replacement equal to the request: the boat's own check decides,
/// the small move on open water-floor goes through with no put-back. As
/// above, the step is 0.125 so the listener's write is the request bit for
/// bit.
#[test]
#[ignore = "needs prepared resources in HORIZON_TEST_RESOURCES"]
fn a_vehicle_replacement_equal_to_the_request_sends_no_correction() {
    hosted_guest(
        "entity-move-boat-small-equal",
        true,
        mods(ReplaceEach([0.125, 0.0, 0.0])),
        |game, rx, at| {
            driver_moves_vehicle(game, 0.125);
            assert!(
                [
                    game.boats[0].position.x,
                    game.boats[0].position.y,
                    game.boats[0].position.z
                ] == [at[0] + 0.125, at[1], at[2]],
                "the equal replacement's vehicle move went through"
            );
            assert!(
                !vehicle_put_back(game, &rx),
                "a vehicle replacement equal to the request sends no correction"
            );
        },
    );
}
