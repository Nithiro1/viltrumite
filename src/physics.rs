//! Viltrumite flight, as a pure state machine: one [`Flight::step`] per client tick (20 a second).
//! Velocities are in blocks per tick (1 b/t = 20 m/s). No game types: the mod's hooks read the
//! player into [`Input`] and [`View`], and write back what [`Step`] says.

/// The flight modes. Grounded leaves movement to the game; the others set the velocity every tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Grounded,
    /// Floating in place: WASD, jump and sneak move you; no gravity.
    Hover,
    /// Flying where you look, speeding up while sprint is held.
    Cruise,
    /// Cruise near top speed: the camera shakes and the view widens.
    Sonic,
}

impl Mode {
    pub fn flying(self) -> bool {
        self != Mode::Grounded
    }
}

/// The keys this tick, as the game read them.
#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    /// W/S, -1..=1.
    pub forward: f32,
    /// A/D, -1..=1, positive to the left (as the game's own input).
    pub strafe: f32,
    /// Jump held.
    pub up: bool,
    /// Sneak held.
    pub down: bool,
    /// Sprint held.
    pub boost: bool,
    /// Jump went down this tick (for the double tap).
    pub jump_pressed: bool,
    /// The dash key went down this tick.
    pub dash_pressed: bool,
}

/// Where the player looks, in the game's degrees.
#[derive(Clone, Copy, Debug, Default)]
pub struct View {
    pub yaw: f32,
    pub pitch: f32,
}

impl View {
    /// The unit vector the player looks along (the game's `Entity.calculateViewVector`).
    pub fn look(self) -> [f64; 3] {
        let (yaw, pitch) = (f64::from(self.yaw).to_radians(), f64::from(self.pitch).to_radians());
        [-yaw.sin() * pitch.cos(), -pitch.sin(), yaw.cos() * pitch.cos()]
    }

    /// `strafe` and `forward` turned by the yaw into world x and z (the game's `moveRelative`).
    pub fn horizontal(self, strafe: f64, forward: f64) -> [f64; 2] {
        let yaw = f64::from(self.yaw).to_radians();
        let (sin, cos) = yaw.sin_cos();
        [strafe * cos - forward * sin, forward * cos + strafe * sin]
    }
}

/// The numbers that make the flight feel the way it does.
#[derive(Clone, Copy, Debug)]
pub struct Tuning {
    /// Hover speed across, b/t.
    pub hover_speed: f64,
    /// Hover speed up and down, b/t.
    pub hover_vertical: f64,
    /// How much of the gap to the wanted hover velocity closes each tick, 0..=1.
    pub hover_response: f64,
    /// Top speed, b/t.
    pub max_speed: f64,
    /// Sonic from this share of the top speed on.
    pub sonic_share: f64,
    /// Speed gained each tick while sprint is held, b/t.
    pub accel: f64,
    /// Speed lost each tick without sprint, b/t.
    pub decel: f64,
    /// Two jump presses at most this many ticks apart take off or land.
    pub double_tap_ticks: u32,
    /// Speed a dash adds along the look, b/t.
    pub dash_speed: f64,
    /// Ticks between dashes.
    pub dash_cooldown: u32,
}

impl Default for Tuning {
    fn default() -> Self {
        Tuning {
            hover_speed: 0.55,
            hover_vertical: 0.45,
            hover_response: 0.25,
            max_speed: 4.0,
            sonic_share: 0.8,
            accel: 0.06,
            decel: 0.15,
            double_tap_ticks: 7,
            dash_speed: 2.5,
            dash_cooldown: 20,
        }
    }
}

impl Tuning {
    pub fn sonic_speed(&self) -> f64 {
        self.max_speed * self.sonic_share
    }
}

/// What one tick decided.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Step {
    pub mode: Mode,
    /// The velocity to give the player this tick, or `None` to leave the game's own.
    pub velocity: Option<[f64; 3]>,
    /// Speed along the flight, b/t (for the HUD).
    pub speed: f64,
    pub took_off: bool,
    pub landed: bool,
    pub entered_sonic: bool,
    pub dashed: bool,
}

