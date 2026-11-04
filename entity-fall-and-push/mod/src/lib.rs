//! A proof mod for `EntityRef::reset_fall_distance` and `MinecartRef::set_push`:
//! `/fallreset` forgets the player's fall mid-air; `/cartpush` fuels the furnace minecarts
//! within 5 blocks and pushes them south.
use mod_api::{
    self, Flow, Hooks, NativeCommand, PerformCommandEvent, RegisterCommandsEvent, ServerHost, Text,
};

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register_commands(&self, _: &mut dyn ServerHost, event: &mut RegisterCommandsEvent<'_>) {
        event.commands.push(NativeCommand::new("fallreset"));
        event.commands.push(NativeCommand::new("cartpush"));
    }

    fn perform_command(&self, host: &mut dyn ServerHost, event: &mut PerformCommandEvent<'_>) -> Flow {
        match event.command {
            "fallreset" => {
                let done = host
                    .player(event.player.uuid)
                    .is_some_and(|player| player.entity().reset_fall_distance(host));
                host.feedback(Text::literal(format!("reset_fall_distance: {done}")));
                Flow::Cancel
            }
            "cartpush" => {
                let Some(level) = host.level(&event.player.dimension) else {
                    return Flow::Cancel;
                };
                let [x, y, z] = event.player.position;
                let near = level
                    .entities_in(host, [x - 5.0, y - 5.0, z - 5.0], [x + 5.0, y + 5.0, z + 5.0])
                    .unwrap_or_default();
                for entity in near {
                    if let Some(cart) = host.minecart(entity) {
                        let fueled = cart.set_fuel(host, 3600);
                        let pushed = cart.set_push(host, [0.0, 0.0, 1.0]);
                        host.feedback(Text::literal(format!(
                            "cart {}: set_fuel {fueled}, set_push {pushed}, push now {:?}",
                            entity.id(),
                            cart.push(host)
                        )));
                    }
                }
                Flow::Cancel
            }
            _ => Flow::Continue,
        }
    }
}
