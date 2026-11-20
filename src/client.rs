//! The window's side: flight, the dash, the camera, the HUD, the ability menu and the poses, for a
//! player the server gave powers.
//!
//! The flight steers from how far the player moved last tick, and pushes from what the game's own
//! travel leaves of that move (gravity, then air drag) to the velocity it wants ([`Flight::drive`]).
//! A wrong guess only adds itself to one tick's move, and the next tick measures it again.
//!
//! The moves go to the server as [`Move`]s: grab and throw, the clap, and a dive that the ground
//! stops is an impact. The server decides what they hit.
use crate::abilities::{
    Ability, DEFAULT_SHAKE, DEFAULT_SPEED, KEYS, Loadout, MENU_KEY, RELEASE, SHAKES, SLOTS, SPEEDS, Settings,
    slot_key,
};
use crate::hud::{self, Choice, ChoiceLayout, HudView, Menu, MenuAction, MenuLayout};
use crate::moves::{self, CLAP_COOLDOWN, Kind, Move};
use crate::physics::{self, Flight, Input, Mode, Step, Tuning, View};
use crate::player_model::Poser;
use crate::poses::{Act, Base, PoseMsg};
use crate::skins::SkinMsg;
use crate::sounds::{self, Audience, Cue};
use crate::{MOD_ID, net, server};
use mod_api::{
    CameraShakeEvent, ClientHost, ComputeFovModifierEvent, ConfigPath, Flow, GuiLayer, GuiLayerDrawEvent,
    GuiLayerPlacement, InputKeyEvent, KeyAction, KeyConflictContext, KeyRegistration,
    MovementInputEvent, ParticleOptions, RegisterGuiLayersEvent, Registrar, RenderPlayerPreEvent,
    ScreenRenderPostEvent, ScreenView, SettingSpec, SoundPlay, SoundRef, SoundSource,
};
use std::sync::{Mutex, MutexGuard};

/// A move this long in one tick was a teleport, not flight.
const TELEPORT: f64 = 12.0;
/// Eye height of a standing player.
const EYE_HEIGHT: f64 = 1.62;
/// The HUD's layer.
const HUD_LAYER: &str = "viltrumite:hud";
/// The loadout and how often the menu was opened, in the mod's data folder.
const LOADOUT_FILE: &str = "loadout.txt";
const MENU_OPENED_FILE: &str = "menu_opened.txt";

/// Keys, settings and the HUD's icons, once at start.
pub fn register(r: &mut dyn Registrar) {
    for (id, label, key) in KEYS.into_iter().chain([RELEASE]) {
        r.register_key_mapping(KeyRegistration::new(id, key).label(label).context(KeyConflictContext::InGame));
    }
    let percent = SHAKES.map(|s| format!("{s}%"));
    let speeds = SPEEDS.map(|s| format!("{s} m/s"));
    let settings = [
        SettingSpec::toggle("hud", "HUD", true),
        SettingSpec::choice("shake", "Tremor de câmera", percent, DEFAULT_SHAKE),
        SettingSpec::choice("max_speed", "Velocidade máxima de voo", speeds, DEFAULT_SPEED),
        SettingSpec::toggle("fov", "Campo de visão aumenta no voo", true),
        SettingSpec::toggle("tunnel", "Quebrar blocos no sônico", true),
        SettingSpec::choice("sfx", "Volume do vento e efeitos do voo", SHAKES.map(|s| format!("{s}%")), DEFAULT_SHAKE),
    ];
    for setting in settings {
        if let Err(error) = r.register_setting(setting) {
            log::warn!("Viltrumite: a setting was refused: {error:?}");
        }
    }
    hud::register(r);
}

pub fn register_gui_layers(event: &mut RegisterGuiLayersEvent<'_>) {
    event.layers.push(GuiLayer {
        id: HUD_LAYER.to_owned(),
        placement: GuiLayerPlacement::Above("minecraft:hotbar".to_owned()),
    });
}

