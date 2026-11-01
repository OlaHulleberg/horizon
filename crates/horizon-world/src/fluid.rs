//! Fluid states carried by the pinned vanilla block-state registry.
//! Flow updates, spreading and entity interaction are separate simulation work.
use crate::{
    chunk::BlockState,
    compression::invalid,
    registry::{BlockRegistry, BlockStateId},
};
use std::{io, sync::RwLock};

/// How many mod fluids (NeoForge `FluidType`s with a source and a flowing fluid) a game holds.
pub const MOD_FLUID_SLOTS: usize = 4;

/// A mod's flowing fluid (`BaseFlowingFluid` with its `Properties`): its names, its block (a
/// `LiquidBlock` with `level` 0-15), and how it spreads. Registered by mod content
/// ([`set_mod_fluids`]) before the block tables are built.
#[derive(Clone, Debug, PartialEq)]
pub struct ModFluid {
    /// The source fluid (`create:honey`) and the flowing one (`create:flowing_honey`).
    pub name: &'static str,
    pub flowing_name: &'static str,
    /// Its liquid block (`create:honey`) and its bucket (`create:honey_bucket`).
    pub block: String,
    pub bucket: &'static str,
    /// `tickRate`, `levelDecreasePerBlock`, `slopeFindDistance`.
    pub tick_delay: i32,
    pub drop_off: i32,
    pub slope_find_distance: i32,
    /// `FluidInteractionRegistry` with lava: the block lava next to it becomes (a lava source,
    /// then flowing lava); None when lava ignores it.
    pub lava_source: Option<String>,
    pub lava_flowing: Option<String>,
    /// How it is drawn: block-atlas sprites (`create:block/fluid/honey_still`) and an RGB tint;
    /// in the solid layer, unlit by itself.
    pub still_texture: String,
    pub flowing_texture: String,
    pub tint: u32,
}

static MOD_FLUIDS: RwLock<Vec<ModFluid>> = RwLock::new(Vec::new());

/// Registers the mod fluids (at most [`MOD_FLUID_SLOTS`]; the rest are left out), each taking
/// the next slot of [`FluidKind`]'s mod kinds. Call before any block table is built.
pub fn set_mod_fluids(mut list: Vec<ModFluid>) {
    list.truncate(MOD_FLUID_SLOTS);
    if let Ok(mut fluids) = MOD_FLUIDS.write() {
        *fluids = list;
    }
}

/// The mod fluid in `slot`.
pub fn mod_fluid(slot: usize) -> Option<ModFluid> {
    MOD_FLUIDS.read().ok()?.get(slot).cloned()
}

/// The slot of the mod fluid whose bucket is `item`.
pub fn mod_fluid_of_bucket(item: &str) -> Option<usize> {
    let fluids = MOD_FLUIDS.read().ok()?;
    fluids.iter().position(|f| f.bucket == item)
}

