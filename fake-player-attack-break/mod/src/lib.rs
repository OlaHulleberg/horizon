//! A proof mod for `PlayerAction::Attack` and `BreakBlock`: `/fakehit` asks for a fake player
//! beside the player; on the next level tick (no player's turn in place) it is made and given
//! an iron pickaxe. It hits the zombie two blocks east of the player, attacks itself, breaks
//! the stone below the block east of the player, and a block out of its reach. Each answer,
//! and the zombie's health around the hit, is told to the player.
use mod_api::{
    self, ENTITY_ITEMS, Flow, Hooks, HostTransaction, ItemResource, ItemStack, LevelTickPostEvent,
    LivingRef, NativeCommand, PerformCommandEvent, PlayerAction, RegisterCommandsEvent,
    ServerHost, Text,
};
use std::sync::Mutex;

/// The player who asked, their dimension and block position.
static ASKED: Mutex<Option<(u128, String, [i32; 3])>> = Mutex::new(None);

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register_commands(&self, _: &mut dyn ServerHost, event: &mut RegisterCommandsEvent<'_>) {
        event.commands.push(NativeCommand::new("fakehit"));
    }

    fn perform_command(&self, _: &mut dyn ServerHost, event: &mut PerformCommandEvent<'_>) -> Flow {
        if event.command != "fakehit" {
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
        let Some(fake) = host.fake_player(level, 0xF00D, "breaker") else {
            host.tell(who, Text::literal("no fake player"));
            return;
        };
        fake.entity()
            .teleport_to(host, [f64::from(x) + 1.5, f64::from(y), f64::from(z) + 0.5], Some([-90.0, 0.0]));
        let pick = ItemResource::of(&ItemStack::new("minecraft:iron_pickaxe", 1));
        if let Some(items) = fake.entity().capability(host, &ENTITY_ITEMS, ()) {
            let _ = host.transaction(|host, tx| Ok::<_, ()>(items.insert_any(host, tx, &pick, 1)));
        }
        let zombie = level
            .entities_in(&*host, [f64::from(x) + 2.0, f64::from(y), f64::from(z)], [f64::from(x) + 4.0, f64::from(y) + 2.0, f64::from(z) + 1.0])
            .unwrap_or_default()
            .into_iter()
            .find(|e| e.kind(&*host).as_deref() == Some("minecraft:zombie"));
        let Some(zombie) = zombie else {
            host.tell(who, Text::literal("no zombie two blocks east"));
            return;
        };
        let health = |host: &dyn ServerHost| LivingRef::new(zombie).health(host);
        let before = health(&*host);
        let hit = fake.perform_action(host, PlayerAction::Attack { target: zombie });
        let after = health(&*host);
        host.tell(who, Text::literal(format!("hit the zombie: {hit:?}, health {before:?} -> {after:?}")));
        let own = fake.perform_action(host, PlayerAction::Attack { target: fake.entity() });
        host.tell(who, Text::literal(format!("attack itself: {own:?}")));
        let broke = fake.perform_action(host, PlayerAction::BreakBlock { pos: [x + 1, y - 1, z] });
        host.tell(who, Text::literal(format!("break the stone under it: {broke:?}")));
        let far = fake.perform_action(host, PlayerAction::BreakBlock { pos: [x + 30, y - 1, z] });
        host.tell(who, Text::literal(format!("a block out of reach: {far:?}")));
    }
}