#[derive(Default)]
pub struct Client {
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    powered: bool,
    flight: Flight,
    tuning: Tuning,
    /// The latest keys from `movement_input`.
    input: Input,
    jump_held: bool,
    boost_held: bool,
    /// Presses waiting for the next tick.
    jump_pressed: bool,
    /// Whether the server says a mob is in our hands.
    holding: bool,
    /// The tick the clap is ready again (the server keeps its own cooldown too).
    clap_ready_at: u32,
    /// A move's camera jolt, 0..=1, fading each tick.
    jolt: f32,
    /// The settings and loadout, read the first time they are needed.
    settings: Settings,
    loadout: Loadout,
    settings_read: bool,
    last_position: Option<[f64; 3]>,
    /// The vertical velocity the flight last asked for (to tell a blocked descent).
    last_vertical: f64,
    last_step: Option<Step>,
    /// How far the player really moved last tick, b/t (the HUD's speed).
    measured: f64,
    /// Ticks flown, for the camera shake's wobble.
    ticks: u32,
    /// The ability menu: its drag, a close asked from a click (closed on the next tick), and how
    /// often it was opened (the HUD's hint goes after three).
    menu: Menu,
    close_menu: bool,
    menu_opened: u32,
    /// Which of the mod's screens is open when one is.
    screen: Screen,
    /// The server asked for the choice (design/escolha); the answer clicked, sent on the next tick.
    ask: bool,
    chosen: Option<bool>,
    /// The wind loop playing and its pitch; the tick the quiet before a sonic boom ends.
    wind: Option<(SoundRef, f32)>,
    quiet_until: u32,
    /// Each slot's cooldown last frame, and until when it flashes for being ready.
    last_left: [u32; SLOTS],
    flash_until: [u32; SLOTS],
    /// The poses of the players in view (design/poses), and the last pose told to the server.
    poser: Poser,
    pose_sent: Option<(Option<Base>, bool)>,
}

/// The mod's own screens.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Screen {
    /// The ability menu (K).
    #[default]
    Menu,
    /// The choice on joining a world.
    Choice,
}

/// The flight mode's pose.
fn base_of(mode: Mode) -> Option<Base> {
    match mode {
        Mode::Grounded => None,
        Mode::Hover => Some(Base::Hover),
        Mode::Cruise => Some(Base::Cruise),
        Mode::Sonic => Some(Base::Sonic),
    }
}

/// The player's keys: the four slots, then the menu (vanilla key names).
fn bindings(host: &dyn ClientHost) -> ([String; SLOTS], String) {
    let bound = |id: &str| {
        let default = KEYS.iter().find(|k| k.0 == id).map_or("", |k| k.2);
        host.mod_key(MOD_ID, id).map_or_else(|| default.to_owned(), |(key, _)| key)
    };
    (std::array::from_fn(|i| bound(slot_key(i))), bound(MENU_KEY))
}

fn data_path(name: &str) -> Option<ConfigPath> {
    ConfigPath::new(name).ok()
}

fn read_data(host: &dyn ClientHost, name: &str) -> Option<String> {
    let bytes = host.data_files().read(&data_path(name)?).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn write_data(host: &dyn ClientHost, name: &str, text: &str) {
    let written = data_path(name).map(|path| host.data_files().write(&path, text.as_bytes()));
    if !matches!(written, Some(Ok(_))) {
        log::warn!("Viltrumite: could not save {name}");
    }
}

/// A sound at this player's ears, on the players' slider: `after` ticks from now, `looping` or not.
fn local(host: &dyn ClientHost, id: &str, volume: f32, pitch: f32, after: u32, looping: bool) -> Option<SoundRef> {
    let mut play = SoundPlay::ui(id).with_volume(volume).with_pitch(pitch);
    play.source = SoundSource::Players;
    if looping {
        play = play.looping(0);
    }
    host.play_local_sound(&play, after)
}

/// The layers of `cue` this player hears, at `scale` of their volume (the effects setting).
fn cue(host: &dyn ClientHost, cue: Cue, scale: f32) {
    if scale <= 0.0 {
        return;
    }
    for l in sounds::layers(cue).iter().filter(|l| l.audience == Audience::Actor) {
        let _ = local(host, l.sound, l.volume * scale, l.pitch, l.delay, false);
    }
}

/// What is over the world.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Over {
    /// Nothing: keys apply.
    Nothing,
    /// The chat box or the ability menu: keep flying as you are, no keys.
    Typing,
    /// The death screen.
    Death,
    /// The pause menu, an inventory: hold still.
    Other,
}

