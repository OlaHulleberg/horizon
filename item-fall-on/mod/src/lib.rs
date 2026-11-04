//! A proof mod for dropped items reaching `BlockPart::fall_on`: a Part on emerald blocks
//! records each landing (entity, kind, fall, damage) and asks for the block to become a gold
//! block; `/fallseen` prints what it recorded.
use mod_api::{
    self, BlockPart, FallOnEvent, Flow, Hooks, LandingWrite, LevelRandomHost, NativeCommand,
    PerformCommandEvent, RegisterCommandsEvent, Registrar, RegistrationContext, ServerHost, Text,
};
use std::sync::Mutex;

static SEEN: Mutex<Vec<String>> = Mutex::new(Vec::new());

#[derive(Clone, Default)]
struct Emerald;

#[mod_api::horizon_part]
impl BlockPart for Emerald {
    fn fall_on(&self, host: &mut dyn LevelRandomHost, event: &mut FallOnEvent<'_>) {
        let gold = host.block_state_default("minecraft:gold_block");
        SEEN.lock().unwrap().push(format!(
            "entity {} ({}) fell {:.3} onto {:?}, damage {}, gold {}",
            event.entity,
            event.kind,
            event.distance,
            event.pos,
            event.damage,
            gold.is_some()
        ));
        if let Some(gold) = gold {
            event.request(LandingWrite::SetBlock(gold));
        }
    }
}

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        r.block_part("minecraft:emerald_block", Emerald);
    }

    fn register_commands(&self, _: &mut dyn ServerHost, event: &mut RegisterCommandsEvent<'_>) {
        event.commands.push(NativeCommand::new("fallseen"));
    }

    fn perform_command(&self, host: &mut dyn ServerHost, event: &mut PerformCommandEvent<'_>) -> Flow {
        if event.command != "fallseen" {
            return Flow::Continue;
        }
        let seen = SEEN.lock().unwrap().clone();
        if seen.is_empty() {
            host.feedback(Text::literal("fall_on heard nothing"));
        }
        for line in seen {
            host.feedback(Text::literal(line));
        }
        Flow::Cancel
    }
}
