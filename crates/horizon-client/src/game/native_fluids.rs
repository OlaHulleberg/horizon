//! Fluids for native mods (hook points API 44): the flow of the fluid at a block, as vanilla's
//! `FluidState.getFlow` gives it, for machines a current turns (a water wheel).
use super::Game;
use horizon_core::{geometry::Direction, position::BlockPos};
use horizon_world::{
    fluid::FluidState,
    fluid_flow::{FluidFlowWorld, flow_at},
    registry::BlockStateId,
};

/// The level's blocks as the flow computation reads them.
struct FlowWorld<'a> {
    world: &'a horizon_client::world::World,
    assets: &'a horizon_client::assets::Assets,
    facts: &'a horizon_client::blocks::BlockFacts,
}

impl FlowWorld<'_> {
    fn state(&self, p: BlockPos) -> BlockStateId {
        self.world.block(p.x, p.y, p.z)
    }
}

impl FluidFlowWorld for FlowWorld<'_> {
    type Error = std::io::Error;
    type Block = BlockStateId;
    fn fluid(&mut self, p: BlockPos) -> std::io::Result<FluidState> {
        FluidState::from_block(
            self.assets
                .registry
                .state(self.state(p))
                .ok_or_else(|| std::io::Error::other("unknown block state"))?,
        )
    }
    fn block(&mut self, p: BlockPos) -> std::io::Result<Self::Block> {
        Ok(self.state(p))
    }
    fn blocks_fluid_flow(&self, s: Self::Block) -> bool {
        self.assets
            .registry
            .state(s)
            .is_some_and(|b| self.assets.blocks_fluid_flow.contains(&b.name))
    }
    fn is_ice_class(&self, s: Self::Block) -> bool {
        self.facts
            .get(s)
            .classes
            .iter()
            .any(|c| c.as_ref() == "IceBlock")
    }
    fn is_face_sturdy(
        &mut self,
        s: Self::Block,
        _: BlockPos,
        d: Direction,
    ) -> std::io::Result<bool> {
        Ok(self.facts.get(s).sturdy_full & (1 << d as u8) != 0)
    }
}

/// The flow at `pos` of `world`'s fluid (zero where there is none).
fn flow(
    world: &horizon_client::world::World,
    assets: &horizon_client::assets::Assets,
    facts: &horizon_client::blocks::BlockFacts,
    pos: [i32; 3],
) -> Option<[f64; 3]> {
    let mut w = FlowWorld {
        world,
        assets,
        facts,
    };
    let at = BlockPos::new(pos[0], pos[1], pos[2]);
    let fluid = w.fluid(at).ok()?;
    let v = flow_at(&mut w, fluid, at).ok()?;
    Some([v.x, v.y, v.z])
}

/// Helpers for server mods, kept as stable as the hook points.
#[allow(dead_code)]
impl Game<'_> {
    /// The flow of the fluid at `pos` in the level `dimension` (`FluidState.getFlow`): a
    /// horizontal vector pointing downstream, of length 1 where the fluid flows, with -1 in y
    /// where it falls; zero for still water or no fluid. None for another level than the one in
    /// place, a position that isn't loaded, or off an authoritative server. API 44.
    pub(super) fn native_fluid_flow(&self, dimension: &str, pos: [i32; 3]) -> Option<[f64; 3]> {
        if (self.serving.is_none() && self.options.dedicated.is_none())
            || dimension != self.dim.id()
            || !self.world.chunks.contains_key(&(pos[0] >> 4, pos[2] >> 4))
        {
            return None;
        }
        flow(&self.world, &self.pack.assets, self.res.facts(), pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_world::chunk::BlockState;

    #[test]
    fn water_flows_away_from_its_source() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/horizon-client")
            .join(horizon_core::VANILLA_VERSION);
        if !root.join("facts.json").exists() {
            return;
        }
        let assets = horizon_client::assets::Assets::load(&root).unwrap();
        let facts = horizon_client::blocks::BlockFacts::load(&root, assets.registry).unwrap();
        let water = |level: &str| {
            assets
                .registry
                .id(&BlockState {
                    name: "minecraft:water".into(),
                    properties: [("level".to_owned(), level.to_owned())]
                        .into_iter()
                        .collect(),
                })
                .unwrap()
        };
        let stone = assets.registry.default_state("minecraft:stone").unwrap();
        let mut world = horizon_client::world::World::new(BlockStateId(0), 0);
        let mut chunk = horizon_client::chunk::Chunk::empty(0, 0);
        for x in 0..16 {
            for z in 0..16 {
                chunk.set(x, 63, z, stone.0);
            }
        }
        // A source at x 4, then water spreading east: levels 1, 2.
        chunk.set(4, 64, 8, water("0").0);
        chunk.set(5, 64, 8, water("1").0);
        chunk.set(6, 64, 8, water("2").0);
        world.chunks.insert((0, 0), std::sync::Arc::new(chunk));
        let east = flow(&world, &assets, &facts, [5, 64, 8]).unwrap();
        assert!(
            east[0] > 0.9 && east[2].abs() < 1e-9 && east[1] == 0.0,
            "{east:?}"
        );
        let still = flow(&world, &assets, &facts, [10, 64, 8]).unwrap();
        assert_eq!(still, [0.0, 0.0, 0.0], "no fluid");
    }
}