fn over(host: &dyn ClientHost) -> Over {
    if host.mod_screen() == Some(MOD_ID) {
        return Over::Typing;
    }
    match host.observed_screen_id() {
        None => Over::Nothing,
        Some(screen) => match screen.id.as_str() {
            "chat" => Over::Typing,
            "death" => Over::Death,
            _ => Over::Other,
        },
    }
}

impl Client {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// A message from the server on `channel`.
    pub fn message(&self, channel: &str, data: &[u8]) {
        let mut state = self.state();
        match channel {
            net::HELD => state.holding = data.first() == Some(&1),
            net::POSE_OF => {
                if let Some((name, msg)) = PoseMsg::unrelay(data) {
                    state.poser.heard(name, msg);
                }
            }
            net::SKIN => {
                if let Some(msg) = SkinMsg::decode(data) {
                    state.poser.skin(msg);
                }
            }
            net::ASK => state.ask = data.first() == Some(&1),
            net::POWER => {
                state.powered = data.first() == Some(&1);
                log::info!("Viltrumite: the server says powered {}", state.powered);
                if !state.powered {
                    state.flight.reset();
                    state.last_step = None;
                }
            }
            _ => {}
        }
    }

    pub fn movement_input(&self, event: &mut MovementInputEvent) {
        let mut state = self.state();
        if event.jump && !state.jump_held {
            state.jump_pressed = true;
        }
        state.jump_held = event.jump;
        state.boost_held = event.sprint;
        state.input.forward = event.forward;
        state.input.strafe = event.strafe;
        state.input.up = event.jump;
        state.input.down = event.sneak;
    }

    /// The menu's key closes the menu while it is open (the game's own clicks don't count then).
    pub fn key(&self, host: &mut dyn ClientHost, event: &mut InputKeyEvent<'_>) {
        if event.action != KeyAction::Press || host.mod_screen() != Some(MOD_ID) || self.state().screen != Screen::Menu {
            return;
        }
        if event.key == bindings(host).1 {
            host.mod_screen_close(MOD_ID);
        }
    }

    /// The settings page changed: read it again.
    pub fn reread_settings(&self, host: &dyn ClientHost) {
        let mut state = self.state();
        state.settings_read = false;
        Self::read_settings(&mut state, host);
    }

    /// The settings, loadout and menu count, read the first time they are needed.
    fn read_settings(state: &mut State, host: &dyn ClientHost) {
        if state.settings_read {
            return;
        }
        state.settings = Settings::from_page(
            host.setting_bool("hud"),
            host.setting_choice("shake"),
            host.setting_choice("max_speed"),
            host.setting_bool("fov"),
            host.setting_bool("tunnel"),
            host.setting_choice("sfx"),
        );
        state.loadout = Loadout::decode(&read_data(host, LOADOUT_FILE).unwrap_or_default());
        state.menu_opened = read_data(host, MENU_OPENED_FILE).and_then(|v| v.trim().parse().ok()).unwrap_or(0);
        state.tuning.max_speed = state.settings.max_speed_bt();
        state.settings_read = true;
    }

