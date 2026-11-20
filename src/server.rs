//! The server's side: who has powers and who chose to stay human (saved with the world), the
//! dialog's answer and the command that set it, the damage powered players take and deal, the moves
//! clients ask for (the sonic tunnel too), and the poses they share.
//!
//! The damage hooks name entities by id, so each tick the powered players' entity ids are noted for
//! them. A punch marks its victim for the knockback of the same hit. The state lock is never held
//! across a call that hurts something: the hurt runs this mod's damage hook.
use crate::moves::{
    CLAP_COOLDOWN, CLAP_COS, CLAP_DAMAGE, CLAP_PUSH, CLAP_REACH, GRAB_RADIUS, GRAB_REACH, Kind, Move,
    SLAM_COOLDOWN, SLAM_MIN_SPEED, THROW_DAMAGE, THROW_SPEED, add, hold_point, in_cone, length, pick_target,
    TUNNEL_START, push_away, scale, slam_damage_at, slam_radius, sub, tunnel_cells, tunnels,
};
use crate::net;
use crate::poses::PoseMsg;
use crate::skins::{SkinMsg, skin_of_textures};
use crate::sounds::{Audience, Cue, RINGING, echo_delay, far_volume, layers};
use mod_api::{
    Command, CommandArgumentValue, CommandSource, ConfigPath, EntityRef, ExplosionInteraction,
    ExplosionResistance, Flow, LevelRef,
    LivingIncomingDamageEvent, LivingKnockbackEvent, ParticleKey, ParticleOptions,
    PlayerLoggedInEvent, PlayerRef, RegisterCommandsEvent, Registrar, RunModCommandEvent, ServerHost,
    ServerTickEvent, SoundSource, Text,
};
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, MutexGuard, OnceLock};

/// The vanilla particles the moves and the flight show (the client's too), asked for once at
/// registration.
pub struct Particles {
    pub gust: ParticleKey,
    pub sonic_boom: ParticleKey,
    pub explosion_emitter: ParticleKey,
    pub poof: ParticleKey,
    pub explosion: ParticleKey,
    pub cloud: ParticleKey,
}

static PARTICLES: OnceLock<Particles> = OnceLock::new();

pub fn register(r: &mut dyn Registrar) {
    let _ = PARTICLES.set(Particles {
        gust: r.particle_key("minecraft:gust"),
        sonic_boom: r.particle_key("minecraft:sonic_boom"),
        explosion_emitter: r.particle_key("minecraft:explosion_emitter"),
        poof: r.particle_key("minecraft:poof"),
        explosion: r.particle_key("minecraft:explosion"),
        cloud: r.particle_key("minecraft:cloud"),
    });
}

/// One of the particles, once registered.
pub fn particle(which: fn(&Particles) -> ParticleKey) -> Option<ParticleKey> {
    PARTICLES.get().map(which)
}

/// Share of damage a Viltrumite still takes.
const DURABILITY: f32 = 0.2;
/// Melee damage multiplier.
const STRENGTH: f32 = 4.0;
/// Melee knockback multiplier: punches send mobs flying.
const PUNCH: f64 = 4.0;
/// Damage that ignores durability: the void, `/kill`, the world border.
const ALWAYS_HURTS: [&str; 3] = ["minecraft:out_of_world", "minecraft:generic_kill", "minecraft:outside_border"];
/// Damage a Viltrumite never takes.
const NEVER_HURTS: [&str; 2] = ["minecraft:fall", "minecraft:fly_into_wall"];
/// Eye height of a standing player.
const EYE_HEIGHT: f64 = 1.62;
/// Where the lists are kept, in the world's own mod data: who has powers, and who chose to stay human.
const POWERED_FILE: &str = "powered.txt";
const HUMAN_FILE: &str = "human.txt";
/// Blocks that resist explosions this much or more stay put in a tunnel: water and lava (100),
/// obsidian, bedrock, portals and their like.
const TUNNEL_SPARES: f32 = 100.0;
/// The tunnel sounds at most this often, in ticks: it cuts every tick.
const TUNNEL_SOUND_EVERY: u64 = 6;