/// One player's flight between ticks.
#[derive(Clone, Debug)]
pub struct Flight {
    mode: Mode,
    speed: f64,
    tick: u32,
    last_jump: Option<u32>,
    dash_ready_at: u32,
}

impl Default for Flight {
    fn default() -> Self {
        Flight { mode: Mode::Grounded, speed: 0.0, tick: 0, last_jump: None, dash_ready_at: 0 }
    }
}

impl Flight {
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Ticks until the dash is ready again.
    pub fn dash_left(&self) -> u32 {
        self.dash_ready_at.saturating_sub(self.tick)
    }

    /// A dive hit the ground: the speed is spent, and the flight hovers where it landed.
    pub fn impact(&mut self) {
        if self.mode.flying() {
            self.mode = Mode::Hover;
            self.speed = 0.0;
        }
    }

    /// Back to the ground at once (powers taken away, death, another world).
    pub fn reset(&mut self) {
        *self = Flight { tick: self.tick, ..Flight::default() };
    }

    /// One tick as the game runs it: [`Flight::step`] from the player's last move, and the push
    /// ([`push_towards`]) that gives them the velocity it wants, if it wants one.
    pub fn drive(&mut self, t: &Tuning, input: &Input, view: View, moved: [f64; 3], on_ground: bool) -> (Step, Option<[f64; 3]>) {
        let step = self.step(t, input, view, moved, on_ground);
        let kick = step.velocity.map(|wanted| push_towards(moved, wanted));
        (step, kick)
    }

    /// One client tick. `velocity` is how far the player really moved last tick: their velocity
    /// during that move, before the game took gravity and drag off it ([`after_air_move`]). Steering
    /// from it, not from what gravity left, keeps a hover from leaking a little fall every tick.
    /// `on_ground`: they came down onto something last tick (the flight asked to go down and they
    /// didn't), which ends the flight.
    pub fn step(&mut self, t: &Tuning, input: &Input, view: View, velocity: [f64; 3], on_ground: bool) -> Step {
        self.tick = self.tick.wrapping_add(1);
        let mut step = Step {
            mode: self.mode,
            velocity: None,
            speed: 0.0,
            took_off: false,
            landed: false,
            entered_sonic: false,
            dashed: false,
        };

        // Double tap jump: take off, or land where you are.
        if input.jump_pressed {
            match self.last_jump {
                Some(last) if self.tick.wrapping_sub(last) <= t.double_tap_ticks => {
                    self.last_jump = None;
                    if self.mode.flying() {
                        self.mode = Mode::Grounded;
                        self.speed = 0.0;
                        step.landed = true;
                    } else {
                        self.mode = Mode::Hover;
                        step.took_off = true;
                    }
                }
                _ => self.last_jump = Some(self.tick),
            }
        }
        // Coming down onto the ground lands: hovering, cruising, or after an impact.
        if self.mode.flying() && on_ground && !step.took_off {
            self.mode = Mode::Grounded;
            self.speed = 0.0;
            step.landed = true;
        }

        let look = view.look();
        let dash = input.dash_pressed && self.tick >= self.dash_ready_at;
        if dash {
            self.dash_ready_at = self.tick + t.dash_cooldown;
            step.dashed = true;
        }

        match self.mode {
            Mode::Grounded => {
                if dash {
                    // Along the ground: across only, with a small hop.
                    let across = view.horizontal(0.0, 1.0);
                    step.velocity = Some([
                        velocity[0] + across[0] * t.dash_speed,
                        velocity[1].max(0.2),
                        velocity[2] + across[1] * t.dash_speed,
                    ]);
                }
                step.speed = length(velocity);
            }
            Mode::Hover => {
                let (mut strafe, mut forward) = (f64::from(input.strafe), f64::from(input.forward));
                let size = (strafe * strafe + forward * forward).sqrt();
                if size > 1.0 {
                    strafe /= size;
                    forward /= size;
                }
                let across = view.horizontal(strafe * t.hover_speed, forward * t.hover_speed);
                let vertical = match (input.up, input.down) {
                    (true, false) => t.hover_vertical,
                    (false, true) => -t.hover_vertical,
                    _ => 0.0,
                };
                let wanted = [across[0], vertical, across[1]];
                let mut v = [0.0; 3];
                for axis in 0..3 {
                    v[axis] = velocity[axis] + (wanted[axis] - velocity[axis]) * t.hover_response;
                }
                if dash {
                    for axis in 0..3 {
                        v[axis] += look[axis] * t.dash_speed;
                    }
                }
                self.speed = length(v);
                if input.boost && input.forward > 0.0 {
                    self.mode = Mode::Cruise;
                    self.speed = self.speed.max(t.hover_speed);
                }
                step.velocity = Some(v);
                step.speed = self.speed;
            }
            Mode::Cruise | Mode::Sonic => {
                let was_sonic = self.mode == Mode::Sonic;
                if input.boost {
                    self.speed = (self.speed + t.accel).min(t.max_speed);
                } else {
                    self.speed -= t.decel;
                }
                if dash {
                    self.speed = (self.speed + t.dash_speed).min(t.max_speed);
                }
                if self.speed <= t.hover_speed {
                    // Slowed down: hover from here, carrying what is left.
                    self.mode = Mode::Hover;
                    self.speed = self.speed.max(0.0);
                } else {
                    self.mode = if self.speed >= t.sonic_speed() { Mode::Sonic } else { Mode::Cruise };
                }
                step.entered_sonic = self.mode == Mode::Sonic && !was_sonic;
                step.velocity = Some(look.map(|c| c * self.speed));
                step.speed = self.speed;
            }
        }
        step.mode = self.mode;
        step
    }
}