    pub fn tick(&self, host: &mut dyn ClientHost) {
        let mut state = self.state();
        state.poser.clock += 1.0;
        if std::mem::take(&mut state.close_menu) {
            host.mod_screen_close(MOD_ID);
        }
        if let Some(give) = state.chosen.take() {
            net::to_server(host, net::CHOOSE, &[u8::from(give)]);
            if give {
                cue(host, Cue::Awaken, f32::from(state.settings.sfx) / 100.0);
            }
        }
        let Some(player) = host.local_player() else {
            // No world, or between worlds.
            state.poser.forget();
            state.pose_sent = None;
            state.flight.reset();
            state.last_step = None;
            state.last_position = None;
            state.settings_read = false;
            return;
        };
        Self::read_settings(&mut state, host);
        let entity = player.entity();
        let (Some(position), Some([yaw, pitch])) = (entity.position(host), entity.rotation(host)) else {
            return;
        };

        // The keys pressed since last tick (none count while a screen is open).
        let mut slots = [false; SLOTS];
        for (slot, pressed) in slots.iter_mut().enumerate() {
            while host.consume_click(MOD_ID, slot_key(slot)) {
                *pressed = true;
            }
        }
        let mut release = false;
        while host.consume_click(MOD_ID, RELEASE.0) {
            release = true;
        }
        let mut open_menu = false;
        while host.consume_click(MOD_ID, MENU_KEY) {
            open_menu = true;
        }

        let over = over(host);
        if over == Over::Death {
            state.flight.reset();
            state.last_step = None;
        }
        if over == Over::Other {
            // The pause menu, an inventory: hold still, and measure from where we are after it.
            state.last_position = None;
            state.jump_pressed = false;
            return;
        }

        // Not chosen yet in this world: the choice, as soon as nothing is over the world. Esc
        // closes it undecided; the server asks again next time.
        if state.ask && over == Over::Nothing && host.mod_screen_open(MOD_ID) {
            state.ask = false;
            state.screen = Screen::Choice;
        } else if open_menu && state.powered && host.mod_screen_open(MOD_ID) {
            state.screen = Screen::Menu;
            state.menu = Menu::default();
            state.menu_opened += 1;
            write_data(host, MENU_OPENED_FILE, &state.menu_opened.to_string());
        }

        let moved = match state.last_position.replace(position) {
            Some(last) => {
                let moved = [position[0] - last[0], position[1] - last[1], position[2] - last[2]];
                if length(moved) > TELEPORT { [0.0; 3] } else { moved }
            }
            None => [0.0; 3],
        };
        state.measured = length(moved);
        if !state.powered {
            state.jump_pressed = false;
            Self::wind(&mut state, host, false, 0.0, 0.0);
            // Powers gone mid-pose: the others see this player stand again.
            if state.pose_sent.is_some_and(|sent| sent != (None, false)) {
                net::to_server(host, net::POSE, &PoseMsg::default().encode());
                state.pose_sent = Some((None, false));
            }
            return;
        }

        let playing = over == Over::Nothing;
        let fired = |ability: Ability| playing && (0..SLOTS).any(|i| slots[i] && state.loadout.slot(i) == Some(ability));
        let (dash, grab, clap) = (fired(Ability::Dash), fired(Ability::Grab), fired(Ability::Clap));
        let jump_pressed = std::mem::take(&mut state.jump_pressed);
        let input = if playing {
            Input { boost: state.boost_held, jump_pressed, dash_pressed: dash, ..state.input }
        } else {
            Input::default()
        };
        let view = View { yaw, pitch };
        let eye = [position[0], position[1] + EYE_HEIGHT, position[2]];
        let look = view.look();
        let dimension = player.dimension(host).map(|d| d.to_string()).unwrap_or_default();
        let send = |host: &mut dyn ClientHost, kind: Kind, speed: f64| {
            let m = Move { kind, eye, look, speed, dimension: dimension.clone() };
            net::to_server(host, net::MOVE, &m.encode());
        };

        // The move that starts a pose this tick, if any (design/poses).
        let mut act = None;

        // A fast dive the ground stopped: the impact, and the flight stops there.
        let diving = state.last_step.is_some_and(|s| matches!(s.mode, Mode::Cruise | Mode::Sonic));
        if diving && moves::impact(state.last_vertical, moved[1]) {
            let speed = state.last_step.map_or(0.0, |s| s.speed);
            state.flight.impact();
            state.jolt = 1.0;
            send(host, Kind::Slam, speed);
            act = Some(Act::Impact);
        }

        // Asked to go down and didn't: standing on something.
        let blocked_down = state.last_vertical < -0.05 && moved[1] > state.last_vertical * 0.2;
        let tuning = state.tuning;
        let (step, kick) = state.flight.drive(&tuning, &input, view, moved, blocked_down);
        if step.dashed {
            act = act.or(Some(Act::Dash));
        }

        if release && playing && state.holding {
            send(host, Kind::Drop, 0.0);
        } else if grab {
            send(host, Kind::Grab, 0.0);
            if state.holding {
                act = Some(Act::Throw);
            }
        }
        if clap && state.ticks >= state.clap_ready_at {
            state.clap_ready_at = state.ticks + CLAP_COOLDOWN as u32;
            state.jolt = state.jolt.max(0.6);
            send(host, Kind::Clap, 0.0);
            act = Some(Act::Clap);
        }

        // Sonic: the blocks ahead go, two ticks' flight and a little more, unless diving (design/escolha).
        if step.mode == Mode::Sonic && state.settings.tunnel && moves::tunnels(look) {
            send(host, Kind::Tunnel, step.speed * 2.0 + 3.0);
        }

        // The pose: this player's own, and the news for the others when it changes.
        let now = state.poser.clock;
        if let Some(act) = act {
            state.poser.own_act.start(act, now);
        }
        let pose = (base_of(step.mode), state.holding);
        if act.is_some() || state.pose_sent != Some(pose) {
            let msg = PoseMsg { base: pose.0, holding: pose.1, act };
            net::to_server(host, net::POSE, &msg.encode());
            state.pose_sent = Some(pose);
        }
        if state.holding {
            send(host, Kind::Hold, 0.0);
        }
        let middle = moves::add(position, [0.0, 0.9, 0.0]);
        if step.mode == Mode::Sonic
            && let Some(cloud) = server::particle(|p| p.cloud)
        {
            // A trail of cloud behind, seen by this player.
            let behind = moves::sub(middle, moves::scale(look, 1.5));
            host.add_particle(&ParticleOptions::simple(cloud), behind, [0.0; 3]);
        }
        if step.entered_sonic
            && let Some(boom) = server::particle(|p| p.sonic_boom)
        {
            host.add_particle(&ParticleOptions::simple(boom), middle, [0.0; 3]);
        }
        state.jolt = (state.jolt - 0.06).max(0.0);

        if let Some(kick) = kick {
            host.push_player(kick.map(|c| c as f32));
        }
        state.last_vertical = step.velocity.map_or(0.0, |wanted| wanted[1]);

        // The sounds (sounds.rs): the moments, and the wind of the flight.
        let sfx = f32::from(state.settings.sfx) / 100.0;
        if step.took_off {
            cue(host, Cue::TakeOff, sfx);
        }
        if step.entered_sonic {
            // Half a second of quiet, then the boom (the server plays it, for the others too).
            send(host, Kind::Boom, 0.0);
            state.quiet_until = state.ticks + sounds::SONIC_QUIET;
        }
        if step.dashed {
            cue(host, Cue::Dash, sfx);
        }
        if step.landed && act != Some(Act::Impact)
            && let Some((volume, pitch)) = sounds::landing(state.last_step.map_or(0.0, |s| s.speed))
        {
            let l = sounds::layers(Cue::Landing)[0];
            let _ = local(host, l.sound, volume * sfx, pitch, 0, false);
        }
        let quiet = state.ticks < state.quiet_until;
        Self::wind(&mut state, host, step.mode.flying() && !quiet, step.speed, sfx);

        state.ticks = state.ticks.wrapping_add(1);
        state.last_step = Some(step);
    }

