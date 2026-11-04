//! A proof mod for `host.block_states()`: `/statefacts` prints the facts of a few blocks.
use mod_api::{
    self, Flow, Hooks, HostBlockStates, NativeCommand, PerformCommandEvent, RegisterCommandsEvent,
    ServerHost, Text,
};

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register_commands(&self, _: &mut dyn ServerHost, event: &mut RegisterCommandsEvent<'_>) {
        event.commands.push(NativeCommand::new("statefacts"));
    }

    fn perform_command(&self, host: &mut dyn ServerHost, event: &mut PerformCommandEvent<'_>) -> Flow {
        if event.command != "statefacts" {
            return Flow::Continue;
        }
        for block in ["minecraft:stone", "minecraft:obsidian", "minecraft:short_grass", "minecraft:chest", "minecraft:oak_stairs"] {
            let Some(state) = host.block_state_default(block) else {
                continue;
            };
            let facts = host.block_states();
            let line = format!(
                "{block}: replaceable {:?}, destroy_speed {:?}, block entity {:?}, break sound {:?}, states {:?}",
                facts.replaceable(state).unwrap_or_default(),
                facts.destroy_speed(state).unwrap_or_default(),
                facts.has_block_entity(state).unwrap_or_default(),
                facts.sound(state).map(|s| s.break_sound).unwrap_or_default(),
                facts.states_of(block).map(|r| r.len()).unwrap_or_default(),
            );
            host.feedback(Text::literal(line));
        }
        Flow::Cancel
    }
}