/// How hard the camera shakes, 0..=1: nothing below sonic, full at top speed.
pub fn shake(t: &Tuning, speed: f64) -> f64 {
    let from = t.sonic_speed();
    if speed <= from || t.max_speed <= from {
        return 0.0;
    }
    ((speed - from) / (t.max_speed - from)).clamp(0.0, 1.0)
}

/// The view's widening at this speed: 1 at rest, up to 1.3 at top speed.
pub fn fov_scale(t: &Tuning, speed: f64) -> f64 {
    1.0 + 0.3 * (speed / t.max_speed).clamp(0.0, 1.0)
}

/// The velocity the game leaves after a move through the air of `moved`: gravity, then air drag
/// (`LivingEntity.travel`). The flight pushes from here to the velocity it wants.
pub fn after_air_move(moved: [f64; 3]) -> [f64; 3] {
    const GRAVITY: f64 = 0.08;
    const DRAG_VERTICAL: f64 = 0.98;
    const DRAG_ACROSS: f64 = 0.91;
    [moved[0] * DRAG_ACROSS, (moved[1] - GRAVITY) * DRAG_VERTICAL, moved[2] * DRAG_ACROSS]
}

/// The push that takes the game's velocity after `moved` to `wanted`.
pub fn push_towards(moved: [f64; 3], wanted: [f64; 3]) -> [f64; 3] {
    let left = after_air_move(moved);
    [wanted[0] - left[0], wanted[1] - left[1], wanted[2] - left[2]]
}

/// Blocks per tick to metres per second.
pub fn metres_per_second(speed: f64) -> f64 {
    speed * 20.0
}