    /// The wind loop: louder and higher the faster, started, retuned or hushed as the flight goes.
    /// A new pitch needs a new loop: the old one fades out as the new one fades in.
    fn wind(state: &mut State, host: &dyn ClientHost, flying: bool, speed: f64, sfx: f32) {
        let (volume, pitch) = sounds::wind(flying, speed, state.tuning.max_speed);
        let volume = volume * sfx;
        if volume <= 0.0 {
            if let Some((sound, _)) = state.wind.take() {
                sound.fade(host, 0.0, 8);
            }
            return;
        }
        let retune = state.wind.is_none_or(|(_, playing)| (playing - pitch).abs() >= 0.15);
        if retune {
            if let Some((old, _)) = state.wind.take() {
                old.fade(host, 0.0, 6);
            }
            if let Some(sound) = local(host, sounds::WIND, 0.01, pitch, 0, true) {
                state.wind = Some((sound, pitch));
            }
        }
        if let Some((sound, _)) = state.wind
            && !sound.fade(host, volume, 4)
        {
            // Gone (a level change clears every sound): start again next tick.
            state.wind = None;
        }
    }

    /// The HUD (design/hud-menu, 1-13), in its own layer.
    pub fn hud(&self, host: &dyn ClientHost, event: &mut GuiLayerDrawEvent<'_>) {
        if event.layer != HUD_LAYER {
            return;
        }
        let gui = event.draw.size();
        let (keys, menu_key) = bindings(host);
        let view = {
            let mut state = self.state();
            if !state.powered || !state.settings.hud {
                return;
            }
            let mode = state.last_step.map_or(Mode::Grounded, |s| s.mode);
            let cooldowns: [(u32, u32); SLOTS] = std::array::from_fn(|i| match state.loadout.slot(i) {
                Some(Ability::Dash) => (state.flight.dash_left(), state.tuning.dash_cooldown),
                Some(Ability::Clap) => (state.clap_ready_at.saturating_sub(state.ticks), CLAP_COOLDOWN as u32),
                _ => (0, 0),
            });
            let now = state.ticks;
            for (i, &(left, _)) in cooldowns.iter().enumerate() {
                if state.last_left[i] > 0 && left == 0 {
                    state.flash_until[i] = now + 4;
                }
                state.last_left[i] = left;
            }
            HudView {
                gui,
                mode: match mode {
                    Mode::Grounded => "No chão",
                    Mode::Hover => "Pairando",
                    Mode::Cruise => "Cruzeiro",
                    Mode::Sonic => "SÔNICO",
                },
                sonic: mode == Mode::Sonic,
                speed_mps: physics::metres_per_second(state.measured),
                speed_fill: hud::speed_fill(state.measured, state.tuning.max_speed),
                sonic_mark: state.tuning.sonic_share as f32,
                holding: state.holding,
                loadout: state.loadout,
                keys,
                menu_key: hud::short_key(&menu_key),
                show_hint: state.menu_opened < 3,
                cooldowns,
                flash: std::array::from_fn(|i| now < state.flash_until[i]),
            }
        };
        hud::draw_hud(event.draw, (4.0, 4.0), &view);
    }

