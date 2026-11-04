//! A proof mod for `LevelRef::update_neighbors`: `/nupdate` tells the neighbours of the block
//! two blocks south of the player (a powered lever placed with `/setblock … strict`, which
//! updates nothing) that it changed, and reports the lamps beside it before and after.
use mod_api::{
    self, Flow, Hooks, NativeCommand, NeighbourUpdate, PerformCommandEvent, RegisterCommandsEvent,
    ServerHost, Text,
};

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register_commands(&self, _: &mut dyn ServerHost, event: &mut RegisterCommandsEvent<'_>) {
        event.commands.push(NativeCommand::new("nupdate"));
    }

    fn perform_command(&self, host: &mut dyn ServerHost, event: &mut PerformCommandEvent<'_>) -> Flow {
        if event.command != "nupdate" {
            return Flow::Continue;
        }
        let Some(level) = host.level(&event.player.dimension) else {
            return Flow::Cancel;
        };
        let [x, y, z] = event.player.position.map(|c| c.floor() as i32);
        let lever = [x, y, z + 2];
        let lamps = [[x - 1, y, z + 2], [x + 1, y, z + 2]];
        let lit = |host: &mut dyn ServerHost| {
            lamps.map(|pos| level.block_property(host, pos, "lit").unwrap_or_default())
        };
        let before = lit(host);
        let Some(state) = level.block_state(host, lever) else {
            return Flow::Cancel;
        };
        let updated = level.update_neighbors(host, lever, state, NeighbourUpdate::Neighbours);
        let after = lit(host);
        host.feedback(Text::literal(format!("lamps lit before: {before:?}")));
        host.feedback(Text::literal(format!("update_neighbors(Neighbours): {updated}")));
        host.feedback(Text::literal(format!("lamps lit after: {after:?}")));
        Flow::Cancel
    }
}
