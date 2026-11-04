//! A proof mod for the `LevelRef` redstone reads: `/redproof` prints all five around a rig
//! three blocks south of the player (a stone block with a lever on its north face and a
//! repeater on its east side, facing east into a second stone block).
use mod_api::{
    self, Direction, Hooks, NativeCommand, PerformCommandEvent, RegisterCommandsEvent, ServerHost,
    Text, Flow,
};

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register_commands(&self, _: &mut dyn ServerHost, event: &mut RegisterCommandsEvent<'_>) {
        event.commands.push(NativeCommand::new("redproof"));
    }

    fn perform_command(&self, host: &mut dyn ServerHost, event: &mut PerformCommandEvent<'_>) -> Flow {
        if event.command != "redproof" {
            return Flow::Continue;
        }
        let Some(level) = host.level(&event.player.dimension) else {
            return Flow::Cancel;
        };
        let [x, y, z] = event.player.position.map(|c| c.floor() as i32);
        let stone = [x, y, z + 3];
        let lever = [x, y, z + 2];
        let west = [x - 1, y, z + 3];
        let out = [x + 2, y, z + 3];
        let far = [x - 4, y, z];
        let lines = [
            format!("lever signal N: {:?}", level.signal(host, lever, Direction::North)),
            format!("lever direct N: {:?}", level.direct_signal(host, lever, Direction::North)),
            format!("lever direct E: {:?}", level.direct_signal(host, lever, Direction::East)),
            format!("stone direct_signal_to: {:?}", level.direct_signal_to(host, stone)),
            format!("west of stone best: {:?}", level.best_neighbor_signal(host, west)),
            format!("west of stone has: {}", level.has_neighbor_signal(host, west)),
            format!("repeater's block direct_signal_to: {:?}", level.direct_signal_to(host, out)),
            format!("far best: {:?}, has: {}", level.best_neighbor_signal(host, far), level.has_neighbor_signal(host, far)),
        ];
        for line in lines {
            host.feedback(Text::literal(line));
        }
        Flow::Cancel
    }
}
