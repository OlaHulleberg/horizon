//! The Player-cause sites of `entity_move` (#2414): a cancelling listener on
//! the guest's position packet turns the move into «moved wrongly», and the
//! hub sends the guest back where it was — the correction the ruling asks
//! for. Built on the real hub: a hosted guest, a `MovePlayerPos` packet in
//! its inbox, the hub's own collide path.
use crate::game::game_tests::tests::server_game;
use horizon_client::lan::guest::InboxItem;
use horizon_client::lan::{Joined, NetConfig, PlayerSender, guest::Guest, hub::HubConfig};
use horizon_core::position::ChunkPos;
use horizon_protocol::packets::configuration::ClientInformation;
use horizon_protocol::packets::play::{Clientbound as C, Serverbound as S};
use mod_api::{EntityMoveEvent, Flow, Hooks, LoopContext, MoveCause};
use std::sync::atomic::AtomicI32;
use std::sync::{Arc, Mutex};

struct CancelAll(Arc<Mutex<Vec<MoveCause>>>);

#[mod_api::__subscriptions]
impl Hooks for CancelAll {
    fn entity_move(&self, _: &dyn LoopContext, event: &mut EntityMoveEvent<'_>) -> Flow {
        self.0.lock().unwrap().push(event.cause);
        Flow::Cancel
    }
}

/// A hosted guest whose `MovePlayerPos` a cancelling listener refuses: the
/// guest keeps its position, the hook heard the player cause, and a
/// `PlayerPosition` correction went out (`awaiting_teleport` is set).
#[test]
#[ignore = "needs prepared resources in HORIZON_TEST_RESOURCES"]
fn a_cancelled_player_packet_sends_the_guest_a_correction() {
    let causes = Arc::new(Mutex::new(Vec::new()));
    let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
        "refuser",
        Box::new(CancelAll(causes.clone())),
    )]
    .into();
    mod_api::testing::with_mods(mods, || {
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
        game.lan_host(
            crate::game::lan::LanSettings {
                port: 0,
                guest_commands: false,
                force_game_mode: false,
            },
            NetConfig {
                listen: false,
                announce: false,
                ..NetConfig::loopback(0, "entity-move-unit".into(), 2)
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
        let (tx, _rx) = std::sync::mpsc::channel();
        let guest_entity_id = game.take_entity_id() as i32;
        let mut guest = Guest::new(
            Joined {
                conn: 1,
                uuid: 1,
                name: "Mover".into(),
                properties: Vec::new(),
                info: ClientInformation::new(2),
                brand: "unit".into(),
                sender: PlayerSender::new(tx, Arc::default()),
                latency: Arc::new(AtomicI32::new(0)),
                address: "127.0.0.1:1".parse().unwrap(),
                expected: false,
            },
            guest_entity_id,
            [0.5, 100.0, 0.5],
            2,
        );
        guest.placed = true;
        guest.loaded = true;
        guest.load_timeout = 0;
        guest.dimension = "minecraft:overworld".into();
        game.lan.as_mut().unwrap().hub.guests.push(guest);
        game.sync_tick_chunks();
        game.ticked_entities_through = game.next_entity;
        // The guest walks two blocks east; the listener cancels every move.
        let before = game.lan.as_ref().unwrap().hub.guests[0].position;
        game.lan.as_mut().unwrap().hub.guests.update(0, |g| {
            g.inbox.push_back(InboxItem::Packet(
                S::MovePlayerPos(horizon_protocol::packets::play::MovePlayerPos {
                    x: before[0] + 2.0,
                    y: before[1],
                    z: before[2],
                    flags: 1,
                }),
                None,
            ));
        });
        game.tick_lan_packets();
        let host = game.lan.as_ref().unwrap();
        let g = &host.hub.guests[0];
        // The cancelled move left the guest where it was, and the correction
        // (a teleport the client must acknowledge) is pending.
        assert_eq!(
            g.position, before,
            "the cancelled packet did not move the guest"
        );
        assert!(
            g.awaiting_teleport.is_some(),
            "the refused move sent the guest a correction"
        );
        // The hook heard the move with the player cause.
        let heard = std::mem::take(&mut *causes.lock().unwrap());
        assert_eq!(
            heard,
            vec![MoveCause::Player],
            "the packet's move fires once, with the player cause"
        );
        let _ = C::PlayerPosition(horizon_protocol::packets::play::PlayerPosition {
            teleport: 0,
            position: [0.0; 3],
            delta: [0.0; 3],
            yaw: 0.0,
            pitch: 0.0,
            relatives: 0,
        });
    });
}
