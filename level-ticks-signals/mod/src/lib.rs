//! A proof mod for the `LevelTicksHost` redstone reads: glass that acts like a diode. When a
//! neighbour changes it reads its input in `neighbor_changed` and schedules its own tick 2
//! ticks later, as `DiodeBlock.checkTickOnNeighbor` does; the tick turns it white while it is
//! powered and clear again when it is not.
use mod_api::{
    self, BlockPart, Flow, Hooks, LevelTicksHost, LevelWriteHost, NativeBlockUpdateFlags,
    NeighborChangedEvent, Registrar, RegistrationContext, TickBlockEvent,
};

const CLEAR: &str = "minecraft:glass";
const LIT: &str = "minecraft:white_stained_glass";

#[derive(Default)]
struct Mod;

struct DiodeGlass;

#[mod_api::horizon_part]
impl BlockPart for DiodeGlass {
    fn neighbor_changed(&self, level: &mut dyn LevelTicksHost, event: &mut NeighborChangedEvent) {
        let pos = [event.pos.x, event.pos.y, event.pos.z];
        let lit = level.state_block(event.state) == Some(LIT);
        if level.has_neighbor_signal(pos) != lit {
            let block = level.state_block(event.state).unwrap_or(CLEAR).to_owned();
            level.schedule_block_tick(pos, &block, 2);
        }
    }
    fn tick_block(&self, level: &mut dyn LevelWriteHost, event: &mut TickBlockEvent) -> Flow {
        let pos = [event.pos.x, event.pos.y, event.pos.z];
        let want = if level.has_neighbor_signal(pos) { LIT } else { CLEAR };
        if level.state_block(event.state) != Some(want)
            && let Some(state) = level.block_state_default(want)
        {
            level.set_block(pos, state, NativeBlockUpdateFlags::NEIGHBORS | NativeBlockUpdateFlags::CLIENTS);
        }
        Flow::Continue
    }
}

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        r.block_part(CLEAR, DiodeGlass);
        r.block_part(LIT, DiodeGlass);
    }
}