#[derive(Default)]
pub struct Server {
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// Whether the lists were read for the world open now.
    loaded: bool,
    /// Players with powers, by UUID: saved.
    powered: HashSet<u128>,
    /// Players who chose to stay human, by UUID: saved. Neither list: not asked yet.
    human: HashSet<u128>,
    /// Their entity ids, for the damage hooks (refreshed each tick).
    ids: HashSet<i32>,
    /// Entity ids a powered player punched this tick, waiting for the hit's knockback.
    punched: HashSet<i32>,
    /// Server ticks since the world opened, for cooldowns.
    tick: u64,
    /// The mob each player holds.
    held: HashMap<u128, EntityRef>,
    /// When each player may use a move again: (player, move) to tick.
    ready_at: HashMap<(u128, u8), u64>,
    /// Sounds waiting for their tick.
    pending: Vec<Pending>,
    /// Powered players (by entity id) who landed a punch since the last tick, for its sound.
    punchers: Vec<i32>,
    /// The powered players by entity id.
    by_id: HashMap<i32, u128>,
}

/// A sound to play later: at a place, or to one player.
struct Pending {
    due: u64,
    play: Play,
}

#[derive(Clone, Copy)]
enum Play {
    At(LevelRef, [f64; 3], &'static str, f32, f32),
    To(PlayerRef, &'static str, f32, f32),
}

impl Server {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn register_commands(event: &mut RegisterCommandsEvent<'_>) {
        event.commands.push(Command::new("viltrumite power"));
        event.commands.push(Command::new("viltrumite depower"));
        event.commands.push(Command::op("viltrumite power <player:player>", 2));
        event.commands.push(Command::op("viltrumite depower <player:player>", 2));
    }

    /// The lists of the world open now, read the first time they are needed.
    fn load(&self, host: &mut dyn ServerHost) {
        if self.state().loaded {
            return;
        }
        let powered = read_set(host, POWERED_FILE);
        let human = read_set(host, HUMAN_FILE);
        log::info!("Viltrumite: {} powered and {} human in this world", powered.len(), human.len());
        let mut state = self.state();
        state.powered = powered;
        state.human = human;
        state.loaded = true;
    }

    fn save(&self, host: &mut dyn ServerHost) {
        let (powered, human) = {
            let state = self.state();
            (state.powered.clone(), state.human.clone())
        };
        write_set(host, POWERED_FILE, &powered);
        write_set(host, HUMAN_FILE, &human);
    }

    /// `player` chose (in the dialog or with the command): powers or none, saved either way.
    fn choose(&self, host: &mut dyn ServerHost, player: PlayerRef, give: bool) {
        {
            let mut state = self.state();
            if give {
                state.human.remove(&player.uuid());
            } else {
                state.human.insert(player.uuid());
            }
        }
        self.set_powered(host, player, give);
    }

    /// The dialog's answer from `player`'s client: one byte, 1 to be a Viltrumite.
    pub fn chose(&self, host: &mut dyn ServerHost, player: PlayerRef, data: &[u8]) {
        let give = match data {
            [1] => true,
            [0] => false,
            _ => return,
        };
        self.load(host);
        self.choose(host, player, give);
        let line = if give {
            "Você agora é um Viltrumita. Toque duas vezes no pulo para voar."
        } else {
            "Você continua humano. Para mudar: /viltrumite power"
        };
        let _ = player.tell(host, Text::literal(line));
        log::info!("Viltrumite: {} chose powered {give}", player.name(host).unwrap_or_default());
    }

    /// `/viltrumite power|depower [player]`.
    pub fn run_command(&self, host: &mut dyn ServerHost, event: &mut RunModCommandEvent<'_>) -> Flow {
        let literals = &event.command.literals;
        if literals.first().map(String::as_str) != Some("viltrumite") {
            return Flow::Continue;
        }
        let give = match literals.get(1).map(String::as_str) {
            Some("power") => true,
            Some("depower") => false,
            _ => return Flow::Continue,
        };
        let sender = match event.source {
            CommandSource::Player { player, .. } => Some(*player),
            _ => None,
        };
        let target = match event.command.argument("player") {
            Some(CommandArgumentValue::Player(player)) => Some(*player),
            _ => sender,
        };
        let Some(target) = target else {
            let _ = host.commands().feedback(Text::literal("Diga qual jogador."));
            return Flow::Cancel;
        };
        self.load(host);
        self.choose(host, target, give);
        let name = target.name(host).unwrap_or_default();
        let own = sender.is_some_and(|s| s.uuid() == target.uuid());
        let line = match (give, own) {
            (true, true) => "Você agora é um Viltrumita. Toque duas vezes no pulo para voar.".to_owned(),
            (false, true) => "Você não tem mais poderes.".to_owned(),
            (true, false) => format!("{name} agora é um Viltrumita."),
            (false, false) => format!("{name} não tem mais poderes."),
        };
        let _ = host.commands().feedback(Text::literal(line));
        if !own {
            let told = if give { "Você agora é um Viltrumita." } else { "Você não tem mais poderes." };
            let _ = target.tell(host, Text::literal(told));
        }
        log::info!("Viltrumite: {name} powered {give}");
        Flow::Cancel
    }

    fn set_powered(&self, host: &mut dyn ServerHost, player: PlayerRef, give: bool) {
        {
            let mut state = self.state();
            if give {
                state.powered.insert(player.uuid());
            } else {
                state.powered.remove(&player.uuid());
                state.held.remove(&player.uuid());
            }
        }
        self.save(host);
        net::to_player(host, player, net::POWER, vec![u8::from(give)]);
        if !give {
            net::to_player(host, player, net::HELD, vec![0]);
        }
    }

    pub fn logged_in(&self, host: &mut dyn ServerHost, event: &mut PlayerLoggedInEvent<'_>) {
        self.load(host);
        let (powered, chose) = {
            let mut state = self.state();
            state.held.remove(&event.player);
            let powered = state.powered.contains(&event.player);
            (powered, powered || state.human.contains(&event.player))
        };
        if let Some(player) = host.player(event.player) {
            net::to_player(host, player, net::POWER, vec![u8::from(powered)]);
            net::to_player(host, player, net::HELD, vec![0]);
            if !chose {
                net::to_player(host, player, net::ASK, vec![1]);
            }
            share_skins(host, player);
        }
        log::info!("Viltrumite: {} joined, powered {powered}, asked {}", event.name, !chose);
    }

    /// Once a server tick: the powered players' entity ids, last tick's punches forgotten (and
    /// sounded), and the sounds whose time came.
    pub fn tick(&self, host: &mut dyn ServerHost, _event: &mut ServerTickEvent<'_>) {
        let (powered, punchers, due) = {
            let mut state = self.state();
            state.tick += 1;
            state.punched.clear();
            let now = state.tick;
            let (due, later): (Vec<Pending>, Vec<Pending>) = std::mem::take(&mut state.pending).into_iter().partition(|p| p.due <= now);
            state.pending = later;
            let punchers: Vec<u128> = std::mem::take(&mut state.punchers).iter().filter_map(|id| state.by_id.get(id).copied()).collect();
            (state.powered.iter().copied().collect::<Vec<u128>>(), punchers, due)
        };
        for pending in due {
            play(host, pending.play);
        }
        let mut by_id = HashMap::new();
        for uuid in powered {
            if let Some(id) = host.player(uuid).and_then(|p| i32::try_from(p.entity().id()).ok()) {
                by_id.insert(id, uuid);
            }
        }
        {
            let mut state = self.state();
            state.ids = by_id.keys().copied().collect();
            state.by_id = by_id;
        }
        let mut sounded = HashSet::new();
        for uuid in punchers {
            if !sounded.insert(uuid) {
                continue;
            }
            let Some(player) = host.player(uuid) else { continue };
            let (Some(level), Some(at)) = (player.level(host), player.entity().position(host)) else { continue };
            self.cue(host, Some(player), level, add(at, [0.0, 1.0, 0.0]), Cue::Punch);
        }
    }

    /// `cue` happening at `at` in `level`, `actor` doing it: its layers now or queued, each to
    /// whom it is for (design: sounds.rs).
    fn cue(&self, host: &mut dyn ServerHost, actor: Option<PlayerRef>, level: LevelRef, at: [f64; 3], cue: Cue) {
        let mut plays: Vec<(u32, Play)> = Vec::new();
        for l in layers(cue) {
            match l.audience {
                Audience::Actor => {
                    if let Some(actor) = actor {
                        plays.push((l.delay, Play::To(actor, l.sound, l.volume, l.pitch)));
                    }
                }
                Audience::Here => plays.push((l.delay, Play::At(level, at, l.sound, l.volume, l.pitch))),
                Audience::Far => {
                    for player in level.online_players(host).unwrap_or_default() {
                        let Some(feet) = player.entity().position(host) else { continue };
                        let distance = length(sub(feet, at));
                        if let Some(volume) = far_volume(l.volume, distance) {
                            plays.push((l.delay + echo_delay(distance), Play::To(player, l.sound, volume, l.pitch)));
                        }
                    }
                }
            }
        }
        let mut state = self.state();
        let now = state.tick;
        let mut right_now = Vec::new();
        for (delay, play) in plays {
            if delay == 0 {
                right_now.push(play);
            } else {
                state.pending.push(Pending { due: now + u64::from(delay), play });
            }
        }
        drop(state);
        for play in right_now {
            play_now(host, play);
        }
    }

    /// The clap's ringing, in the ears of the other players in its cone.
    fn ring(&self, host: &mut dyn ServerHost, actor: PlayerRef, level: LevelRef, m: &Move) {
        for player in level.online_players(host).unwrap_or_default() {
            if player.uuid() == actor.uuid() {
                continue;
            }
            let Some(feet) = player.entity().position(host) else { continue };
            if in_cone(m.eye, m.look, feet, CLAP_REACH, CLAP_COS) {
                play_now(host, Play::To(player, RINGING.sound, RINGING.volume, RINGING.pitch));
            }
        }
    }

    /// The world closed: the next one reads its own list.
    pub fn stopped(&self) {
        *self.state() = State::default();
    }

    pub fn incoming_damage(&self, event: &mut LivingIncomingDamageEvent<'_>) -> Flow {
        let mut state = self.state();
        let source = event.source;
        if state.ids.contains(&event.entity) {
            if NEVER_HURTS.contains(&source.damage_type) {
                event.cancelled = true;
                return Flow::Cancel;
            }
            if !ALWAYS_HURTS.contains(&source.damage_type) {
                let amount = event.container.new_damage() * DURABILITY;
                event.container.set_new_damage(amount);
            }
        }
        if is_punch(&state.ids, event.entity, source.causing, source.direct) {
            let amount = event.container.new_damage() * STRENGTH;
            event.container.set_new_damage(amount);
            state.punched.insert(event.entity);
            state.punchers.push(source.causing);
        }
        Flow::Continue
    }

    pub fn knockback(&self, event: &mut LivingKnockbackEvent<'_>) {
        let mut state = self.state();
        let punched = state.punched.remove(&event.entity)
            || is_punch(&state.ids, event.entity, event.source.causing, event.source.direct);
        if punched {
            event.strength *= PUNCH;
        }
    }

    /// A move from `player`'s client.
    pub fn moved(&self, host: &mut dyn ServerHost, player: PlayerRef, data: &[u8]) {
        let Some(m) = Move::decode(data) else {
            log::warn!("Viltrumite: a malformed move");
            return;
        };
        let uuid = player.uuid();
        let (powered, held) = {
            let state = self.state();
            (state.powered.contains(&uuid), state.held.get(&uuid).copied())
        };
        if !powered {
            return;
        }
        let Some(level) = host.level(&m.dimension) else {
            return;
        };
        match m.kind {
            Kind::Hold => {
                if let Some(mob) = held
                    && !mob.teleport_to(host, hold_point(m.eye, m.look), None)
                {
                    // Dead, unloaded or gone to another dimension: the hands are free.
                    self.release(host, player);
                }
            }
            Kind::Grab => match held {
                Some(mob) => self.throw(host, player, level, mob, &m),
                None => self.grab(host, player, level, &m),
            },
            Kind::Clap => {
                if self.ready(uuid, Kind::Clap, CLAP_COOLDOWN) {
                    clap(host, player, level, &m);
                    self.cue(host, Some(player), level, m.eye, Cue::Clap);
                    self.ring(host, player, level, &m);
                }
            }
            Kind::Slam => {
                if m.speed >= SLAM_MIN_SPEED && self.ready(uuid, Kind::Slam, SLAM_COOLDOWN) {
                    slam(host, player, level, &m);
                    self.cue(host, Some(player), level, sub(m.eye, [0.0, EYE_HEIGHT, 0.0]), Cue::Impact);
                }
            }
            Kind::Tunnel => {
                if tunnel(host, player, level, &m) > 0 && self.ready(uuid, Kind::Tunnel, TUNNEL_SOUND_EVERY) {
                    self.cue(host, Some(player), level, add(m.eye, scale(m.look, 2.0)), Cue::Tunnel);
                }
            }
            Kind::Boom => self.cue(host, Some(player), level, m.eye, Cue::Sonic),
            Kind::Drop => {
                if held.is_some() {
                    self.release(host, player);
                }
            }
        }
    }

    /// Whether `player` may use `kind` now; if so, its cooldown starts.
    fn ready(&self, player: u128, kind: Kind, cooldown: u64) -> bool {
        let mut state = self.state();
        let now = state.tick;
        let key = (player, kind as u8);
        if state.ready_at.get(&key).is_some_and(|&at| now < at) {
            return false;
        }
        state.ready_at.insert(key, now + cooldown);
        true
    }

    fn grab(&self, host: &mut dyn ServerHost, player: PlayerRef, level: LevelRef, m: &Move) {
        let mobs = mobs_near(host, player, level, m.eye, GRAB_REACH);
        let candidates: Vec<(u32, [f64; 3])> = mobs.iter().map(|(e, at)| (e.id(), *at)).collect();
        let Some(id) = pick_target(m.eye, m.look, &candidates, GRAB_REACH, GRAB_RADIUS) else {
            return;
        };
        let Some(&(mob, _)) = mobs.iter().find(|(e, _)| e.id() == id) else {
            return;
        };
        if !mob.teleport_to(host, hold_point(m.eye, m.look), None) {
            return;
        }
        self.state().held.insert(player.uuid(), mob);
        net::to_player(host, player, net::HELD, vec![1]);
        self.cue(host, Some(player), level, m.eye, Cue::Grab);
    }

    fn throw(&self, host: &mut dyn ServerHost, player: PlayerRef, level: LevelRef, mob: EntityRef, m: &Move) {
        self.release(host, player);
        mob.teleport_to(host, hold_point(m.eye, m.look), None);
        mob.hurt(host, "minecraft:generic", THROW_DAMAGE, None, None);
        mob.push(host, add(scale(m.look, THROW_SPEED), [0.0, 0.25, 0.0]));
        self.cue(host, Some(player), level, m.eye, Cue::Throw);
        particles(host, level, |p| p.gust, hold_point(m.eye, m.look), [0.0; 3], 1);
    }

    fn release(&self, host: &mut dyn ServerHost, player: PlayerRef) {
        self.state().held.remove(&player.uuid());
        net::to_player(host, player, net::HELD, vec![0]);
    }

    /// A client's pose news: passed to every other player, by name.
    pub fn relay_pose(&self, host: &mut dyn ServerHost, player: PlayerRef, data: &[u8]) {
        let Some(msg) = PoseMsg::decode(data) else {
            return;
        };
        let name = player.name(host).unwrap_or_default();
        let relay = msg.relay(&name);
        for other in host.players().server_players() {
            if other.uuid() != player.uuid() {
                net::to_player(host, other, net::POSE_OF, relay.clone());
            }
        }
    }
}

/// The skin of `player` (from their profile), if they have one of their own.
fn skin_msg(host: &dyn ServerHost, player: PlayerRef) -> Option<SkinMsg> {
    let profile = player.profile(host)?;
    let property = profile.properties.iter().find(|p| p.name == "textures")?;
    let (texture, slim) = skin_of_textures(&property.value)?;
    Some(SkinMsg { name: profile.name, texture, slim })
}

/// Someone joined: their skin to everyone (themself included), and everyone else's to them.
fn share_skins(host: &mut dyn ServerHost, joined: PlayerRef) {
    let online = host.players().server_players();
    let mine = skin_msg(host, joined);
    if let Some(msg) = &mine {
        log::info!("Viltrumite: {} has skin `{}` (slim {})", msg.name, msg.texture, msg.slim);
    }
    for other in online.iter().copied().filter(|p| p.uuid() != joined.uuid()).chain([joined]) {
        if let Some(msg) = &mine {
            net::to_player(host, other, net::SKIN, msg.encode());
        }
        if other.uuid() != joined.uuid()
            && let Some(theirs) = skin_msg(host, other)
        {
            net::to_player(host, joined, net::SKIN, theirs.encode());
        }
    }
}

/// A queued sound whose time came (the same as one played now).
fn play(host: &mut dyn ServerHost, play: Play) {
    play_now(host, play);
}

fn play_now(host: &mut dyn ServerHost, play: Play) {
    match play {
        Play::At(level, at, sound, volume, pitch) => {
            level.play_sound(host, sound, at, SoundSource::Players, volume, pitch);
        }
        Play::To(player, sound, volume, pitch) => {
            player.play_sound(host, sound, SoundSource::Players, volume, pitch);
        }
    }
}

/// A list of UUIDs from the world's mod data (one hex UUID a line); empty when there is none.
fn read_set(host: &dyn ServerHost, file: &str) -> HashSet<u128> {
    let text = ConfigPath::new(file)
        .ok()
        .and_then(|path| host.world_data_files().read(&path).ok())
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default();
    text.lines().filter_map(|l| u128::from_str_radix(l.trim(), 16).ok()).collect()
}

fn write_set(host: &dyn ServerHost, file: &str, set: &HashSet<u128>) {
    let mut uuids: Vec<_> = set.iter().map(|u| format!("{u:032x}")).collect();
    uuids.sort();
    let text = uuids.join("\n") + "\n";
    let written = ConfigPath::new(file)
        .map_err(|e| format!("{e:?}"))
        .and_then(|path| host.world_data_files().write(&path, text.as_bytes()).map_err(|e| format!("{e:?}")));
    if let Err(error) = written {
        log::warn!("Viltrumite: could not save {file}: {error}");
    }
}

/// The sonic flight's tunnel: the blocks ahead of the body vanish, without drops, but for liquids
/// and the blocks explosions can't break. Refused for a steep dive (that one ends in an impact)
/// and for a start far from the player. Answers how many blocks it cut.
fn tunnel(host: &mut dyn ServerHost, player: PlayerRef, level: LevelRef, m: &Move) -> i32 {
    if !tunnels(m.look) {
        return 0;
    }
    let near = player.entity().position(host).is_some_and(|feet| length(sub(m.eye, feet)) <= TUNNEL_START);
    if !near {
        return 0;
    }
    let mut removed = 0;
    for cell in tunnel_cells(m.eye, m.look, m.speed) {
        let centre = cell.map(|c| f64::from(c) + 0.5);
        let resistance =
            level.explosion_resistance(host, cell, None, centre, 1.0, false, ExplosionInteraction::Block);
        let cut = matches!(resistance, Some(ExplosionResistance::Resists(r)) if r < TUNNEL_SPARES);
        if cut && level.remove_block(host, cell, false) {
            removed += 1;
        }
    }
    if removed > 0 {
        let ahead = add(m.eye, scale(m.look, 2.0));
        particles(host, level, |p| p.poof, ahead, [1.2; 3], removed.min(24));
    }
    removed
}

fn clap(host: &mut dyn ServerHost, player: PlayerRef, level: LevelRef, m: &Move) {
    for (mob, feet) in mobs_near(host, player, level, m.eye, CLAP_REACH) {
        if in_cone(m.eye, m.look, feet, CLAP_REACH, CLAP_COS) {
            mob.hurt(host, "minecraft:sonic_boom", CLAP_DAMAGE, None, None);
            mob.push(host, add(scale(m.look, CLAP_PUSH), [0.0, 0.4, 0.0]));
        }
    }
    for step in 1..=6 {
        let at = add(m.eye, scale(m.look, f64::from(step) * 1.6));
        particles(host, level, |p| p.sonic_boom, at, [0.0; 3], 1);
    }
}

fn slam(host: &mut dyn ServerHost, player: PlayerRef, level: LevelRef, m: &Move) {
    let feet = sub(m.eye, [0.0, EYE_HEIGHT, 0.0]);
    let radius = slam_radius(m.speed);
    for (mob, at) in mobs_near(host, player, level, feet, radius) {
        let distance = length(sub(at, feet));
        if distance <= radius {
            mob.hurt(host, "minecraft:explosion", slam_damage_at(m.speed, distance), None, None);
            mob.push(host, push_away(feet, at, 1.0 + m.speed * 0.5, 0.6));
        }
    }
    let spread = radius * 0.4;
    particles(host, level, |p| p.explosion_emitter, feet, [0.0; 3], 1);
    particles(host, level, |p| p.poof, add(feet, [0.0, 0.2, 0.0]), [spread, 0.1, spread], 60);
    particles(host, level, |p| p.explosion, feet, [spread, 0.3, spread], 6);
    log::info!("Viltrumite: impact at {:.1} b/t, radius {radius:.1}", m.speed);
}

fn particles(
    host: &mut dyn ServerHost,
    level: LevelRef,
    which: fn(&Particles) -> ParticleKey,
    at: [f64; 3],
    spread: [f64; 3],
    count: i32,
) {
    let Some(keys) = PARTICLES.get() else {
        return;
    };
    let speed = if count > 1 { 0.05 } else { 0.0 };
    level.send_particles(host, &ParticleOptions::simple(which(keys)), None, at, spread, speed, count);
}

/// The living mobs (players aside) within `reach` of `centre` on each axis, with where they stand.
fn mobs_near(host: &mut dyn ServerHost, actor: PlayerRef, level: LevelRef, centre: [f64; 3], reach: f64) -> Vec<(EntityRef, [f64; 3])> {
    let entities = level.entities_in(host, sub(centre, [reach; 3]), add(centre, [reach; 3])).unwrap_or_default();
    // Every player in the level (the list can be empty in a world only one plays), and the one acting.
    let mut players: HashSet<u32> = level.online_players(host).unwrap_or_default().into_iter().map(|p| p.entity().id()).collect();
    players.insert(actor.entity().id());
    entities
        .into_iter()
        .filter(|e| !players.contains(&e.id()) && host.living(*e).is_some())
        .filter_map(|e| Some((e, e.position(host)?)))
        .collect()
}

/// A powered player's own blow: their hand or weapon, not an arrow they shot (whose direct entity
/// is the arrow). A blow whose direct entity is not known (0) counts as their own.
fn is_punch(ids: &HashSet<i32>, victim: i32, causing: i32, direct: i32) -> bool {
    causing != 0 && causing != victim && ids.contains(&causing) && (direct == 0 || direct == causing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_powered_players_own_blow_is_a_punch() {
        let ids = HashSet::from([7]);
        assert!(is_punch(&ids, 3, 7, 7));
        assert!(is_punch(&ids, 3, 7, 0), "direct entity not known");
        assert!(!is_punch(&ids, 3, 7, 12), "an arrow they shot");
        assert!(!is_punch(&ids, 3, 8, 8), "someone without powers");
        assert!(!is_punch(&ids, 7, 7, 7), "hurting themselves");
        assert!(!is_punch(&ids, 3, 0, 0), "no attacker");
    }
}