    /// A player in view (this one in third person): drawn in a pose while one is on.
    pub fn render_player(&self, host: &dyn ClientHost, event: &mut RenderPlayerPreEvent<'_>) -> Flow {
        let mut state = self.state();
        let own = if state.powered {
            (state.last_step.and_then(|s| base_of(s.mode)), state.holding)
        } else {
            (None, false)
        };
        state.poser.render(host, event, own)
    }

    /// The ability menu (design/hud-menu, 14-22), drawn over the world while it is open.
    pub fn menu_render(&self, host: &dyn ClientHost, event: &mut ScreenRenderPostEvent<'_>) {
        if host.mod_screen() != Some(MOD_ID) {
            return;
        }
        let (keys, menu_key) = bindings(host);
        let (which, menu, loadout) = {
            let state = self.state();
            (state.screen, state.menu, state.loadout)
        };
        let screen = event.screen;
        let gui = [screen.rect[2], screen.rect[3]];
        match which {
            Screen::Menu => hud::draw_menu(event.draw, gui, screen.mouse, &menu, &loadout, &keys, &hud::short_key(&menu_key)),
            Screen::Choice => hud::draw_choice(event.draw, gui, screen.mouse),
        }
    }

    pub fn menu_press(&self, host: &dyn ClientHost, screen: &ScreenView<'_>, button: i32) -> Flow {
        if host.mod_screen() != Some(MOD_ID) {
            return Flow::Continue;
        }
        let gui = [screen.rect[2], screen.rect[3]];
        let mut state = self.state();
        if state.screen == Screen::Choice {
            // A card's click is the answer; it goes on the next tick, and the screen closes then.
            if button == hud::LEFT
                && let Some(choice) = ChoiceLayout::new(gui).at(screen.mouse)
            {
                state.chosen = Some(choice == Choice::Viltrumite);
                state.close_menu = true;
            }
            return Flow::Cancel;
        }
        let layout = MenuLayout::new(gui);
        let mut loadout = state.loadout;
        let action = state.menu.press(&layout, screen.mouse, button, &mut loadout);
        Self::menu_action(&mut state, host, action, loadout);
        Flow::Cancel
    }

