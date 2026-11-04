//! A proof mod for `level.craft(host, width, height, &items)`: `/craftread` asks what a few
//! grids make and prints each output with what each slot keeps.
use mod_api::{
    self, Flow, Hooks, ItemStack, NativeCommand, PerformCommandEvent, RegisterCommandsEvent,
    ServerHost, Text,
};

#[derive(Default)]
struct Mod;

fn stack(item: &str) -> Option<ItemStack> {
    Some(ItemStack::new(&format!("minecraft:{item}"), 1))
}

fn short(stack: &ItemStack) -> String {
    format!("{} {}", stack.count, stack.item.trim_start_matches("minecraft:"))
}

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register_commands(&self, _: &mut dyn ServerHost, event: &mut RegisterCommandsEvent<'_>) {
        event.commands.push(NativeCommand::new("craftread"));
    }

    fn perform_command(&self, host: &mut dyn ServerHost, event: &mut PerformCommandEvent<'_>) -> Flow {
        if event.command != "craftread" {
            return Flow::Continue;
        }
        let Some(level) = host.level(&event.player.dimension) else {
            return Flow::Cancel;
        };
        let mut cake = vec![stack("milk_bucket"); 3];
        cake.extend([stack("sugar"), stack("egg"), stack("sugar")]);
        cake.extend(vec![stack("wheat"); 3]);
        let mut corner = vec![None; 9];
        corner[5] = stack("birch_planks");
        corner[8] = stack("birch_planks");
        let grids = [
            ("cake 3x3", 3, 3, cake),
            ("planks in a 3x3 corner", 3, 3, corner),
            ("paper+gunpowder 2x1", 2, 1, vec![stack("paper"), stack("gunpowder")]),
            ("dirt 1x1", 1, 1, vec![stack("dirt")]),
        ];
        for (name, width, height, grid) in grids {
            let line = match level.craft(&*host, width, height, &grid) {
                Some(made) => {
                    let kept: Vec<String> = made
                        .remainders
                        .iter()
                        .enumerate()
                        .filter_map(|(i, r)| r.as_ref().map(|r| format!("{i}:{}", short(r))))
                        .collect();
                    format!("{name} -> {}; keeps [{}]", short(&made.output), kept.join(", "))
                }
                None => format!("{name} -> None"),
            };
            host.feedback(Text::literal(line));
        }
        Flow::Cancel
    }
}
