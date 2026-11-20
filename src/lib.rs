//! Viltrumite for Horizon: powers given by a command, flight (hover, cruise, sonic), strength,
//! durability, a dash, grab and throw, the thunderclap, the dive's impact, a HUD, an ability menu
//! and the character's poses. Fan-made, written from scratch; not affiliated with Image Comics,
//! Skybound or Amazon, and no code or assets from the Java mod "Viltrumite".
//!
//! The server keeps who has powers and changes the damage they take and deal; the client flies.
//! Flight sets the player's own velocity, so it works in worlds you host (single player, LAN).
//!
//! The game makes one `Mod` and calls its hooks; the state lives in the two statics below, so the
//! payload handlers (registered once, in `register`) reach it too.
use mod_api::{
    self, CameraShakeEvent, ClientHost, ClientTickPreEvent, ComputeFovModifierEvent, Flow,
    GuiLayerDrawEvent, Hooks, InputKeyEvent, LevelRandomHost, LivingIncomingDamageEvent,
    LivingKnockbackEvent, MovementInputEvent, PlayerLoggedInEvent, Registrar, RegistrationContext,
    RegisterCommandsEvent, RegisterGuiLayersEvent, RenderPlayerPreEvent, RunModCommandEvent,
    ScreenMouseButtonPressedPreEvent, ScreenMouseButtonReleasedPreEvent, ScreenMouseDraggedPreEvent,
    ScreenRenderPostEvent, ServerHost, ServerStoppedEvent, ServerTickEvent, SettingChangedEvent,
};
use std::sync::LazyLock;

pub mod abilities;
mod client;
pub mod hud;
pub mod moves;
mod net;
pub mod physics;
mod player_model;
pub mod poses;
mod server;
mod skins;
pub mod sounds;

/// The mod's id, as in `horizon-mod.toml`.
pub const MOD_ID: &str = "viltrumite";

static SERVER: LazyLock<server::Server> = LazyLock::new(server::Server::default);
static CLIENT: LazyLock<client::Client> = LazyLock::new(client::Client::default);

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        net::register(r);
        server::register(r);
        client::register(r);
    }

    // The server's side.

    fn register_commands(&self, _host: &mut dyn ServerHost, event: &mut RegisterCommandsEvent<'_>) {
        server::Server::register_commands(event);
    }

    fn run_mod_command(&self, host: &mut dyn ServerHost, event: &mut RunModCommandEvent<'_>) -> Flow {
        SERVER.run_command(host, event)
    }

    fn player_logged_in(&self, host: &mut dyn ServerHost, event: &mut PlayerLoggedInEvent<'_>) {
        SERVER.logged_in(host, event);
    }

    fn server_tick(&self, host: &mut dyn ServerHost, event: &mut ServerTickEvent<'_>) {
        SERVER.tick(host, event);
    }

    fn server_stopped(&self, _host: &mut dyn ServerHost, _event: &mut ServerStoppedEvent) {
        SERVER.stopped();
    }

    fn living_incoming_damage(&self, _level: &mut dyn LevelRandomHost, event: &mut LivingIncomingDamageEvent<'_>) -> Flow {
        SERVER.incoming_damage(event)
    }

    fn living_knockback(&self, _level: &mut dyn LevelRandomHost, event: &mut LivingKnockbackEvent<'_>) -> Flow {
        SERVER.knockback(event);
        Flow::Continue
    }

    // The window's side.

    fn client_tick_pre(&self, host: &mut dyn ClientHost, _event: &mut ClientTickPreEvent) {
        CLIENT.tick(host);
    }

    fn movement_input(&self, _host: &dyn ClientHost, event: &mut MovementInputEvent) {
        CLIENT.movement_input(event);
    }

    fn input_key(&self, host: &mut dyn ClientHost, event: &mut InputKeyEvent<'_>) {
        CLIENT.key(host, event);
    }

    fn setting_changed(&self, host: &mut dyn ClientHost, event: &mut SettingChangedEvent<'_>) {
        if event.mod_id == MOD_ID {
            CLIENT.reread_settings(host);
        }
    }

    fn register_gui_layers(&self, _context: &RegistrationContext, event: &mut RegisterGuiLayersEvent<'_>) {
        client::register_gui_layers(event);
    }

    fn gui_layer_draw(&self, host: &dyn ClientHost, event: &mut GuiLayerDrawEvent<'_>) {
        CLIENT.hud(host, event);
    }

    fn camera_shake(&self, _host: &dyn ClientHost, event: &mut CameraShakeEvent<'_>) {
        CLIENT.camera_shake(event);
    }

    fn compute_fov_modifier(&self, _host: &dyn ClientHost, event: &mut ComputeFovModifierEvent<'_>) {
        CLIENT.fov(event);
    }

    fn screen_render_post(&self, host: &dyn ClientHost, event: &mut ScreenRenderPostEvent<'_>) {
        CLIENT.menu_render(host, event);
    }

    fn screen_mouse_button_pressed_pre(&self, host: &dyn ClientHost, event: &mut ScreenMouseButtonPressedPreEvent<'_>) -> Flow {
        CLIENT.menu_press(host, event.screen, event.button)
    }

    fn screen_mouse_button_released_pre(&self, host: &dyn ClientHost, event: &mut ScreenMouseButtonReleasedPreEvent<'_>) -> Flow {
        CLIENT.menu_release(host, event.screen)
    }

    fn screen_mouse_dragged_pre(&self, host: &dyn ClientHost, _event: &mut ScreenMouseDraggedPreEvent<'_>) -> Flow {
        CLIENT.menu_drag(host)
    }

    fn render_player_pre(&self, host: &dyn ClientHost, event: &mut RenderPlayerPreEvent<'_>) -> Flow {
        CLIENT.render_player(host, event)
    }
}