    pub fn menu_release(&self, host: &dyn ClientHost, screen: &ScreenView<'_>) -> Flow {
        if host.mod_screen() != Some(MOD_ID) {
            return Flow::Continue;
        }
        let layout = MenuLayout::new([screen.rect[2], screen.rect[3]]);
        let mut state = self.state();
        if state.screen == Screen::Choice {
            return Flow::Cancel;
        }
        let mut loadout = state.loadout;
        let action = state.menu.release(&layout, screen.mouse, &mut loadout);
        Self::menu_action(&mut state, host, action, loadout);
        Flow::Cancel
    }

    /// A drag on the menu: the card follows the mouse when it is drawn; nothing else takes it.
    pub fn menu_drag(&self, host: &dyn ClientHost) -> Flow {
        if host.mod_screen() == Some(MOD_ID) { Flow::Cancel } else { Flow::Continue }
    }

    fn menu_action(state: &mut State, host: &dyn ClientHost, action: MenuAction, loadout: Loadout) {
        match action {
            MenuAction::None => {}
            MenuAction::Changed => {
                state.loadout = loadout;
                write_data(host, LOADOUT_FILE, &loadout.encode());
            }
            MenuAction::Close => state.close_menu = true,
        }
    }

    pub fn camera_shake(&self, event: &mut CameraShakeEvent<'_>) {
        let state = self.state();
        let speed = state.last_step.map_or(0.0, |s| s.speed);
        let amount = (physics::shake(&state.tuning, speed) as f32).max(state.jolt) * f32::from(state.settings.shake) / 100.0;
        if amount <= 0.0 {
            return;
        }
        let t = state.ticks as f32 + event.partial;
        event.shake.pitch += (t * 1.9).sin() * 0.8 * amount;
        event.shake.yaw += (t * 2.3 + 1.0).sin() * 0.8 * amount;
        event.shake.roll += (t * 1.3 + 2.0).sin() * 0.5 * amount;
    }

    pub fn fov(&self, event: &mut ComputeFovModifierEvent<'_>) {
        let state = self.state();
        if !state.settings.fov {
            return;
        }
        let Some(step) = state.last_step.filter(|s| s.mode.flying()) else {
            return;
        };
        let scale = physics::fov_scale(&state.tuning, step.speed) as f32;
        event.modifier = (event.modifier * scale).clamp(0.01, 10.0);
    }
}

fn length(v: [f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}