/// The slot of the mod fluid whose block is `block`.
fn mod_fluid_of_block(block: &str) -> Option<usize> {
    let fluids = MOD_FLUIDS.read().ok()?;
    if fluids.is_empty() {
        return None;
    }
    fluids.iter().position(|f| f.block == block)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum FluidKind {
    Empty,
    FlowingWater,
    Water,
    FlowingLava,
    Lava,
    /// The mod fluids' slots ([`set_mod_fluids`]): flowing then source, as vanilla's pairs.
    FlowingMod0,
    Mod0,
    FlowingMod1,
    Mod1,
    FlowingMod2,
    Mod2,
    FlowingMod3,
    Mod3,
}
impl FluidKind {
    /// Vanilla's fluids, in registry order.
    pub const VANILLA: [Self; 5] = [
        Self::Empty,
        Self::FlowingWater,
        Self::Water,
        Self::FlowingLava,
        Self::Lava,
    ];
    pub const ALL: [Self; 13] = [
        Self::Empty,
        Self::FlowingWater,
        Self::Water,
        Self::FlowingLava,
        Self::Lava,
        Self::FlowingMod0,
        Self::Mod0,
        Self::FlowingMod1,
        Self::Mod1,
        Self::FlowingMod2,
        Self::Mod2,
        Self::FlowingMod3,
        Self::Mod3,
    ];
    /// The registry name; a mod slot with no fluid registered is `minecraft:empty`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Empty => "minecraft:empty",
            Self::FlowingWater => "minecraft:flowing_water",
            Self::Water => "minecraft:water",
            Self::FlowingLava => "minecraft:flowing_lava",
            Self::Lava => "minecraft:lava",
            _ => self
                .mod_slot()
                .and_then(mod_fluid)
                .map_or("minecraft:empty", |f| {
                    if self.is_flowing() {
                        f.flowing_name
                    } else {
                        f.name
                    }
                }),
        }
    }
    /// The mod fluid slot of a mod kind.
    pub const fn mod_slot(self) -> Option<usize> {
        let i = self as usize;
        if i >= Self::FlowingMod0 as usize {
            Some((i - Self::FlowingMod0 as usize) / 2)
        } else {
            None
        }
    }
    /// A mod slot's source kind (`flowing` false) or flowing kind.
    pub const fn of_mod_slot(slot: usize, flowing: bool) -> Self {
        Self::ALL[Self::FlowingMod0 as usize + 2 * slot + if flowing { 0 } else { 1 }]
    }
    pub const fn is_flowing(self) -> bool {
        match self {
            Self::FlowingWater | Self::FlowingLava => true,
            _ => match self.mod_slot() {
                Some(_) => (self as usize - Self::FlowingMod0 as usize) % 2 == 0,
                None => false,
            },
        }
    }
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }
    pub const fn same_family(self, other: Self) -> bool {
        if let (Some(a), Some(b)) = (self.mod_slot(), other.mod_slot()) {
            return a == b;
        }
        matches!(
            (self, other),
            (Self::Empty, Self::Empty)
                | (
                    Self::Water | Self::FlowingWater,
                    Self::Water | Self::FlowingWater
                )
                | (
                    Self::Lava | Self::FlowingLava,
                    Self::Lava | Self::FlowingLava
                )
        )
    }
    /// Its bit in a fluid tag's mask: vanilla's fluids only (mod fluids are in no tag here).
    pub const fn mask(self) -> u8 {
        if self.mod_slot().is_some() {
            0
        } else {
            1 << self as u8
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FluidState {
    kind: FluidKind,
    amount: u8,
    falling: bool,
}
impl FluidState {
    /// Native fluid states require an object; properties decode leniently.
    pub fn from_json(json: &str) -> io::Result<Self> {
        let value: serde_json::Value =
            serde_json::from_str(json).map_err(|e| invalid(e.to_string()))?;
        let name = value
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| invalid("missing fluid identifier"))?;
        let (namespace, path) = name.split_once(':').unwrap_or(("minecraft", name));
        let name = format!(
            "{}:{path}",
            if namespace.is_empty() {
                "minecraft"
            } else {
                namespace
            }
        );
        let kind =
            FluidKind::from_name(&name).ok_or_else(|| invalid("unknown fluid identifier"))?;
        if kind == FluidKind::Empty {
            return Ok(Self::EMPTY);
        }
        let flowing = kind.is_flowing();
        let mut state = Self {
            kind,
            amount: if flowing { 1 } else { 8 },
            falling: true,
        };
        if let Some(properties) = value
            .get("properties")
            .and_then(serde_json::Value::as_object)
        {
            if let Some(value) = properties
                .get("falling")
                .and_then(serde_json::Value::as_str)
            {
                match value {
                    "true" => state.falling = true,
                    "false" => state.falling = false,
                    _ => {}
                }
            }
            if flowing {
                if let Some(value) = properties
                    .get("level")
                    .and_then(serde_json::Value::as_str)
                    .and_then(crate::registry::java_integer)
                    .filter(|v| (1..=8).contains(v))
                {
                    state.amount = value as u8;
                }
            }
        }
        Ok(state)
    }
    pub fn to_json(self) -> io::Result<String> {
        let mut value = serde_json::json!({"id": self.kind.name()});
        if self.kind != FluidKind::Empty {
            let mut properties = serde_json::json!({"falling": self.falling.to_string()});
            if !self.is_source() {
                properties["level"] = self.amount.to_string().into();
            }
            value["properties"] = properties;
        }
        serde_json::to_string(&value).map_err(|e| invalid(e.to_string()))
    }
    /// A fluid state from its parts; the empty fluid ignores amount and falling.
    pub const fn new(kind: FluidKind, amount: u8, falling: bool) -> Self {
        if matches!(kind, FluidKind::Empty) {
            return Self::EMPTY;
        }
        Self {
            kind,
            amount,
            falling,
        }
    }
    pub const EMPTY: Self = Self {
        kind: FluidKind::Empty,
        amount: 0,
        falling: false,
    };
    pub const WATER: Self = Self {
        kind: FluidKind::Water,
        amount: 8,
        falling: false,
    };
    pub const fn kind(self) -> FluidKind {
        self.kind
    }
    pub const fn amount(self) -> u8 {
        self.amount
    }
    pub const fn falling(self) -> bool {
        self.falling
    }
    pub const fn is_source(self) -> bool {
        !matches!(self.kind, FluidKind::Empty) && !self.kind.is_flowing()
    }
    pub const fn is_full(self) -> bool {
        self.amount == 8
    }
    pub fn height_at<W: FluidHeightWorld>(
        self,
        world: &mut W,
        position: [i32; 3],
    ) -> Result<f32, W::Error> {
        if self.kind == FluidKind::Empty {
            return Ok(0.0);
        }
        let above = [position[0], position[1].wrapping_add(1), position[2]];
        Ok(if self.kind.same_family(world.fluid(above)?.kind) {
            1.0
        } else {
            self.own_height()
        })
    }
    pub fn camera_height_at<W: FluidHeightWorld>(
        self,
        world: &mut W,
        position: [i32; 3],
    ) -> Result<f32, W::Error> {
        if self.is_source() {
            let above = [position[0], position[1].wrapping_add(1), position[2]];
            if world.sturdy_down(above)? {
                return Ok(1.0);
            }
        }
        self.height_at(world, position)
    }
    pub fn own_height(self) -> f32 {
        f32::from(self.amount) / 9.0
    }
    pub fn from_block(state: &BlockState) -> io::Result<Self> {
        let water = state.name == "minecraft:water";
        let modded = if water || state.name.starts_with("minecraft:") {
            None
        } else {
            mod_fluid_of_block(&state.name)
        };
        if water || state.name == "minecraft:lava" || modded.is_some() {
            let level = state
                .properties
                .get("level")
                .and_then(|v| v.parse::<u8>().ok())
                .filter(|v| *v <= 15)
                .ok_or_else(|| invalid("invalid liquid block level"))?;
            let kind = match (water, level == 0) {
                _ if modded.is_some() => FluidKind::of_mod_slot(modded.unwrap_or(0), level != 0),
                (true, true) => FluidKind::Water,
                (true, false) => FluidKind::FlowingWater,
                (false, true) => FluidKind::Lava,
                (false, false) => FluidKind::FlowingLava,
            };
            return Ok(Self {
                kind,
                amount: if level == 0 || level >= 8 {
                    8
                } else {
                    8 - level
                },
                falling: level >= 8,
            });
        }
        if matches!(
            state.name.as_str(),
            "minecraft:kelp"
                | "minecraft:kelp_plant"
                | "minecraft:seagrass"
                | "minecraft:tall_seagrass"
                | "minecraft:bubble_column"
        ) {
            return Ok(Self::WATER);
        }
        match state.properties.get("waterlogged").map(String::as_str) {
            Some("true") => {
                // The pinned copper-grate blocks use falling source water.
                let falling = matches!(
                    state.name.as_str(),
                    "minecraft:copper_grate"
                        | "minecraft:exposed_copper_grate"
                        | "minecraft:weathered_copper_grate"
                        | "minecraft:oxidized_copper_grate"
                        | "minecraft:waxed_copper_grate"
                        | "minecraft:waxed_exposed_copper_grate"
                        | "minecraft:waxed_weathered_copper_grate"
                        | "minecraft:waxed_oxidized_copper_grate"
                );
                Ok(Self {
                    falling,
                    ..Self::WATER
                })
            }
            None | Some("false") => Ok(Self::EMPTY),
            _ => Err(invalid("invalid waterlogged block property")),
        }
    }
}
/// Queries required by fluid surface height and camera immersion. The caller
/// owns block shape/support behavior and world-query failures.
pub trait FluidHeightWorld {
    type Error;
    fn fluid(&mut self, position: [i32; 3]) -> Result<FluidState, Self::Error>;
    fn sturdy_down(&mut self, position: [i32; 3]) -> Result<bool, Self::Error>;
}
#[derive(Debug)]
pub struct FluidStateTable(Vec<FluidState>);
impl FluidStateTable {
    pub fn new(registry: &BlockRegistry) -> io::Result<Self> {
        (0..registry.len())
            .map(|i| FluidState::from_block(registry.state(BlockStateId(i as u32)).unwrap()))
            .collect::<io::Result<Vec<_>>>()
            .map(Self)
    }
    pub fn get(&self, state: BlockStateId) -> Option<FluidState> {
        self.0.get(state.0 as usize).copied()
    }
}

/// Bound identities for converting fluid states into their legacy liquid blocks.
pub struct FluidBlockBindings {
    air: BlockStateId,
    water: [BlockStateId; 16],
    lava: [BlockStateId; 16],
    /// The mod fluids' blocks by slot, when the registry has them.
    modded: [Option<[BlockStateId; 16]>; MOD_FLUID_SLOTS],
}
impl FluidBlockBindings {
    pub fn new(registry: &BlockRegistry) -> io::Result<Self> {
        let levels = |name: &str| -> io::Result<[BlockStateId; 16]> {
            let default = registry
                .default_state(name)
                .ok_or_else(|| invalid("missing liquid block"))?;
            let mut state = registry.state(default).unwrap().clone();
            let mut result = [default; 16];
            for (level, id) in result.iter_mut().enumerate() {
                state.properties.insert("level".into(), level.to_string());
                *id = registry
                    .id(&state)
                    .ok_or_else(|| invalid("missing liquid block level"))?;
            }
            Ok(result)
        };
        Ok(Self {
            air: registry
                .default_state("minecraft:air")
                .ok_or_else(|| invalid("missing air block"))?,
            water: levels("minecraft:water")?,
            lava: levels("minecraft:lava")?,
            modded: std::array::from_fn(|slot| mod_fluid(slot).and_then(|f| levels(&f.block).ok())),
        })
    }
    pub fn block(&self, state: FluidState) -> BlockStateId {
        if state.kind == FluidKind::Empty {
            return self.air;
        }
        let level = if state.is_source() {
            0
        } else {
            8 - state.amount.min(8) + if state.falling { 8 } else { 0 }
        } as usize;
        if let Some(slot) = state.kind.mod_slot() {
            return self.modded[slot].map_or(self.air, |levels| levels[level]);
        }
        if state.kind.same_family(FluidKind::Water) {
            self.water[level]
        } else {
            self.lava[level]
        }
    }
}

#[cfg(test)]
mod mod_fluid_tests {
    use super::*;

    fn honey() -> ModFluid {
        ModFluid {
            name: "test:honey",
            flowing_name: "test:flowing_honey",
            block: "test:honey".into(),
            bucket: "test:honey_bucket",
            tick_delay: 25,
            drop_off: 2,
            slope_find_distance: 3,
            lava_source: Some("minecraft:obsidian".into()),
            lava_flowing: None,
            still_texture: "test:block/honey_still".into(),
            flowing_texture: "test:block/honey_flow".into(),
            tint: 0xFF_FFFF,
        }
    }

    #[test]
    fn a_mod_fluid_block_is_its_slot_s_source_or_flowing_fluid() {
        set_mod_fluids(vec![honey()]);
        let block = |level: u8| BlockState {
            name: "test:honey".into(),
            properties: [("level".to_string(), level.to_string())].into(),
        };
        let source = FluidState::from_block(&block(0)).unwrap();
        assert_eq!(source.kind(), FluidKind::Mod0);
        assert!(source.is_source() && source.amount() == 8);
        let flowing = FluidState::from_block(&block(3)).unwrap();
        assert_eq!(
            (flowing.kind(), flowing.amount()),
            (FluidKind::FlowingMod0, 5)
        );
        assert!(!flowing.is_source() && flowing.kind().is_flowing());
        let falling = FluidState::from_block(&block(8)).unwrap();
        assert!(falling.falling() && falling.amount() == 8);
        // Its own family only; in no vanilla fluid tag.
        assert!(FluidKind::Mod0.same_family(FluidKind::FlowingMod0));
        assert!(!FluidKind::Mod0.same_family(FluidKind::Mod1));
        assert!(!FluidKind::Mod0.same_family(FluidKind::Water));
        assert_eq!(FluidKind::Mod0.mask(), 0);
        assert_eq!(
            (FluidKind::Mod0.name(), FluidKind::FlowingMod0.name()),
            ("test:honey", "test:flowing_honey")
        );
        assert_eq!(
            FluidKind::from_name("test:flowing_honey"),
            Some(FluidKind::FlowingMod0)
        );
        assert_eq!(FluidKind::of_mod_slot(1, false), FluidKind::Mod1);
        assert_eq!(mod_fluid_of_bucket("test:honey_bucket"), Some(0));
        assert_eq!(mod_fluid_of_bucket("minecraft:water_bucket"), None);
        assert_eq!(FluidKind::Mod3.mod_slot(), Some(3));
        assert_eq!(FluidKind::Lava.mod_slot(), None);
        // Vanilla's liquids read as before.
        let water = BlockState {
            name: "minecraft:water".into(),
            properties: [("level".to_string(), "0".to_string())].into(),
        };
        assert_eq!(FluidState::from_block(&water).unwrap(), FluidState::WATER);
    }
}
