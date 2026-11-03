# Native mods (experimental)

Native mods are **mixin bundles** that change the game's own Rust source before it is compiled, with
[horizon-mixins](https://github.com/horizon-rust/horizon-mixins) (MIT, by levzzz5154, made for Horizon). A bundle
is a `horizon-mixin.toml` and a few `.rs` helpers that hook into named functions of the game (HEAD, BEFORE_CALL,
RETURN, MODIFY_ARG, OVERWRITE...; the tool's README lists them). A port such as Veinminer is three HEAD hooks and
one module. Start a mod from [horizon-native-mod-template](https://github.com/horizon-rust/horizon-native-mod-template).

Native mods are native code with the player's rights, not sandboxed. The launcher turns them on only when the player
installs one (after a notice) and builds them only behind a red warning; the registry lists one only after a review (its later releases are checked by machine); and
they play in a game of their own, built for the instance they are installed in.

## Pinned to a Nightly

A native mod builds on one exact Horizon commit: `[compatibility] horizon_revision` in its `horizon-mixin.toml`,
and `horizon` in the `horizon-native.json` of its registry file (`.hnmod`, `horizon_mods::index::native`). Mods
pin a **Nightly's commit** (or a release's). Every Nightly ships its source and keeps its commit for good as the
tag `nightly-source/<date>-<short>` ([release-flow.md](release-flow.md#nightly-channel)), so a pinned mod still
builds after its Nightly is replaced. When the mods of one build pin different commits, the launcher says which
mod pins which and the commit the build would use, the newest of them, and builds only after the player chooses
**Build anyway** (`native-build --allow-version-mismatch`): such mods often still apply and compile. A failed build
then says the mods were made for other versions. Their horizon-mixins pins may differ too: the build uses the
newest of them and the launcher's own, never one older than a mod was made with. A bundle with no pin builds on
the commit of the installed version the build starts from.

### Mods on the hook points

A mod that hooks only Horizon's native hook points (`crates/horizon-client/src/game/native_hooks.rs`: empty
functions the game calls at fixed places, plus helpers, kept stable across releases) can name the version it
needs instead of pinning: `"hooks": 1` in its `.hnmod`'s `horizon-native.json`, the `NATIVE_HOOKS_API` of that
file when it was made. Hook points are only ever added: each addition raises `NATIVE_HOOKS_API`, and a mod builds
on any Horizon whose version is the one it needs or a later one. Its `horizon` is then only the commit it was tested on. The launcher builds it on the commit the other mods
pin, else on the installed version's commit (the game the player plays), after checking that commit's
`NATIVE_HOOKS_API` (read from GitHub) is at least the mod's; when it isn't and nothing pins, it builds on the commit the
mod was tested on. In its own copy of the mod it sets `horizon_revision` to the commit it builds on. Horizon's
own native mods (Zoom, FPS counter, Keystrokes, Minimap; horizon-rust/horizon-mods, `native/<name>/`) are made this way, so any of them build together
with no re-release for each Nightly.

API 2 adds the item panel beside container screens and its recipe pages (`native_panel_items`,
`native_panel_recipes`, and the hook `native_panel_event` for the player's clicks, searches and keys over items), and
the game's items and recipes (`native_recipes`, `native_known_recipes`, `native_place_recipe`, `native_give`): what a
recipe viewer needs.

API 11 adds `GameLevel::native_send_particles` on authoritative servers. It sends supported
vanilla particles to nearby players. Block particles require a valid block state. Item, trail and
vibration particles are not supported by the current dispatch. Count zero sends one directional
particle. The return value says whether the call was accepted, not whether any player received it. Bundles that use it need `"hooks": 11`.

API 3 adds authoritative world lifecycle events through `native_world_lifecycle`: open ready, open failed,
and final close. Bundles that use these events need `"hooks": 3`.

API 4 adds authoritative server tick events through `native_server_tick`: start (before any level ticks, like NeoForge's `ServerTickEvent.Pre`) and end of each non-paused
server tick, with `world_running`. A frozen world reports false except during a requested step. Bundles that
use these events need `"hooks": 4`.

API 8 adds server commands and players: `native_register_commands` (the commands a mod adds to every player's
command tree) and `native_perform_command` (a player's command on the server), with helpers to answer
([Server commands and players](#server-commands-and-players)). Bundles that use them need `"hooks": 8`.

API 9 adds the acting player's plain items for server mods (count, take, give; [Server commands and
players](#server-commands-and-players)). Bundles that use them need `"hooks": 9`.

API 10 adds server menus: a vanilla chest menu a server mod opens and fills, whose clicks and closing reach the mod
(`native_menu_clicked`, `native_menu_closed`), and `native_exchange_items`, two players' stacks swapped in one checked
step ([Server menus](#server-menus)). Bundles that use them need `"hooks": 10`.

API 12 adds item tooltips through `native_tooltip`: the hovered stack, with its components, and its tooltip's
lines, which a mod can add to, change or take out of, wherever the game shows an item's tooltip. Bundles that use it
need `"hooks": 12`.

API 13 adds using an item, on the server for every player: `native_use_item_on` hears a right click with an item on a
block once the block let it through (`ItemStack.useOn`), and `native_use_item` hears the item's own use, in the air or
after `native_use_item_on` let it through (`Item.use`). Each hears a click once. A mod that claims one swings the hand
instead of the item's use. Bundles that use them need `"hooks": 13`.

API 14 adds breaking and placing a block, on the server for every player: `native_destroy_block` hears a player break a
block once the game allowed it (`ServerPlayerGameMode.destroyBlock`) and may cancel it, which keeps the block and its
drops; `native_place_block` hears a block placed from an item (`BlockItem.place`). To stop a placement, a mod claims
`native_use_item_on`, which runs before it. Bundles that use them need `"hooks": 14`.

API 15 adds block entities for a mod's blocks (`native_block_entity`, `native_block_entity_ticks`,
`native_block_entity_tick`): made when the block is placed, ticked on the server and saved with the chunk. Bundles that use them need `"hooks": 15`.

API 16 adds item slots for those block entities (`native_block_entity_container`): hoppers, comparators and breaking the
block use them as they use vanilla's containers, and `native_block_entity_tick` also gets the slots. Bundles that use them need `"hooks": 16`.

API 17 adds menus for mod containers: the container names one of vanilla's slot-grid or furnace menus, which opens when the block is used,
and `native_block_entity_tick` takes the whole block entity, with the data slots the menu shows. Bundles that use them need `"hooks": 17`.

API 18 adds what a machine's tick reaches: `native_with_mod_block_entity` (change a neighbouring mod block entity, which marks its chunk
and tells comparators when its items changed) and `native_set_block_property` (a property of the block's own state, through the level's
normal update path). Bundles that use them need `"hooks": 18`.

API 19 adds screens a mod draws for its blocks' menus: `native_screen` lays one out (its height, where each container slot is,
where the inventory starts; a slot it leaves out is not in the menu, so shift-clicks skip it) and `native_screen_draw` draws it with the mod's
own GUI textures (`assets/<namespace>/textures/gui`, drawn with `native_sprite`). Bundles that use them need `"hooks": 19`.

API 20 adds buttons, tooltips and item icons on mod screens: `native_screen_click` and `native_screen_tooltip` (the client), `native_menu_button`
(a button press on the server, on the mod block entity) and the helpers `native_item`, `native_screen_title`, `native_data`, `native_block`,
`native_block_property`, `native_insert_item` and `native_cursor`. Bundles that use them need `"hooks": 20`.

API 21 adds what a server mod needs to guard land and move players. `native_right_click_block` hears a player's right
click on a block before the block's own use (a chest, a door, a lever) and the item's, and may cancel it: nothing
happens and the player sees the block as it is. `native_acting_player` says who acts inside a player's hook (UUID,
name, level, position, game mode, health); `native_run_command` runs a command as the server's console (level 4, no
output), so `execute in <dimension> run tp <player> <x> <y> <z>` moves a player across levels; `native_action_bar`
tells a player why, even inside their own hook; and the players
`native_server_players` lists carry their health. Bundles that use them need `"hooks": 21`.

API 22 adds what a server mod needs to keep the world itself out of guarded land. `native_block_spread` hears a
fluid flow or fire spread into a block, `native_piston_move` a piston's push or pull, `native_explosion_detonate` the
blocks an explosion is about to break (a mod takes out those it spares), `native_fill_bucket` a bucket filled or
emptied, and `native_attack_entity` a player's blow on an entity (an item frame, an armor stand, an animal, a
player). Each may cancel, and nothing happens. Bundles that use them need `"hooks": 22`.

API 23 adds `native_level_tick_pre` and `native_level_tick_post`, server-side observations around each
level tick pass. Each receives the dimension ID; neither can cancel the tick. Paused and held passes fire
neither hook, and an error skips the post hook. Bundles that use them need `"hooks": 23`.

API 24 adds `native_set_block`, a server-side `Game` helper that replaces a state in the current dimension through
`Level.setBlock`. It accepts vanilla's neighbour and shape flags and returns whether a block changed; a block with a
block entity gets one. Horizon sends every write to players whatever the flags: `UPDATE_CLIENTS`, `UPDATE_INVISIBLE`
and `UPDATE_IMMEDIATE` change nothing yet. Hooks holding only `GameLevel` cannot call it yet. Bundles that use it need
`"hooks": 24`.

API 25 adds `GameLevel::native_play_sound` for registered vanilla sound events on an authoritative server.
It queues the sound for nearby players in the level's dimension, including during a guest's turn.
Pass a `SOUND_SOURCES` index, position, volume and pitch. The return value says whether the call
was queued, not whether a player heard it. The server discards unknown sound events when it dispatches
the queue. Bundles that use it need `"hooks": 25`.

API 26 adds `Game::native_config_dir()`: the game's shared `config/` folder beside `options.txt` (beside
`server.properties` on a dedicated server), where a mod names its own files, made when missing. It takes no `self`, so
it works from any hook ([mods.md](mods.md#native-mods)). Bundles that use it need `"hooks": 26`.

API 27 adds the player-health HUD rows through `native_health_hud`: every frame the HUD draws them, the
hook hears the layout's inputs (`HealthInputs`) and the screen's GUI size, and a mod that answers with
draws replaces all the rows — armour, hearts, food, air and the heights above the hotbar — for that
frame. Nothing answered draws Horizon's own. Bundles that use it need `"hooks": 27`.

API 28 adds `native_block_in_tag` and `native_item_in_tag` for server tick hooks. They read the world's merged block and item tags, including data packs and content mods. An unknown tag returns false. An empty item stack tests the `minecraft:air` item, as vanilla does. Calls with an inactive or stale tick context return an error. Bundles that use them need `"hooks": 28`.

API 29 adds right-clicking an entity, on the server for every player: `native_interact_entity` hears a right click on
an entity (its id and type, and the stack in the hand) where `Player.interactOn` starts, once per hand. A mod that claims
it swings the hand instead of the entity's and the item's own interaction. Bundles that use it need `"hooks": 29`.

API 30 adds left-clicking a block, on the server for every player: `native_left_click_block` hears the start of a dig
(the block, the face hit) once the game allowed it, before the block is attacked (a note block plays) or broken in
creative. A mod that cancels it keeps the block: on the server it isn't attacked, dug or broken, and the player's client
gets it back. The clicking player's own client still predicts the crack until then.
Bundles that use it need `"hooks": 30`.

API 31 adds a player's server tick: `native_player_tick` runs once a tick for every living player in the world, at the
end of their `ServerPlayer.tick`, with the helpers reading that player. Bundles that use it need `"hooks": 31`.

API 32 adds shape updates a mod can answer through `native_update_shape`: on an authoritative server, each time a
block's state is worked out from a neighbour (`BlockBehaviour.updateShape`): neighbour shape updates after a
`setBlock`, a block fitted to all its neighbours as a piston lands it or a chunk is post-processed, and the blocks at
a feature's edge as it is placed in the live world, such as a tree grown from a sapling
(`StructureTemplate.updateShapeAtEdge`). It gets the dimension, position, state, the neighbour's direction, position
and state, before the block computes it. `state` may not be in the world yet (a piston landing it), so decide from
`state`, not from the block at `pos`. Answer only for your own blocks and leave `new` empty otherwise: a state put
there is the block's new state, applied as vanilla applies the result (air breaks the block, with its drops, on a
neighbour update; post-processing and a feature's edge write air without drops), and the block's own `updateShape`
doesn't run, nor the ticks it would schedule. The level is read-only, as vanilla's `LevelReader` is there; the hook
schedules ticks with its `ticks` through the level's `native_schedule_block_tick(ticks, pos, block, delay)` and
`native_schedule_fluid_tick(ticks, pos, fluid, delay)` (vanilla's `ScheduledTickAccess.scheduleTick`; false for an
unknown name or a position that isn't loaded). It runs often, at least six times per `setBlock` that updates shapes
(more for redstone wire's diagonals and knock-on updates): keep it short. A LAN guest shows vanilla's shape for a
moment, until the server's update arrives. The deprecated in-frame path and chunk generation fire none. Bundles that
use it need `"hooks": 32`.

API 33 adds random ticks a mod can handle through `native_random_tick`: on an authoritative server, each block
`ServerLevel.tickChunk` random-ticks (`BlockState.randomTick`), with its dimension, position and state, before the
block handles it. Only blocks that tick randomly fire it (a mod block as its vanilla template block does), once per
draw that lands on one: it runs often, so keep it short. Bone meal's random ticks (`StemBlock.performBonemeal` calls
`randomTick` itself) don't fire it. The hook may change the block with the level's
`native_set_block_property(ticks, pos, key, value, flags)` and set `handled` to skip the block's own random tick, so a
mod crop grows its own way. Without `handled`, the block then takes its own random tick as it is now (if it still
ticks randomly), and lava's fluid random tick runs either way. It needs the level mutably, which only `native_random_tick` and API 34's `native_tick_block` give. It is API 18's `Game::native_set_block_property` with `Level.setBlock`'s flags (`native_hooks::flags`): a
vanilla crop grows with `flags::UPDATE_CLIENTS` (neighbours' shapes update, so an observer fires, but no neighbour
gets `neighborChanged` and comparators aren't told), a furnace sets its `lit` with `flags::UPDATE_ALL` (both). Horizon
sends every write to players whatever the flags: `UPDATE_CLIENTS` (2), `UPDATE_INVISIBLE` (4) and `UPDATE_IMMEDIATE`
(8) change nothing yet. It writes at any loaded position, returns false off an authoritative server (the `Game` one too: a window that isn't a server), for a property or
value the block doesn't have or a position that isn't loaded, and true without writing when the property already has
the value. Elsewhere, `Game::native_set_block_property_with(pos, key, value, flags)` does the same, as a brewing
stand's tick sets its bottles with `flags::UPDATE_CLIENTS`. The deprecated in-frame path fires none. Bundles that use
them need `"hooks": 33`.

API 34 adds scheduled block ticks a mod can handle through `native_tick_block`: on an authoritative server, each due
scheduled tick of a block that is still there (`ServerLevel.tickBlock` → `BlockState.tick`), with its dimension,
position and state, before the block handles it. The hook may change the block with the level's
`native_set_block_property` and the flags the block's own tick would write with (as in API 33: an observer's tick
uses `flags::UPDATE_CLIENTS`) and set `handled` to skip the block's own tick. Without `handled`, the block then takes
its own tick as it is now, if it is still the tick's block. The deprecated in-frame path fires none. Bundles that use
it need `"hooks": 34`.

API 35 adds the commands a mod owns on the client: `native_commands` is asked for their names, so the chat box
colours, completes and lists them like the game's own, and `native_command` runs them. A name the game already has
keeps that command. Bundles that use it need `"hooks": 35`.

API 36 adds recipes of a mod's own types, for a machine's crusher or altar. A recipe file whose `type` is outside `minecraft`
(`mymod:crusher`) in `data/<namespace>/recipe/`, in a content mod (`.hmod`) or a world's data pack (not in a native mod's own bundle), stays loaded as it is: `native_recipes_of_type`
gives each one's id and JSON, for the mod to read as it likes, and `native_recipe_ingredient` and `native_recipe_result` read the
usual fields, tags included (a bare id is read as `minecraft:`; items the game doesn't know are left out, and the helper gives nothing if none is left). The recipes are the server's table: on the server thread, a dedicated server or a LAN host; a client, the single-player window included, sees only the content mods' recipes, not the world's data packs'. All three give nothing outside a world; on a server they work in its tick hooks too (`native_level_tick_post`), where no player's state is swapped in. The game doesn't craft them or show them in the recipe book; `/reload` reads a data pack's again.
Bundles that use them need `"hooks": 36`.

API 37 adds [mobs](#mobs) for server mods: spawn one, read the mobs of a level in place, and move, hurt or remove one by its id.
Bundles that use them need `"hooks": 37`.

API 38 adds redstone from a mod's block, on authoritative servers. `native_block_signal` may change the signal a block gives on
a face, weak or direct, and `native_is_signal_source` lets redstone wire connect to it. Both are asked very often, so check the
block first and answer from the mod's own state. When the output changes, and once the block is placed (a direct signal past a conductor isn't read before), `native_update_signal_neighbors(dimension, pos)` makes the blocks around
it read it again; it acts on the level in place and is refused for another dimension. Bundles that use them need `"hooks": 38`.

API 39 adds item behaviour, on the server for every player: `native_hurt_enemy` hears a player's hit land on a mob with the
held stack, `native_inventory_tick` runs each tick for every stack a player carries (armour and off hand included), and
`native_carried_stack` reads and changes that stack, and `native_on_equip` hears an equipment slot take a different item. The helpers read the acting player. Bundles
that use them need `"hooks": 39`.

API 40 adds [mod messages](#mod-messages) between a mod's server and client halves, and hearing players and mobs on the
server for every player: `native_player_died`, `native_mob_spawned`, `native_mob_killed` (a player's melee blow, a sweep or a spear thrust too),
`native_item_dropped` (a throw; not the items scattered when a player dies) and `native_item_picked_up` (`canceled` leaves the stack on the ground). They name a player by UUID. Bundles that use them
need `"hooks": 40`.

API 41 adds chunks joining and leaving a server level, NeoForge's `ChunkEvent.Load` and `.Unload`:
`native_chunk_load(dimension, chunk)` hears each column once as it joins the level (loaded from the save or
generated), in the level's next tick pass, and `native_chunk_unload(dimension, chunk)` as it leaves. A mod that
keeps its own state for its blocks (a network of machines) builds it from the columns it hears, so its passive blocks
need no block entity tick. Server-side only. Bundles that use them need `"hooks": 41`.

API 42 adds animated parts, for machines whose parts turn (shafts, gears): `native_part_model(state)` bakes a block
state's model once into a part model and returns its handle; `native_part_set(key, model, block, axis, rpm, phase)` shows
it at a block under the mod's own key, turning about an axis at a speed in RPM from a starting angle, and
`native_part_clear(key)` hides it. The renderer keeps each model on the GPU and draws all its instances in one call; the
GPU turns them from the game's clock (stopped while paused), so a turning part costs nothing per frame and an instance is
uploaded again only when it is set to something else or its block's light changes. A content block drawn this way sets
`properties.render_shape` to `entityblock_animated`, so its model leaves the chunk mesh. Client-side (the window's
`Game`); nothing on a server. Bundles that use them need `"hooks": 42`.

API 43 adds a mod's own data saved with a chunk, NeoForge's `ChunkDataEvent.Save`: `native_chunk_data_save(dimension,
chunk, data)` is asked each time a server chunk is saved, and a mod puts its compound in `data` under its id (vanilla
never reads the `horizon:mod_data` root key it is kept in, and a world opened without the mod keeps it as it was).
Inside `native_chunk_load`, `Game::native_chunk_data(chunk, mod_id)` reads it back. A mod that keeps typed state in
memory (settings of its machines) writes NBT only then; after changing it, `native_mark_chunk_unsaved(dimension,
chunk)` makes the chunk save. Server-side. Bundles that use them need `"hooks": 43`.

API 44 adds `native_fluid_flow(dimension, pos)`: the flow of the fluid at a block, as vanilla's `FluidState.getFlow`
gives it (a horizontal unit vector downstream, -1 in y where it falls, zero for still water or none), for machines a
current turns. Server-side, for the level in place. Bundles that use it need `"hooks": 44`.

API 45 adds items and moving parts: `native_part_item_model(item, pose)` bakes an item as it is drawn in a frame
(`ItemDisplayContext.FIXED`), placed by `pose` in the block (the item is centred on 0, so translate it by 0.5), into a
part model; `native_part_glide(key, model, block, axis, rpm, phase, offset, velocity)` shows a part that sits `offset`
blocks from its block at clock time `offset[3]` (`native_parts_seconds()`) and moves at `velocity` blocks a second from
then on, on the GPU, so an item gliding along a belt is uploaded once per change of motion, not per frame. The clock
wraps every hour of game time. Client-side. Bundles that use them need `"hooks": 45`.

API 46 adds `native_contents_changed(pos)` (NeoForge's `ItemStackHandler.onContentsChanged`): a hopper or dropper
changed the items of a mod's container, so a block entity that does not tick can react only when it must; and
`ModContainer::only_when_empty`, for a container hoppers fill only while it holds nothing (Create's depot).
Server-side. Bundles that use them need `"hooks": 46`.

API 47 adds `native_item_uses_block_light(item)`: whether an item's model is lit like a block (`BakedModel.isGui3d`),
for mods that lay items down standing or flat (Create's depots and belts). Client-side. Bundles that use it need
`"hooks": 47`.

API 48 adds `native_part_model_posed(state, pose)`: a block state's model baked into a part model under a transform in
block units, for partial models turned in ways block states don't offer (Create's sideways belts). Client-side. Bundles
that use it need `"hooks": 48`.

API 49 adds `native_update_entity_after_fall_on(pos, stack, at)`: an item entity lies on the block at `pos`
(`Block.updateEntityAfterFallOn`); a mod's block takes some or all of it by lowering the stack's count (Create's belts
and depots). Called each tick the item is on the ground. Server-side. Bundles that use it need `"hooks": 49`.

API 50 adds `native_part_model_tinted(state, pose, tint)`: `native_part_model_posed` with the model's tinted faces
(those with a tint index) coloured `0xRRGGBB`, as a block colour tints them: a fluid's still texture in its colour (Create's
tanks). Client-side. Bundles that use it need `"hooks": 50`.

API 51 adds `native_part_model_compound(blocks)`: one part model of many block states, each under its pose, for a
structure that moves as one (Create's contraptions turning about their bearing); and `native_part_model_free(model)`,
which empties it when the structure is gone. Client-side. Bundles that use them need `"hooks": 51`.

API 52 adds `native_items_in(dimension, min, max, f)`: the item entities whose box touches a box of the level
(`Level.getEntitiesOfClass(ItemEntity.class, box)`), each given to `f` with its entity id, stack, position and motion;
`f` may change the stack and the motion (`setItem`, `setDeltaMovement`), and a stack left empty removes the entity
(`discard`). For machines that act on items in the world (Create's fans and crushing wheels). Server-side, for the
level in place. Bundles that use it need `"hooks": 52`.

API 53 adds `native_extract_item(pos, face, max)`: takes up to `max` items of one kind from the container at `pos`
through its side `face` (`IItemHandler.extractItem`): from the first slot a hopper could take from through that face,
up to its whole stack; None when there is no container or nothing to take. For machines that pull from inventories
(Create's funnels). Server-side. Bundles that use it need `"hooks": 53`.

API 54 adds `native_use_item_at(pos, facing, stack)`: uses `stack` on the block in front of `pos` as a machine standing in
for a player (Create's deployer): the dispenser's behaviour for items that act on the block there (buckets, flint and
steel, bone meal, shears, glass bottles, glowstone, honeycomb, brushes), without its sound and smoke. What is left of the
stack, or None for any other item. Server-side. Bundles that use it need `"hooks": 54`.

API 55 adds text in the world, what a NeoForge `BlockEntityRenderer` draws with `Font.drawInBatch` (a machine's display):
`native_text_set(key, pos, lines)` draws `TextLine`s at block `pos` under the mod's own key, in place of what the key
showed, and `native_text_clear(key)` takes them away. A line has a `pose` from text space to the block (block units;
text space is font pixels, as a sign's: x right, y down, a glyph 8 high, a string `native_text_width` wide, so a
standing sign's front text is `translate(0.5, 0.5 + 1/3, 0.5 + 0.0467) * scale(1/96, -1/96, 1/96)`), its string, an
ARGB colour, an outline colour (0 for none) and `full_bright` (else lit by the light at the block). The text uses sign
text's font and stays on the GPU as a part model: it is laid out again only when the entry or the font changes, so a mod
may set it every tick, and it shows within 64 blocks of the camera in a loaded chunk. Client-side (the window's `Game`).
Bundles that use it need `"hooks": 55`.

API 56 adds comparators reading a mod's blocks, vanilla's `Block.hasAnalogOutputSignal` and `getAnalogOutputSignal`: a
content block with `properties.analog_output` set to `true` is read by comparators (`false` stops a template's analog
output), and `native_analog_output_signal(dimension, pos, state, direction, signal)` answers what they read (0 to 15;
`direction` points from the block to the comparator; `signal` holds the template's answer). It is asked only of
blocks with an analog output, as comparators read them: check the block first and answer from the mod's own state.
When the answer changes, `native_update_neighbour_for_output_signal(dimension, pos)` (`Level.updateNeighbourForOutputSignal`)
makes the comparators beside the block, or behind a conductor there, read it again; it acts on the level in place and
is refused for another dimension. Server-side. Bundles that use them need `"hooks": 56`.

API 57 adds key input for client mods, NeoForge's `InputEvent.Key`: `native_input_key(key, action, modifiers, screen,
consumed)` hears every key that goes down, repeats or goes up in a world, by its vanilla name (`key.keyboard.r`), with
its action (`KeyAction::Press`, `Repeat`, `Release`) and GLFW's modifier bits (`KEY_SHIFT`, `KEY_CONTROL`, `KEY_ALT`,
`KEY_SUPER`). It fires where `KeyboardHandler.keyPress` takes the key, after the window's shortcuts (fullscreen, a
screenshot, the F3 debug keys) and before a screen or a key mapping sees it. `screen` is true when a screen is open
(chat, an inventory, a sign): stand back there unless the key is meant for it. A mod that sets `consumed` on a press or
a repeat keeps it from the game (no screen, mapping or `native_key` hears it); a release always reaches the game, so
nothing stays held. `native_is_key_down(key)` (`InputConstants.isKeyDown`) says whether a key, or a mouse button
(`key.mouse.left`), is held on the keyboard and mouse themselves, a consumed key's too, for a mod that polls in
`native_tick` (NeoForge's `ClientTickEvent`). Client-side (the window's `Game`). Bundles that use them need `"hooks": 57`.

API 58 adds a mod's own world data, vanilla's `SavedData` in the overworld's `DimensionDataStorage`:
`native_saved_data(mod_id, name)` gives the compound kept in the world folder as `data/<mod_id>/<name>.dat` (beside
`data/minecraft/scoreboard.dat`, written as vanilla writes them: `{data: {...}, DataVersion: N}`), read the first time it
is asked for in a world (empty when there is no file) and then held in memory; `mod_id` is `[a-z0-9_.-]` and `name` the
same with `/` between parts. `native_set_saved_data_dirty(mod_id, name)` (`SavedData.setDirty`) has the next save write
it: data that isn't dirty is never written. `native_level_load(dimension)` (NeoForge's `LevelEvent.Load`) fires once
when the world opens on its server, before `WorldOpenReady`, to read the data; `native_level_save(dimension)`
(`LevelEvent.Save`) fires on every save (autosave, `/save-all`, the closing save), before the dirty data is written, so a
mod that keeps typed state in memory fills its compound there and marks it dirty. Both carry the overworld, whose
storage is the world folder; other dimensions fire none yet. Server-side (a client of a server gets none). Bundles that
use them need `"hooks": 58`.

API 59 adds per-position block models, what NeoForge's `ModelData` gives an `IDynamicBakedModel`
(`IBlockGetterExtension.getModelData`, read in `getQuads`): a copycat block that draws the block it imitates cut to its
own shape. `native_block_model_override(pos, Some(model))` draws block `pos` of a content block with
`properties.dynamic_model` ([mods.md](mods.md)) as a `ModelOverride` says: `state`'s baked model, with `faces[d]` (by
`Direction` ordinal) giving another state's quads for the faces looking `d`, cut to `boxes` (`[min, max]` in block
units; empty draws the model whole). A quad on the block's side moves to the box's side and is clipped to the box,
its UVs following; one inside is clipped; one that isn't an axis-aligned rectangle is kept only whole inside a box.
Faces between two boxes are drawn, so give boxes that don't touch where the material should not show. The block keeps
its own state: it is lit (smooth light, AO) and culled against its neighbours as itself, they cull against it as
before, and each quad takes the tint of the state it comes from (a grass block's top stays green). `None` draws the
block's own model again. Only the position's section is meshed again, and only when the override changed, so a mod
may set it every tick; a position of a block without `dynamic_model` never reads it. Overrides are kept until cleared
or the level changes (a dimension, another world). Client-side (the window's `Game`): in multiplayer each client's
half of the mod sets them, from the block's data it sent with its own messages. Bundles that use it need
`"hooks": 59`.

```rust
use horizon_client::mesher::ModelOverride;
// A copycat panel on the north side, showing the material its client half received.
let mut model = ModelOverride::new(material);
model.boxes.push([[0.0, 0.0, 0.0], [1.0, 1.0, 0.125]]);
self.native_block_model_override(pos, Some(model));
// Its material taken out: back to the panel's own model.
self.native_block_model_override(pos, None);
```

API 60 adds [entities](#entities) for server mods: read any entity of a level by id or in a box, and set its motion
(NeoForge's `Entity.setDeltaMovement` with `hurtMarked`), reset its fall (`Entity.resetFallDistance`) or turn it
(`Entity.setYRot`), players included. Bundles that use them need `"hooks": 60`.

API 61 adds `native_entity_inside(dimension, pos, state, entity)`, vanilla's `BlockBehaviour.entityInside`, for content
blocks with `properties.entity_inside` ([mods.md](mods.md)): each server tick, once for each such block an entity's box
(less 1.0E-5 on each side) is in, with the entity as `native_entity` reads it (players, mobs, item entities,
minecarts, boats and armor stands; not spectators). It fires where the level's block tick checks pressure plates and
tripwires, with the entities where the last tick left them; inside it, the [entity helpers](#entities) move, turn and
stop them (a turntable, a belt). With no block of that property in the game nothing is searched. Server-side. Bundles
that use it need `"hooks": 61`.

API 62 adds [minecarts](#minecarts) on a mod's rails: content blocks with `properties.rail` ([mods.md](mods.md)) are
plain rails to the carts, and `native_minecart_pass` (NeoForge's `IBlockExtension.onMinecartPass`) hears each cart on one
and may change its motion; `native_minecart` and `native_minecarts` read the carts with their riders. Bundles that use
them need `"hooks": 62`.

API 63 adds [riding](#riding): seats a mod spawns and moves, players and mobs put on a vehicle or taken off
(`Entity.startRiding`, `stopRiding`), who rides what, and `native_entity_mount` (NeoForge's `EntityMountEvent`) when a
player gets on or sneaks off a vehicle. Bundles that use them need `"hooks": 63`.

API 64 adds `sneaking` and `flying` to `NativeEntity` (`Entity.isShiftKeyDown`, `Abilities.flying`): a fan pushes a
sneaking player less and lets a flying one go. Bundles that read them need `"hooks": 64`.

API 65 adds `native_spawn_minecart` and `native_remove_minecart` ([minecarts](#minecarts)). Bundles that use them
need `"hooks": 65`.

API 66 adds clicks for client mods, NeoForge's `InputEvent.InteractionKeyMappingTriggered`:
`native_interaction_key_mapping_triggered(button, hand, canceled, swing)` hears each click of the attack, use and
pick-block keys while playing (`Button::Attack`, `Use`, `Pick`; `hand` is the main hand, whose turn comes first),
where `Minecraft.startAttack`, `startUseItem` (a held use repeating too) and `pickBlock` start, whatever the
crosshair is on: a block, an entity or nothing. A mod that sets `canceled` keeps the click from the game (nothing is
attacked, used or picked, nothing is sent to the server) and the hand swings unless it clears `swing`. It is for what
only the mod knows is there, such as the blocks of a moving contraption it draws: it reads the crosshair itself and
sends the click to its server half with [its own messages](#mod-messages). Client-side (the window's `Game` as a
client of its server: single player's server thread, LAN, multiplayer). Bundles that use it need `"hooks": 66`.

API 67 saves [seats](#riding) with their riders, as vanilla's vehicles: with their chunk and the mobs on them, or in
the file of the player riding one, who sits on it again at the next login. `native_seat_loaded(dimension, seat, pos)`
hears each one come back. Bundles that use it need `"hooks": 67`.

API 68 adds three [minecart](#minecarts) helpers for a mod that carries something on a cart:
`native_set_minecart_position`, `native_set_furnace_fuel` and `native_set_minecart_carrying`, and
`fuel`, `push` and `carrying` in `NativeMinecart`. Bundles that use them need `"hooks": 68`.

API 69 adds `native_item_handler_insert(pos, side, stack, simulate, handled)` and
`native_item_handler_extract(pos, side, amount, simulate, taken, handled)` (NeoForge's
`Capabilities.ItemHandler.BLOCK`, `IItemHandler.insertItem` / `extractItem`): a hopper facing a mod block with no
container offers it one item at a time, and a hopper below one takes one item a transfer, if it has room
(`VanillaInventoryCodeHooks.insertHook` / `extractHook`). A mod sets `handled` for its blocks, leaves in `stack` what it
didn't take, and gives in `taken` what it gives; `simulate` asks without changing anything. For a block whose items
aren't slots, such as Create's creative crate (an endless supply that voids what it is given), where a
`ModContainer` would hold copies. Server-side. Bundles that use them need `"hooks": 69`.

## What the launcher does

In the window: install a native mod from the Mods page. The first one shows a notice once (the first build downloads
Rust and can take 20 to 30 minutes; later builds are faster) and turns on Settings → Game → **Native mods
(experimental)**, which stays off for players who never install one; `mods install` does the same. Then **Build**. The launcher names the mods, warns in red, and builds only from "I trust these mods: build"; the
row shows each step, the error if one fails, and the build the instance plays. From the command line:
`horizon-launcher native-build [FOLDER] [--instance NAME] --trust-native-code`. FOLDER is a bundle, an unpacked
`.hnmod`, or a folder of them; without it, the native mods turned on in the instance. It is not an MCP tool:
trusting native code is the player's decision, never an agent's.

1. **Rust.** The launcher's own toolchain ([`rust_toolchain`](../launcher/src/rust_toolchain.rs)): Rust 1.99.1's
   official `rustc`, `cargo` and `rust-std` (and `rust-mingw` on Windows), each pinned by SHA-256, downloaded once
   from static.rust-lang.org into `<launcher data>/rust/`, with its own Cargo home `<launcher data>/cargo/`. No
   rustup, nothing on the system; the player's own Rust is never used. Windows uses the `windows-gnu` host, which
   links without Visual Studio. `rust-mingw` only links, so on Windows the toolchain also brings MinGW-w64
   (`rust_toolchain::MINGW`: WinLibs' GCC 16.2.0 on MinGW-w64 14.0.0, MSVCRT like `rust-mingw`, one zip pinned by
   SHA-256, its `mingw64/` without the documentation) on the build's `PATH`: `ring` and `hidapi` compile C,
   and `windows-sys`' `raw-dylib` imports need `dlltool` and the assembler it runs. The linker is the system's on
   macOS (the Command Line Tools: the launcher asks for `xcode-select --install`) and Linux (`cc`). On Linux the
   game also builds against the system's ALSA (`alsa-sys`) and libudev (`libudev-sys`, `hidapi`), found with
   `pkg-config`, whose development files desktop distributions don't install. Before downloading anything the
   launcher checks for `cc`, the C library's headers (`cc` compiles an include of `stdint.h`), `pkg-config`,
   `alsa` and `libudev` and names what is missing, with the install command for the distribution in
   `/etc/os-release` (Debian, Ubuntu and Mint:
   `sudo apt install build-essential pkg-config libasound2-dev libudev-dev`). SteamOS has GCC without the C
   library's headers, and a read-only system: native builds don't work there yet.
   `horizon-launcher rust status|install|remove`.
2. **horizon-mixins**, at the commit the mods name (their `.hnmod`'s `mixins`), else `native_mods::MIXINS_REV`,
   built once with that cargo (`cargo install --locked --git … --rev …`) into `native/tools/<commit>/`.
3. **The source** of the pinned commit: GitHub's archive of it, once per commit (`native/source/<commit>/`, about
   63 MB unpacked), with the commit in `.horizon-revision`. Never the player's own checkout. Links in the
   archive are left out: the tool refuses them and nothing needs them to build.
4. **Mixins.** The bundles are copied first, so the build and its key use that copy whatever happens to the
   folder meanwhile. `horizon-mixins apply` stages the modded source; the launcher copies only the files that
   changed into `native/stage/`, so Cargo, which goes by file times, recompiles only the crates the mods touch.
5. **Compile**: `cargo build --locked --profile release --features horizon-client/mods -p horizon-client`, the
   release's own command (game-build.yml), into `native/target/`. Cargo downloads the locked crates from
   crates.io (Horizon has no git dependencies). The log is `native/build.log`; a failure shows its last lines (a
   horizon-mixins diagnostic in one line).
6. **Install**: a new version, `native-<commit>-<key>`, a copy of the starting version's folder (resources
   included, cloned on APFS, Btrfs and ReFS) with the built game in place of its own, and a `native-mods.json`
   saying which commit, mods, horizon-mixins and Rust made it. The instance the mods are installed in plays it (a
   folder built from the command line, or mods of the default home, go to the **Native mods** instance). That
   instance gets each mod's settings from the default home the first time it plays it (its
   `horizon-mods/.horizon-mod-data/<id>/` folder and `key.<id>.` lines of `horizon-mods.txt`, where earlier versions of
   the mod kept them); nothing it already has is overwritten.

## Install from GitHub

An author can ship a native mod without a registry pull request per version: the player installs it from a
GitHub link, on the Mods page ("Add from GitHub") or with `horizon-launcher mods add-github <url>
[--mod ID | --all] [--instance NAME] --trust-native-code`. The link is `https://github.com/<owner>/<repo>`
(its latest release), `.../releases/tag/<tag>`, `.../tree/<ref>` or `.../commit/<sha>`.

- **Resolved to something exact.** GitHub's public API (no sign-in; a rate limit is said plainly, and
  `HORIZON_GITHUB_TOKEN` raises it) turns the link into a release's `.hnmod` file or a **commit SHA**, never a
  branch that moves. A release with a `.hnmod` asset installs that file (its SHA-256 is compared with the one GitHub
  lists). With no release or no such asset, the launcher fetches
  `codeload.github.com/<owner>/<repo>/tar.gz/<sha>` and looks in it for mods by the rules of
  `native-build <folder>`: a folder with `horizon-mixin.toml`, or an unpacked `.hnmod` (`horizon-native.json`).
  Several mods: they are listed, and the player picks one or installs all. A source folder must pin the Horizon commit
  (`[compatibility] horizon_revision`); the launcher packs its Rust, TOML and `data/` files into a `.hnmod`.
  A release that ships only a content mod (`.hmod`) says "not supported yet": installing `.hmod` files from a link
  is not built.
- **Checked like the registry's files.** The `.hnmod` is read as `horizon_mods::index::native::Bundle::read` reads a
  registry file (only Rust, TOML and data under `mixins/<id>/`, size limits, nothing run) before it is installed.
- **Said first.** Before anything is downloaded: "not reviewed by the Horizon registry; native mods run with full
  access to this computer; read the source", with a link to the exact commit. The command needs
  `--trust-native-code`, as `native-build` does. It is not an MCP tool: trusting native code is the player's
  decision, never an agent's.
- **Origin kept.** `horizon-native-mods/<id>.origin.json` beside the `.hnmod`: owner/repo, release tag (for a
  release), commit, asset name and the SHA-256 of the installed file. `mods list` shows "from GitHub: owner/repo @
  tag" (or the commit's first 7 digits).
- **Updates.** `horizon-launcher mods update-github [ID] [--check]` compares a mod from a release with the
  repository's latest release, and a mod from a commit with the head of the default branch; a newer one is
  installed with the same warning and `--trust-native-code`. `mods remove <id>` removes it. A mod already
  installed from the registry (or by hand) is never replaced.

Installing turns Native mods on, as any native mod does; then build as usual.

## Cache and disk

The key hashes the commit, the horizon-mixins commit, Rust's version, the profile and features, and every file of
every bundle. The same set on the same commit is never built twice. A change gives a new build, and
`native/target/` stays, so a rebuild compiles only `horizon-client` and what the mods touched.

Measured 2026-10-28 on the Mac mini (M4 Pro, 14 cores) with Veinminer 0.3.0 on Nightly `83f15fa`,
`CARGO_BUILD_JOBS=8` while other agents' builds ran (upper bounds):

| What | Size, time |
|---|---|
| Rust 1.99.1 (`<launcher data>/rust/`) | 453 MB; 58 s to download and unpack |
| horizon-mixins (`native/tools/`) | 7 MB; built once in 19 s |
| Crates in the launcher's Cargo home (`<launcher data>/cargo/`) | 289 MB |
| Source of one commit, and the stage | 64 MB each |
| First build: release profile, the whole game | 9 min 10 s of compiling, 9 min 41 s in all; `native/target/` 1.5 GB |
| Rebuild after a helper's code changed | 2 min 46 s of compiling (only `horizon-client`), 2 min 53 s in all |
| A change horizon-mixins ignores (a comment) | nothing compiled; 31 s, mostly the version folder's copy |

Measured 2026-10-29 on Windows 11 (Ryzen 7 9800X3D, 16 threads), the same mod and Nightly, an empty `native/target/`
(horizon-mixins, the source and the crates were already downloaded):

| What | Size, time |
|---|---|
| Rust 1.99.1 and MinGW-w64 (`<launcher data>/rust/`) | 1.7 GB, MinGW-w64 875 MB of it |
| Crates in the launcher's Cargo home | 657 MB |
| First build: release profile, the whole game | 6 min 57 s of compiling, 8 min 3 s in all; `native/target/` 2.0 GB |

Measured 2026-10-28 on Linux (an x86_64 server with no Rust of its own), Zoom on Horizon `1149c92`, 3 jobs while CI
ran: `rust install` in 11 s (592 MB), the first build in 12 min 9 s. The built game ran Zoom's proof (under `xvfb-run`:
the game needs a display even with `--offscreen`).

Only the native builds an instance plays are kept (`native_mods::prune`); `native/target/` stays for the next one.

## Trust

- **Off until the player installs one.** Settings → Game → Native mods stays off for a player who never installs a
  native mod; the first install shows the notice and turns it on. The command needs `--trust-native-code`; the window
  shows a warning in red before every build: the mods run with full access to the computer, and their ids and
  versions. The Mods page says who made each one, and "Not made or endorsed by the Horizon project" for any mod not
  Horizon's own.
- **The registry** (horizon-rust/horizon-mods) lists them as their own kind, `native-mod`, only after a maintainer
  has read the source and the repository they come from. Its entry names the GitHub repository (`repo`); each
  release of it with a `.hnmod` asset is a version once the bundle passes the registry's checks, with no further
  review (nothing is built or run by the registry; the player's launcher builds it behind its warning). The `.hnmod` contains `.rs` and `.toml` files and may contain data files under
  `mixins/<id>/data/`. The launcher checks the bundle without running anything. Native code is trusted.
  The file is pinned by SHA-256 like every registry file, and the version, game and pins are the bundle's own
  `horizon-native.json`. `revoked.toml` takes a bad release down.
  A content mod's entry may turn `native-mod` once (never back): its `.hmod` versions stay published and
  are checked as `.hmod`s, the index lists only its `.hnmod` versions, and the launcher's update keeps the
  player's `.hmod`; removing the mod removes both.
- **Isolation of data.** Native mods run only in the instance built for them, with its own worlds, options and logs.
- What this does not do: sandbox the compiled game. horizon-mixins' checks stop mistakes, not a malicious helper
  ([security notes](https://github.com/horizon-rust/horizon-mixins/blob/main/docs/security.md)).

## Multiplayer

- **Single player and LAN host:** the mods run in the host's game and its integrated server, so a server-side
  hook applies to every player of the world.
- **Joining a server:** only client-side hooks do anything; the protocol is unchanged.
- **Dedicated servers:** native mods are not built yet. Lifecycle hook call sites are ready for when the pipeline builds them (`--no-default-features`).

## Left to do

- Rebuild after the mods or the Nightly change (asking first, as it compiles for minutes).
- A prebuilt horizon-mixins from its CI instead of building it on the player's computer.

## Named hook catalog (draft)

`native-hooks.json` maps stable hook names and contract versions to the current
source targets. A contract includes the signature, call timing and behavior.
Keep an existing contract or add a new version when it changes incompatibly.
The catalog does not cover helper methods or arbitrary private game code.
Named injection resolution needs a matching horizon-mixins change; existing raw
targets and the `hooks` metadata continue to work as before.

### Reading server blocks (draft)

A native server tick hook can call `native_server_block` with its context, a dimension id and a block position. It returns the block name and state properties from a loaded chunk. It does not load a chunk or open a dimension. Reads outside that hook, from a stale context, from unavailable dimensions or outside the world bounds return an error.

### Neighbour observations (draft)

`native_neighbor_changed` observes each authoritative server `neighborChanged` before the block handles it. It supplies the dimension, position, current state ID, changed block ID and moved flag. Resolve IDs with the level's registries during the hook; the hook does not load chunks or change the update queue.

### Server block changes (draft)

`native_block_changed` observes a committed authoritative server `Level.setBlock` change before block side effects and neighbour updates. It supplies the dimension, position and old and new state IDs. It does not observe chunk loading or direct storage writes that bypass `Level.setBlock`.

### Server commands and players

For mods that run where the world is simulated: a dedicated server, a LAN host, single player's server thread
(`crates/horizon-client/src/game/native_server.rs`). The SMP mod of the official server is built on them
([smp.md](smp.md)).

- **`native_register_commands(commands)`**: push a `NativeCommand` per usage, asked once when a world opens to
  players. A usage is words, then arguments `<name:kind>`, optional ones last in brackets:
  `NativeCommand::new("pay <player:player> <amount:integer>")`, `NativeCommand::op("eco set <player:player>
  <amount:integer>", 3)` for operators of level 3. Kinds: `word`, `text` (the rest of the line, last), `integer`,
  `number`, `boolean`, `player`, `item`, `block_pos`. The usages of one command merge into one tree, which every
  player receives after vanilla's (filtered by their level, as vanilla's is), so their chat box completes and colours
  it. A name vanilla has, or a usage that doesn't parse, is left out with a warning in the log.
- **`native_perform_command(player, command, ran)`**: a player typed a registered command (without its slash) and
  their level allows it, whether the world allows commands or not. The mod parses the line and sets `ran`. The
  player's state is swapped in for the call, as for their other actions, so `native_player` reads them; a
  registered command no mod takes answers as vanilla's unknown command.
- Helpers: `native_server_players()` (uuid, name, level, dimension, position and game mode of each player on);
  `native_tell(uuid, text)`, a system line to one player; `native_broadcast(text)`, to every player and the server's
  log; `native_feedback(text)`, the command's answer to the player whose command it is; `native_world_dir()`, the
  open world's folder, where a server mod keeps its data in a folder of its own so the world's backups carry it.
- Items (API 9), of the acting player (inside `native_perform_command`, whose player is swapped in): `native_held()`
  (the main hand's item, count, and whether it is plain), `native_count_items(item)`, `native_carried_items()`,
  `native_take_items(item, count)` (the main hand first, then as `Inventory.add` orders slots),
  `native_room_for(item)` and `native_give_items(item, count)` (only where it fits: nothing is dropped). They look at
  the 36 inventory slots and the off hand, never the armour, and only at **plain** stacks: no damage and no
  component (name, enchantment...), so a shop never takes a worn or enchanted item for a new one. Each returns how
  many it counted, took or gave; the player's screen follows as for any change to their inventory.
- API 21: `native_acting_player()`, the player whose state is swapped in (inside `native_perform_command`,
  `native_right_click_block`, `native_use_item_on`, `native_destroy_block`...; None elsewhere);
  `native_run_command(line)`, a command run as the server's console (level 4, no output), only where no player is
  swapped in (`native_tick`, a menu hook): a teleport to another level (`execute in minecraft:the_nether run tp Alex 0
  70 0`) lands at the next tick's start. `native_server_players()` and the acting player carry `health` (0 once dead).
  `native_action_bar(uuid, text)`, a line over a player's hotbar, which also reaches the acting player inside their
  hook (where `native_tell` reaches nobody).
- **`native_right_click_block(pos, face, canceled)`** (API 21): a player's right click on a block, before the block's
  own use and the item's (`ServerPlayerGameMode.useItemOn`), once per hand. A mod that sets `canceled` stops both,
  without a swing; the player's client is sent the block back. With `native_destroy_block` it keeps players out of
  claimed land.
- **`native_block_spread(dimension, kind, from, to, canceled)`** and **`native_piston_move(dimension, piston,
  direction, push, destroy, canceled)`** (API 22, on `GameLevel`, like the block observers): a fluid flowing or fire
  spreading (`kind` `fluid` or `fire`) from `from` into `to`, and a piston moving blocks once its structure resolved.
  Canceled, the block at `to` stays as it is, and the piston moves nothing. Fluids ask on every flow: answer from
  memory, fast.
- **`native_random_tick(ticks, dimension, pos, state, handled)`** (API 33, on `GameLevel`): a block `tickChunk`
  random-ticks, before its own `randomTick`. Change the block with the level's `native_set_block_property(ticks, pos,
  key, value, flags)`; set `handled` to skip the block's own random tick.
- **`native_tick_block(ticks, dimension, pos, state, handled)`** (API 34, on `GameLevel`): a due scheduled tick of a block
  that is still there (`ServerLevel.tickBlock`), before its own `tick`; the same contract as `native_random_tick`.
- **`native_explosion_detonate(dimension, center, blocks)`** (API 22): the positions an explosion is about to break,
  before any is hit; take out the ones to spare. Entities are hurt as before.
- **`native_fill_bucket(stack, pos, canceled)`** and **`native_attack_entity(entity, kind, pos, canceled)`** (API 22):
  a player's bucket at a block, and their blow on an entity, with the acting player swapped in. Canceled, the bucket
  does nothing and the blow does no harm.

### Mobs

API 37, `crates/horizon-client/src/game/native_mobs.rs`: helpers for server mods, for the level in place, on a server only (on a
client they do nothing, and the readers give nothing). Each takes the `dimension` (`minecraft:overworld`) of the level it means, as
`native_set_block` does, and is refused when another level is in place. The per-level tick hooks (API 23) run with a defined level in
place; `native_server_tick` does not define one, so name the dimension and expect a refusal there.

- `native_spawn_mob(dimension, kind, pos, &NativeSpawn)` spawns a mob and returns its id. `NativeSpawn` holds an optional name and health, and the
  `no_ai`, `invisible`, `silent`, `invulnerable` and `no_gravity` flags; its default is a plain mob. The mob is finalized as a natural spawn
  is, and kept when its chunk unloads. Nothing is spawned outside `/summon`'s bounds, in a chunk that isn't loaded, or a monster on peaceful.
- `native_mob(dimension, id)` and `native_mobs(dimension)` read the living mobs: id, dimension, kind, position, yaw, health and maximum health.
- `native_teleport_mob(dimension, id, pos)` (as `/tp`: it dismounts and drops its path), `native_push_mob(dimension, id, velocity)` and
  `native_set_mob_health(dimension, id, health)` change one, and answer false for no such mob or a value that isn't finite. Health must be
  above 0.
- `native_kill_mob(dimension, id)` kills as `/kill` does, with its loot; `native_remove_mob(dimension, id)` takes the mob away with no death
  and no loot (it is not saved either).
- While the game ticks its mobs they are out of the level: from a hook inside that tick, a spawned mob joins after the tick, and the others
  read as gone.

### Entities

API 60, `crates/horizon-client/src/game/native_entities.rs`: any entity of the level in place, for server mods (a
server's `Game` only; elsewhere the readers give nothing and the setters answer false). Each takes the `dimension` of
the level it means, as the [mob helpers](#mobs) do. Players, mobs, item entities, minecarts, boats and armor stands
are reached; ids are entity ids, the ones the mob, item and hook helpers give.

- `native_entity(dimension, id)` and `native_entities_in(dimension, min, max)` read them as `NativeEntity`: id, kind
  (`minecraft:player`...), feet, motion, yaw, ground, whether a player sneaks or flies (API 64), the vehicle it rides
  and its box. A player's motion is the
  server's copy: their client moves them.
- `native_set_delta_movement(dimension, id, velocity)` sets the motion, in blocks a tick. A player's client is sent it
  as a knockback is (`ClientboundSetEntityMotionPacket`), which is how a fan or a belt moves a player.
- `native_reset_fall_distance(dimension, id)`: the entity has fallen nowhere yet, so landing doesn't hurt it.
- `native_set_yaw(dimension, id, yaw)`: degrees, 0 facing south. A mob's head turns with it, and its AI may turn it
  again; a player's client is turned by the difference from the yaw the server last heard
  (`ClientboundPlayerRotationPacket`, relative), so their own mouse movement on the way is kept.
- A player inside their own hook (`native_player_tick`, `native_use_item_on`...) has their state swapped in and is not
  reached: call these from `native_level_tick_post` or `native_entity_inside`, where every player of the level is.

### Minecarts

API 62, `crates/horizon-client/src/game/native_minecarts.rs`, for server mods.

- A content block with `properties.rail` is a rail to Horizon's minecarts (`OldMinecartBehavior`): they are placed on
  it, follow it along its `shape` state property and climb its slopes as on `minecraft:rail`, with no powered or
  activator rail's behaviour. Vanilla's rails are known by name, so they and every other block cost one bit read more.
- `native_minecart_pass(dimension, pos, state, cart)`: a cart is on such a rail, as its tick starts to follow it,
  once a tick. `cart.velocity` (blocks a tick) is the motion it goes on with; the track then turns it along the
  rail, as it turns a powered rail's boost. Change this cart through `cart` only.
- `native_minecart(dimension, id)` and `native_minecarts(dimension)` read the level's carts as `NativeMinecart`: id,
  UUID (kept in the save: key a mod's own saved data on it), kind, position, motion, yaw and riders (players and mobs, by
  entity id). Ids last while the world is open, so a mod may keep what it tracks about a cart in a map by id, and drop
  an entry once `native_minecart` gives nothing. API 60's `native_set_delta_movement` moves a cart from any hook.
- `native_spawn_minecart(dimension, kind, pos, yaw, velocity)` (API 65): a new cart of a vanilla kind
  (`minecraft:minecart`, `minecraft:chest_minecart`...) in a loaded chunk, as `EntityType.create` and
  `Level.addFreshEntity`; its id. `native_remove_minecart(dimension, id)` (API 65): `Entity.discard`, nothing dropped,
  its riders let off first. A mod that keeps something on a cart (a contraption riding it) spawns and picks it up so.
- `native_set_minecart_position(dimension, id, pos)` (API 68): `Entity.setPos`, its motion kept (two linked carts
  pulled back to their length after a collision). The mod keeps it on its rail; clients see it move as any move.
- `native_set_furnace_fuel(dimension, id, fuel, push)` (API 68): a furnace cart's fuel ticks and push (blocks a
  tick, its way and strength), as a player's coal sets them; `NativeMinecart.fuel` and `push` read them (0 and none
  for other kinds). Fuel runs out as vanilla's does. False for another kind.
- `native_set_minecart_carrying(dimension, id, carrying)` (API 68): the cart carries something of the mod's that
  isn't an entity (a contraption) and moves as a cart with a passenger does (`isVehicle`: 0.997 of its speed kept a
  tick instead of 0.96, three quarters of its motion moved). One flag read in the cart's tick; not saved, so the mod
  sets it again when the world opens. `NativeMinecart.carrying` reads it.
- These setters act on the level in place, from `native_level_tick_post` or another hook outside the cart's own
  tick: inside `native_minecart_pass`, change that cart through `cart` only.

```rust
// A powered-rail-like booster: carts on it speed up along their motion, to 0.4 blocks a tick.
fn native_minecart_pass(&mut self, _dimension: &str, _pos: BlockPos, state: BlockStateId, cart: &mut NativeMinecart) {
    if !my_booster(state) {
        return;
    }
    let [x, y, z] = cart.velocity;
    let speed = x.hypot(z);
    if speed > 0.01 {
        let boost = (speed + 0.06).min(0.4) / speed;
        cart.velocity = [x * boost, y, z * boost];
    }
}
```

### Riding

API 63, `crates/horizon-client/src/game/native_riding.rs`, for server mods, on the level in place as the
[entity helpers](#entities) are.

- `native_spawn_seat(dimension, pos, yaw)`: a seat, an empty `minecraft:item_display` that every client (vanilla's
  too) draws as nothing and seats its riders on itself; its id. A player on it stands 0.6 below `pos` (`Player`'s
  vehicle attachment), a mob by its own attachment. A seat saves as vanilla's vehicles do (API 67): with its chunk and
  the mobs riding it, or, ridden by a player, in that player's file (`RootVehicle`), leaving the world with them.
- `native_seat_loaded(dimension, seat, pos)`: a seat came back from a save (NeoForge's `EntityJoinLevelEvent` with
  `loadedFromDisk`), under a new id, its mobs on it; a player whose file held it gets on in the same tick. Take it
  back into the mod's state in `native_level_tick_post`, or remove it.
- `native_move_seat(dimension, id, pos, yaw)`: its riders go with it, and clients glide it over vanilla's 3
  interpolation steps, so a move every tick is a smooth ride (a train, a contraption).
  `native_remove_seat(dimension, id)` lets its riders off first.
- `native_start_riding(dimension, rider, vehicle)` and `native_stop_riding(dimension, rider)`: a player rides a seat, a
  boat or a minecart; a mob a seat, or a mob by vanilla's rules. Getting off puts a player where the seat is, teleported
  there as vanilla's `dismountTo`; a minecart or boat lets them off as when they sneak.
- `native_passengers(dimension, vehicle)`: who rides it, players then mobs, by entity id. `native_entity(..).vehicle`
  says what an entity rides.
- `native_entity_mount(dimension, rider, vehicle, mounting, canceled)`: a player gets on a vehicle by using it, or
  sneaks to get off any vehicle; `canceled` keeps them as they were (a seat a player can't leave while the train moves).
  The players are out of the helpers' reach inside it: act on it in `native_level_tick_post`.
- A player turns on their own; `native_set_yaw` turns them with a turning seat.

```rust
// Seat a player on a chair block at `pos`, then carry them along x, one block a second.
let seat = self.native_spawn_seat(dim, [x + 0.5, y + 0.5 + 0.6, z + 0.5], 0.0)?; // feet on the half slab
self.native_start_riding(dim, player_id, seat);
// Each tick, in native_level_tick_post:
self.native_move_seat(dim, seat, [x + 0.5 + t as f64 * 0.05, y + 1.1, z + 0.5], 0.0);
// Gone: the player stands where the seat was.
self.native_remove_seat(dim, seat);
```

### Mod messages

API 40, `crates/horizon-client/src/game/native_net.rs`: bytes between a mod's server half and its client half, on the
one custom payload channel `horizon:mod_net`. A vanilla client never gets them, and a vanilla server isn't sent any.

- `native_net_send_to_player(player, channel, data)` on the server and `native_net_send_to_server(channel, data)` on the
  client send one. The channel is `mod_id:name`: the id `[a-z0-9_]`, the name `[a-z0-9_./-]`, each 1 to 64 characters;
  `minecraft` and `horizon` are the game's. The data is at most 30 000 bytes, and 32 messages a tick go each way per
  player. Ok means accepted: a player who left doesn't get it. Sending to the server is an error until the player has
  joined.
- `native_net_from_client(player, channel, data)` and `native_net_from_server(channel, data)` hear them, all mods on every
  message: check the channel. What a client sent is checked by nothing, so treat it as a player's own input.
- A player's message reaches the server's hook in the tick after it arrives, when the helpers can reach every player;
  a message to a player leaves at the end of the tick, from any server hook.

### Server menus

API 10, `crates/horizon-client/src/game/native_menus.rs`: menus a server mod builds from ordinary items, which a
stock client shows as a chest (`generic_9x1` to `generic_9x6`); nothing is installed on the player's side.

- `native_open_menu(uuid, rows, title, items)` opens one in place of the player's open menu (its window id);
  `native_set_menu_items(uuid, items)` changes what it shows; `native_close_menu(uuid)` closes it.
- **No click moves anything.** Each reaches `native_menu_clicked(uuid, window, slot, button, mode)`, and the client
  is sent the menu, its inventory and its cursor as they were. `slot` counts the menu's own slots first, then the
  player's inventory below it: `native_menu_inventory_slot(size, slot)` gives the inventory slot (0..9 the hotbar).
- `native_menu_closed(uuid, window)`: the player closed it, another menu opened, or the player left.
- `native_player_stacks(uuid)`: the player's 36 inventory slots, to show copies or offer them.
- `native_exchange_items(a, a_offer, b, b_offer)`: each offer is inventory slots, each with the exact stack it must
  still hold. Both offers leave their slots and go to the other player where they fit whole (onto matching stacks,
  then empty slots), or nothing moves and the error says why (an offer that changed, no room, a player gone or
  dead). A trade on these menus leaves the offered stacks in their owners' inventories until this one step, so
  closing the menu, leaving or dying moves nothing.

The menu hooks run with no player's state swapped in; `native_player_stacks` and `native_exchange_items` refuse a
player whose state is (inside `native_perform_command`): call them from a menu hook or `native_server_tick`.
