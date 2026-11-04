//! A proof mod for `PlayerRef::perform_action`: `/fakeuse` asks for a fake player beside the
//! player; on the next level tick (no player's turn in place) it is made, given an oak log, and
//! places the log two blocks south, clicks the chest east of that, uses its empty hand in the
//! air, and clicks a block out of its reach. Each answer is told to the player.
use mod_api::{
    self, BlockHit, ENTITY_ITEMS, Flow, Hand, Hooks, HostTransaction, ItemResource, ItemStack,
    LevelTickPostEvent, NativeCommand, PerformCommandEvent, PlayerAction, RegisterCommandsEvent,
    ServerHost, Text,
};
use std::sync::Mutex;

/// The player who asked, their dimension and block position.
static ASKED: Mutex<Option<(u128, String, [i32; 3])>> = Mutex::new(None);

#[derive(Default)]
struct Mod;

fn on_top(pos: [i32; 3]) -> PlayerAction {
    let [x, y, z] = pos.map(f64::from);
    PlayerAction::UseOn {
        hand: Hand::Main,
        pos,
        hit: BlockHit::new([x + 0.5, y + 1.0, z + 0.5], [0, 1, 0], false),
    }
}

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register_commands(&self, _: &mut dyn ServerHost, event: &mut RegisterCommandsEvent<'_>) {
        event.commands.push(NativeCommand::new("fakeuse"));
    }

    fn perform_command(&self, _: &mut dyn ServerHost, event: &mut PerformCommandEvent<'_>) -> Flow {
        if event.command != "fakeuse" {
            return Flow::Continue;
        }
        let at = event.player.position.map(|c| c.floor() as i32);
        *ASKED.lock().unwrap() = Some((event.player.uuid, event.player.dimension.clone(), at));
        Flow::Cancel
    }

    fn level_tick_post(&self, host: &mut dyn ServerHost, event: &mut LevelTickPostEvent<'_>) {
        let mut asked = ASKED.lock().unwrap();
        let Some((who, dimension, [x, y, z])) = asked.take_if(|(_, d, _)| d == event.dimension) else {
            return;
        };
        drop(asked);
        let Some(level) = host.level(&dimension) else {
            return;
        };
        let Some(fake) = host.fake_player(level, 0xF00D, "deployer") else {
            host.tell(who, Text::literal("no fake player"));
            return;
        };
        fake.entity()
            .teleport_to(host, [f64::from(x) + 1.5, f64::from(y), f64::from(z) + 0.5], Some([0.0, 0.0]));
        let log = ItemResource::of(&ItemStack::new("minecraft:oak_log", 1));
        let given = match fake.entity().capability(host, &ENTITY_ITEMS, ()) {
            Some(items) => host
                .transaction(|host, tx| Ok::<_, ()>(items.insert_any(host, tx, &log, 1)))
                .unwrap_or(0),
            None => 0,
        };
        let far = [x + 30, y - 1, z];
        let actions = [
            ("place the log", on_top([x + 1, y - 1, z + 2])),
            ("click the chest", on_top([x + 2, y, z + 2])),
            ("empty hand in the air", PlayerAction::UseItem { hand: Hand::Main }),
            ("a block out of reach", on_top(far)),
        ];
        host.tell(who, Text::literal(format!("fake given {given} oak log")));
        for (what, action) in actions {
            let outcome = fake.perform_action(host, action);
            host.tell(who, Text::literal(format!("{what}: {outcome:?}")));
        }
    }
}