fn length(v: [f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    const STILL: [f64; 3] = [0.0, 0.0, 0.0];

    fn press_jump() -> Input {
        Input { jump_pressed: true, ..Input::default() }
    }

    fn take_off(f: &mut Flight, t: &Tuning) {
        f.step(t, &press_jump(), View::default(), STILL, true);
        let s = f.step(t, &press_jump(), View::default(), STILL, true);
        assert!(s.took_off);
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn looking_ahead_at_yaw_zero_is_south() {
        let look = View { yaw: 0.0, pitch: 0.0 }.look();
        assert!(close(look[0], 0.0) && close(look[1], 0.0) && close(look[2], 1.0));
        let west = View { yaw: 90.0, pitch: 0.0 }.look();
        assert!(close(west[0], -1.0) && close(west[2], 0.0));
        let up = View { yaw: 0.0, pitch: -90.0 }.look();
        assert!(close(up[1], 1.0));
    }

    #[test]
    fn a_double_tap_takes_off_and_a_single_one_does_not() {
        let t = Tuning::default();
        let mut f = Flight::default();
        let s = f.step(&t, &press_jump(), View::default(), STILL, true);
        assert!(!s.took_off && s.mode == Mode::Grounded);
        for _ in 0..t.double_tap_ticks + 1 {
            f.step(&t, &Input::default(), View::default(), STILL, true);
        }
        let late = f.step(&t, &press_jump(), View::default(), STILL, true);
        assert!(!late.took_off, "too slow a second tap starts a new double tap");
        let s = f.step(&t, &press_jump(), View::default(), STILL, true);
        assert!(s.took_off && s.mode == Mode::Hover);
    }

    #[test]
    fn hovering_cancels_the_fall() {
        let t = Tuning::default();
        let mut f = Flight::default();
        take_off(&mut f, &t);
        let mut v = [0.0, -1.5, 0.0];
        for _ in 0..40 {
            v = f.step(&t, &Input::default(), View::default(), v, false).velocity.unwrap();
        }
        assert!(v[1].abs() < 0.01, "still falling at {}", v[1]);
    }

    #[test]
    fn hover_moves_forward_where_the_player_faces() {
        let t = Tuning::default();
        let mut f = Flight::default();
        take_off(&mut f, &t);
        let ahead = Input { forward: 1.0, ..Input::default() };
        let mut v = STILL;
        for _ in 0..60 {
            v = f.step(&t, &ahead, View { yaw: 90.0, pitch: 0.0 }, v, false).velocity.unwrap();
        }
        assert!(close(v[0], -t.hover_speed) || (v[0] + t.hover_speed).abs() < 0.01, "{v:?}");
        assert!(v[2].abs() < 0.01);
    }

    #[test]
    fn sprinting_cruises_to_sonic_once_and_back_down_to_hover() {
        let t = Tuning::default();
        let mut f = Flight::default();
        take_off(&mut f, &t);
        let boost = Input { forward: 1.0, boost: true, ..Input::default() };
        let mut sonic_entries = 0;
        let mut last = None;
        for _ in 0..200 {
            let s = f.step(&t, &boost, View::default(), STILL, false);
            sonic_entries += usize::from(s.entered_sonic);
            last = Some(s);
        }
        let last = last.unwrap();
        assert_eq!(last.mode, Mode::Sonic);
        assert!(close(last.speed, t.max_speed));
        assert_eq!(sonic_entries, 1);
        let v = last.velocity.unwrap();
        assert!(close(v[2], t.max_speed), "flies along the look: {v:?}");

        let mut mode = Mode::Sonic;
        for _ in 0..100 {
            mode = f.step(&t, &Input::default(), View::default(), STILL, false).mode;
        }
        assert_eq!(mode, Mode::Hover);
    }

    #[test]
    fn a_double_tap_in_the_air_lands() {
        let t = Tuning::default();
        let mut f = Flight::default();
        take_off(&mut f, &t);
        f.step(&t, &press_jump(), View::default(), STILL, false);
        let s = f.step(&t, &press_jump(), View::default(), STILL, false);
        assert!(s.landed && s.mode == Mode::Grounded && s.velocity.is_none());
    }

    #[test]
    fn touching_the_ground_ends_the_flight() {
        let t = Tuning::default();
        let mut f = Flight::default();
        take_off(&mut f, &t);
        assert_eq!(f.step(&t, &Input::default(), View::default(), STILL, false).mode, Mode::Hover, "in the air it hovers on");
        let s = f.step(&t, &Input::default(), View::default(), STILL, true);
        assert!(s.landed && s.mode == Mode::Grounded && s.velocity.is_none(), "no sneak needed");

        take_off(&mut f, &t);
        let boost = Input { forward: 1.0, boost: true, ..Input::default() };
        f.step(&t, &boost, View { yaw: 0.0, pitch: 30.0 }, STILL, false);
        assert_eq!(f.mode(), Mode::Cruise);
        let s = f.step(&t, &boost, View { yaw: 0.0, pitch: 30.0 }, STILL, true);
        assert!(s.landed && s.mode == Mode::Grounded, "a slow dive into the ground lands");
    }

    #[test]
    fn taking_off_is_never_a_landing() {
        let t = Tuning::default();
        let mut f = Flight::default();
        f.step(&t, &press_jump(), View::default(), STILL, true);
        let s = f.step(&t, &press_jump(), View::default(), STILL, true);
        assert!(s.took_off && !s.landed && s.mode == Mode::Hover);
    }

    #[test]
    fn dashes_wait_for_the_cooldown() {
        let t = Tuning::default();
        let mut f = Flight::default();
        let dash = Input { dash_pressed: true, ..Input::default() };
        assert!(f.step(&t, &dash, View::default(), STILL, true).dashed);
        assert!(!f.step(&t, &dash, View::default(), STILL, true).dashed);
        for _ in 0..t.dash_cooldown {
            f.step(&t, &Input::default(), View::default(), STILL, true);
        }
        let s = f.step(&t, &dash, View::default(), STILL, true);
        assert!(s.dashed);
        let v = s.velocity.unwrap();
        assert!(close(v[2], t.dash_speed) && v[1] > 0.0, "{v:?}");
    }

    /// The whole loop as the game runs it: the flight's push lands on what gravity and drag left of
    /// the last move, and the player moves by the sum (open air, no keys of the game's own).
    fn fly_in_the_game(f: &mut Flight, t: &Tuning, input: &Input, view: View, mut moved: [f64; 3], ticks: usize) -> [f64; 3] {
        for _ in 0..ticks {
            let mut velocity = after_air_move(moved);
            if let (_, Some(kick)) = f.drive(t, input, view, moved, false) {
                for axis in 0..3 {
                    velocity[axis] += kick[axis];
                }
            }
            moved = velocity;
        }
        moved
    }

    #[test]
    fn a_hover_in_the_game_holds_its_height() {
        let t = Tuning::default();
        let mut f = Flight::default();
        take_off(&mut f, &t);
        // Taken off in the middle of a fall: the hover stops it and stays put.
        let moved = fly_in_the_game(&mut f, &t, &Input::default(), View::default(), [0.0, -1.0, 0.0], 100);
        assert!(moved[1].abs() < 0.001, "drifting at {} b/t", moved[1]);
        let up = Input { up: true, ..Input::default() };
        let moved = fly_in_the_game(&mut f, &t, &up, View::default(), moved, 60);
        assert!(close(moved[1], t.hover_vertical) || (moved[1] - t.hover_vertical).abs() < 0.001, "{moved:?}");
    }

    #[test]
    fn cruising_in_the_game_flies_level() {
        let t = Tuning::default();
        let mut f = Flight::default();
        take_off(&mut f, &t);
        let boost = Input { forward: 1.0, boost: true, ..Input::default() };
        let moved = fly_in_the_game(&mut f, &t, &boost, View::default(), [0.0; 3], 200);
        assert!(moved[1].abs() < 1e-9 && close(moved[2], t.max_speed), "{moved:?}");
    }

    #[test]
    fn an_impact_spends_the_speed_and_hovers() {
        let t = Tuning::default();
        let mut f = Flight::default();
        take_off(&mut f, &t);
        let boost = Input { forward: 1.0, boost: true, ..Input::default() };
        fly_in_the_game(&mut f, &t, &boost, View { yaw: 0.0, pitch: 60.0 }, [0.0; 3], 80);
        assert_eq!(f.mode(), Mode::Sonic);
        f.impact();
        assert_eq!(f.mode(), Mode::Hover);
        let s = f.step(&t, &Input::default(), View::default(), STILL, false);
        assert_eq!(s.mode, Mode::Hover);
        assert!(s.speed < 0.01);

        let mut grounded = Flight::default();
        grounded.impact();
        assert_eq!(grounded.mode(), Mode::Grounded, "walking into the ground is no impact");
    }

    #[test]
    fn shake_and_fov_follow_the_speed() {
        let t = Tuning::default();
        assert_eq!(shake(&t, 1.0), 0.0);
        assert!(close(shake(&t, t.max_speed), 1.0));
        assert!(close(fov_scale(&t, 0.0), 1.0));
        assert!(close(fov_scale(&t, t.max_speed), 1.3));
    }
}
