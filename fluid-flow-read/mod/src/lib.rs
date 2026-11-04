//! A proof mod for `level.fluid_flow(host, pos)`. A `fluid_flow` listener reverses the current
//! at every `x` that is a multiple of 8. `/flowread x y z` prints the server's read of the eight
//! blocks east of `x y z`; the chat line `flowc x y z` is rewritten on the client with the
//! client's own read of the same blocks.
use mod_api::{
    self, ClientChatEvent, ClientHost, Flow, FluidFlowEvent, Host, Hooks, NativeCommand,
    PerformCommandEvent, RegisterCommandsEvent, ServerHost, Text,
};

#[derive(Default)]
struct Mod;

fn coords(words: &[&str]) -> Option<[i32; 3]> {
    let n: Vec<i32> = words.iter().filter_map(|w| w.parse().ok()).collect();
    (n.len() == 3).then(|| [n[0], n[1], n[2]])
}

fn read_row(host: &dyn Host, [x, y, z]: [i32; 3]) -> String {
    let Some(level) = host.level("minecraft:overworld") else {
        return "no level".into();
    };
    (0..8)
        .map(|dx| match level.fluid_flow(host, [x + dx, y, z]) {
            Some(v) => format!("{}:({:.2},{:.2},{:.2})", x + dx, v.x, v.y, v.z),
            None => format!("{}:None", x + dx),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn fluid_flow(&self, event: &mut FluidFlowEvent<'_>) {
        if event.pos.x.rem_euclid(8) == 0 {
            for c in event.flow.iter_mut() {
                *c = -*c;
            }
        }
    }

    fn register_commands(&self, _: &mut dyn ServerHost, event: &mut RegisterCommandsEvent<'_>) {
        event.commands.push(NativeCommand::new("flowread <x:integer> <y:integer> <z:integer>"));
    }

    fn perform_command(&self, host: &mut dyn ServerHost, event: &mut PerformCommandEvent<'_>) -> Flow {
        let words: Vec<&str> = event.command.split_whitespace().collect();
        if words.first() != Some(&"flowread") {
            return Flow::Continue;
        }
        let line = match coords(&words[1..]) {
            Some(at) => format!("server: {}", read_row(&*host, at)),
            None => "usage: /flowread x y z".into(),
        };
        host.feedback(Text::literal(line));
        Flow::Cancel
    }

    fn client_chat(&self, host: &mut dyn ClientHost, event: &mut ClientChatEvent<'_>) -> Flow {
        let words: Vec<&str> = event.original_message.split_whitespace().collect();
        if words.first() == Some(&"flowc")
            && let Some(at) = coords(&words[1..])
        {
            *event.message = format!("client: {}", read_row(&*host, at));
        }
        Flow::Continue
    }
}
