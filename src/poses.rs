//! The character's poses (design/poses: 1-imagem.png, 2-wireframe.png, spec.md), pure: which pose
//! is on, its angles, the 4-tick blend between poses, and the bytes that tell other players.
//!
//! Angles are degrees in the player model's own convention: an arm or leg's x below zero swings
//! it forward (-90 straight ahead, -180 overhead), the right arm's z above zero opens it outward
//! (the left arm's below zero), and `tilt` leans the whole body forward about its middle.

/// Client to server: a [`PoseMsg`] about the sender.
pub const POSE_CHANNEL: &str = "viltrumite:pose";
/// Server to clients: another player's name and their [`PoseMsg`].
pub const POSE_OF_CHANNEL: &str = "viltrumite:pose_of";

/// The flight's pose (A-C).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base {
    Hover = 1,
    Cruise = 2,
    Sonic = 3,
}

/// A move's pose (D, F, G, H), with how long it lasts and its priority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Dash = 1,
    Throw = 2,
    Clap = 3,
    Impact = 4,
}

impl Act {
    pub fn ticks(self) -> f32 {
        match self {
            Act::Dash => 10.0,
            Act::Throw | Act::Clap => 8.0,
            Act::Impact => 20.0,
        }
    }

    /// H > F = G > D.
    fn priority(self) -> u8 {
        match self {
            Act::Dash => 3,
            Act::Throw | Act::Clap => 4,
            Act::Impact => 5,
        }
    }

    fn from_byte(b: u8) -> Option<Act> {
        Some(match b {
            1 => Act::Dash,
            2 => Act::Throw,
            3 => Act::Clap,
            4 => Act::Impact,
            _ => return None,
        })
    }
}

fn base_from_byte(b: u8) -> Option<Base> {
    match b {
        1 => Some(Base::Hover),
        2 => Some(Base::Cruise),
        3 => Some(Base::Sonic),
        _ => None,
    }
}

/// One player's pose news: the flight's pose, whether they hold a mob, and a move just begun.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PoseMsg {
    pub base: Option<Base>,
    pub holding: bool,
    pub act: Option<Act>,
}

impl PoseMsg {
    pub fn encode(self) -> [u8; 3] {
        [self.base.map_or(0, |b| b as u8), u8::from(self.holding), self.act.map_or(0, |a| a as u8)]
    }

    pub fn decode(data: &[u8]) -> Option<PoseMsg> {
        let [base, holding, act] = data.try_into().ok()?;
        if base > 3 || holding > 1 || act > 4 {
            return None;
        }
        Some(PoseMsg { base: base_from_byte(base), holding: holding == 1, act: Act::from_byte(act) })
    }

    /// The relay to others: the sender's name, then the message.
    pub fn relay(self, name: &str) -> Vec<u8> {
        let name = &name.as_bytes()[..name.len().min(64)];
        let mut out = vec![name.len() as u8];
        out.extend_from_slice(name);
        out.extend_from_slice(&self.encode());
        out
    }

    pub fn unrelay(data: &[u8]) -> Option<(String, PoseMsg)> {
        let len = usize::from(*data.first()?);
        let name = std::str::from_utf8(data.get(1..1 + len)?).ok()?;
        Some((name.to_owned(), PoseMsg::decode(data.get(1 + len..)?)?))
    }
}

/// The parts' angles for one frame (spec: 1-7).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Angles {
    /// 7: the whole body leaning forward.
    pub tilt: f32,
    /// 1: added to the look's pitch (x); the look's yaw is the game's.
    pub head: f32,
    pub right_arm: [f32; 3],
    pub left_arm: [f32; 3],
    pub right_leg: f32,
    pub left_leg: f32,
    /// The hover's bob, in model pixels up.
    pub bob: f32,
}

pub const NEUTRAL: Angles = Angles {
    tilt: 0.0,
    head: 0.0,
    right_arm: [0.0; 3],
    left_arm: [0.0; 3],
    right_leg: 0.0,
    left_leg: 0.0,
    bob: 0.0,
};

/// What decides a player's pose this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Situation {
    pub base: Option<Base>,
    pub holding: bool,
    /// The move on, and ticks since it began.
    pub act: Option<(Act, f32)>,
    /// The look's pitch, degrees (down is positive).
    pub pitch: f32,
    /// A running clock in ticks, for the swaying.
    pub time: f32,
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

/// The pose this frame should reach, or None when no pose is on (the game draws the player).
pub fn target(s: &Situation) -> Option<Angles> {
    if s.base.is_none() && !s.holding && s.act.is_none() {
        return None;
    }
    let sway = (s.time * std::f32::consts::TAU / 60.0).sin();
    let lying = (90.0 + s.pitch).clamp(0.0, 160.0);
    let mut a = match s.base {
        None => NEUTRAL,
        Some(Base::Hover) => Angles {
            right_arm: [-10.0, 0.0, 8.0],
            left_arm: [10.0, 0.0, -8.0],
            right_leg: 6.0 + sway * 6.0,
            left_leg: -10.0 - sway * 6.0,
            bob: sway * 0.5,
            ..NEUTRAL
        },
        Some(Base::Cruise) => Angles {
            tilt: lying,
            head: -(lying - 5.0),
            right_arm: [-175.0, 0.0, 0.0],
            left_leg: 3.0 + sway * 2.0,
            right_leg: -sway * 2.0,
            ..NEUTRAL
        },
        Some(Base::Sonic) => Angles {
            tilt: lying + (s.time * 7.0).sin(),
            head: -(lying - 5.0),
            right_arm: [-178.0, 0.0, -6.0],
            left_arm: [-178.0, 0.0, 6.0],
            ..NEUTRAL
        },
    };
    if s.holding {
        // One hand up, toward the held mob (it floats along the look): the arm's angle from
        // hanging down is the look's, turned back by the body's lean.
        a.right_arm = [(s.pitch - a.tilt - 90.0).clamp(-180.0, 0.0), 0.0, 0.0];
    }
    if let Some((act, elapsed)) = s.act {
        let p = (elapsed / act.ticks()).clamp(0.0, 1.0);
        match act {
            Act::Dash => {
                a = Angles {
                    tilt: 30.0,
                    head: -20.0,
                    right_arm: [55.0, 0.0, 0.0],
                    left_arm: [60.0, 0.0, 0.0],
                    right_leg: -30.0,
                    left_leg: 20.0,
                    ..NEUTRAL
                };
            }
            Act::Throw => {
                a.tilt += 12.0;
                a.right_arm = [if p < 0.4 { lerp(0.0, -150.0, p / 0.4) } else { lerp(-150.0, -40.0, (p - 0.4) / 0.6) }, 0.0, 0.0];
                a.left_arm = [-40.0, 0.0, 0.0];
                a.right_leg = -18.0;
                a.left_leg = 16.0;
            }
            Act::Clap => {
                let open = if p < 3.0 / 8.0 { lerp(0.0, 70.0, p / (3.0 / 8.0)) } else { lerp(70.0, 0.0, (p - 3.0 / 8.0) / (5.0 / 8.0)) };
                a.right_arm = [-90.0, 0.0, open];
                a.left_arm = [-90.0, 0.0, -open];
            }
            Act::Impact => {
                a = Angles {
                    tilt: 30.0,
                    head: -25.0,
                    right_arm: [-38.0, 0.0, 0.0],
                    left_arm: [40.0, 0.0, 0.0],
                    right_leg: -80.0,
                    left_leg: 55.0,
                    ..NEUTRAL
                };
            }
        }
    }
    Some(a)
}

/// Moves `current` toward `target` for `dt` ticks: about 94% of the way in 4 ticks (spec: 10).
pub fn approach(current: Angles, target: Angles, dt: f32) -> Angles {
    let k = 1.0 - (-dt.max(0.0) / 1.4).exp();
    let l = |a: f32, b: f32| a + (b - a) * k;
    let l3 = |a: [f32; 3], b: [f32; 3]| [l(a[0], b[0]), l(a[1], b[1]), l(a[2], b[2])];
    Angles {
        tilt: l(current.tilt, target.tilt),
        head: l(current.head, target.head),
        right_arm: l3(current.right_arm, target.right_arm),
        left_arm: l3(current.left_arm, target.left_arm),
        right_leg: l(current.right_leg, target.right_leg),
        left_leg: l(current.left_leg, target.left_leg),
        bob: l(current.bob, target.bob),
    }
}

fn settled(a: &Angles) -> bool {
    let all = [a.tilt, a.head, a.right_leg, a.left_leg]
        .into_iter()
        .chain(a.right_arm)
        .chain(a.left_arm);
    all.into_iter().all(|v| v.abs() < 1.5) && a.bob.abs() < 0.1
}

/// One player's moves: the one on, kept unless a stronger one comes (H > F = G > D).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ActTrack {
    act: Option<(Act, f32)>,
}

impl ActTrack {
    pub fn start(&mut self, act: Act, now: f32) {
        let busy = self.current(now).is_some_and(|(on, _)| on.priority() > act.priority());
        if !busy {
            self.act = Some((act, now));
        }
    }

    /// The move on at `now`, with ticks since it began.
    pub fn current(&self, now: f32) -> Option<(Act, f32)> {
        let (act, started) = self.act?;
        let elapsed = now - started;
        (elapsed >= 0.0 && elapsed <= act.ticks()).then_some((act, elapsed))
    }
}

/// One player's drawn pose between frames: blends into each new pose, and back to the game's own
/// drawing once nothing is on and the limbs are home.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PoseTrack {
    current: Option<Angles>,
    last_time: Option<f32>,
}

impl PoseTrack {
    /// The angles to draw at `time` (ticks), or None to let the game draw.
    pub fn frame(&mut self, target: Option<Angles>, time: f32) -> Option<Angles> {
        let dt = self.last_time.map_or(0.0, |last| (time - last).clamp(0.0, 5.0));
        self.last_time = Some(time);
        let next = match (target, self.current) {
            (Some(t), Some(c)) => approach(c, t, dt),
            (Some(t), None) => approach(NEUTRAL, t, dt.max(0.5)),
            (None, Some(c)) => {
                let back = approach(c, NEUTRAL, dt);
                if settled(&back) {
                    self.current = None;
                    return None;
                }
                back
            }
            (None, None) => return None,
        };
        self.current = Some(next);
        Some(next)
    }
}

/// Whether a 64x64 skin is a slim one: its right arm's last two columns (x 54-55, y 20-31) are
/// see-through, as the 3-pixel arms leave them.
pub fn slim_skin(width: u32, height: u32, rgba: &[u8]) -> bool {
    if width != 64 || height != 64 || rgba.len() < (width * height * 4) as usize {
        return false;
    }
    (20..32).all(|y| (54..56).all(|x| rgba[((y * width + x) * 4 + 3) as usize] == 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(base: Option<Base>) -> Situation {
        Situation { base, ..Situation::default() }
    }

    #[test]
    fn nothing_on_lets_the_game_draw() {
        assert_eq!(target(&s(None)), None);
    }

    #[test]
    fn cruising_lies_along_the_look_with_a_fist_ahead() {
        let level = target(&s(Some(Base::Cruise))).unwrap();
        assert_eq!(level.tilt, 90.0);
        assert_eq!(level.head, -85.0);
        assert_eq!(level.right_arm[0], -175.0);
        let diving = target(&Situation { pitch: 40.0, ..s(Some(Base::Cruise)) }).unwrap();
        assert_eq!(diving.tilt, 130.0, "a dive tips further");
        let steep = target(&Situation { pitch: 90.0, ..s(Some(Base::Cruise)) }).unwrap();
        assert_eq!(steep.tilt, 160.0, "capped");
    }

    #[test]
    fn sonic_puts_both_fists_forward_and_hover_lets_the_legs_hang() {
        let sonic = target(&s(Some(Base::Sonic))).unwrap();
        assert_eq!(sonic.right_arm[0], -178.0);
        assert_eq!(sonic.left_arm[0], -178.0);
        let hover = target(&s(Some(Base::Hover))).unwrap();
        assert_eq!(hover.tilt, 0.0);
        assert!(hover.right_arm[2] > 0.0 && hover.left_arm[2] < 0.0, "arms a little open");
    }

    #[test]
    fn holding_raises_one_hand_toward_the_mob() {
        let standing = target(&Situation { holding: true, ..s(None) }).unwrap();
        assert_eq!(standing.right_arm, [-90.0, 0.0, 0.0], "straight ahead");
        assert_eq!(standing.left_arm, [0.0; 3], "the other hand stays down");
        let looking_up = target(&Situation { holding: true, pitch: -30.0, ..s(None) }).unwrap();
        assert_eq!(looking_up.right_arm[0], -120.0);
        let cruising = target(&Situation { holding: true, ..s(Some(Base::Cruise)) }).unwrap();
        assert_eq!(cruising.tilt, 90.0, "still lying in the flight");
        assert_eq!(cruising.right_arm[0], -180.0, "ahead of the head, along the flight");
        let hovering = target(&Situation { holding: true, ..s(Some(Base::Hover)) }).unwrap();
        assert_eq!(hovering.left_arm, [10.0, 0.0, -8.0], "the hover's own left arm");
    }

    #[test]
    fn the_clap_opens_then_shuts() {
        let at = |e: f32| target(&Situation { act: Some((Act::Clap, e)), ..s(None) }).unwrap();
        assert_eq!(at(3.0).right_arm[2], 70.0);
        assert_eq!(at(3.0).left_arm[2], -70.0);
        assert_eq!(at(8.0).right_arm[2], 0.0);
        assert_eq!(at(8.0).right_arm[0], -90.0);
    }

    #[test]
    fn the_throw_goes_over_the_head_and_down() {
        let at = |e: f32| target(&Situation { act: Some((Act::Throw, e)), ..s(None) }).unwrap();
        assert_eq!(at(3.2).right_arm[0], -150.0);
        assert_eq!(at(8.0).right_arm[0], -40.0);
    }

    #[test]
    fn the_impact_and_the_dash_take_the_whole_body() {
        let impact = target(&Situation { act: Some((Act::Impact, 5.0)), holding: true, ..s(Some(Base::Sonic)) }).unwrap();
        assert_eq!(impact.tilt, 30.0);
        assert_eq!(impact.right_arm[0], -38.0, "the fist in the ground");
        assert_eq!(impact.right_leg, -80.0);
        let dash = target(&Situation { act: Some((Act::Dash, 2.0)), ..s(Some(Base::Hover)) }).unwrap();
        assert_eq!(dash.right_arm[0], 55.0, "arms thrown back");
    }

    #[test]
    fn a_stronger_move_wins_and_a_weaker_one_waits() {
        let mut t = ActTrack::default();
        t.start(Act::Impact, 0.0);
        t.start(Act::Dash, 5.0);
        assert_eq!(t.current(6.0).map(|c| c.0), Some(Act::Impact));
        t.start(Act::Dash, 25.0);
        assert_eq!(t.current(26.0).map(|c| c.0), Some(Act::Dash), "the impact is over");
        t.start(Act::Clap, 27.0);
        assert_eq!(t.current(28.0).map(|c| c.0), Some(Act::Clap), "stronger than a dash");
        assert_eq!(t.current(40.0), None);
    }

    #[test]
    fn poses_blend_in_four_ticks_and_hand_back_to_the_game() {
        let mut track = PoseTrack::default();
        let goal = target(&s(Some(Base::Cruise))).unwrap();
        let mut drawn = track.frame(Some(goal), 0.0).unwrap();
        for i in 1..=4 {
            drawn = track.frame(Some(goal), i as f32).unwrap();
        }
        assert!((drawn.tilt - goal.tilt).abs() < goal.tilt * 0.1, "{} of {}", drawn.tilt, goal.tilt);
        let mut released = false;
        for i in 5..60 {
            if track.frame(None, i as f32).is_none() {
                released = true;
                break;
            }
        }
        assert!(released, "back to the game once the limbs are home");
        assert_eq!(track.frame(None, 61.0), None);
    }

    #[test]
    fn pose_news_survive_the_wire_and_the_relay() {
        let msg = PoseMsg { base: Some(Base::Sonic), holding: true, act: Some(Act::Clap) };
        assert_eq!(PoseMsg::decode(&msg.encode()), Some(msg));
        assert_eq!(PoseMsg::decode(&[9, 0, 0]), None);
        assert_eq!(PoseMsg::decode(&[1, 0]), None);
        assert_eq!(PoseMsg::unrelay(&msg.relay("Nithiro")), Some(("Nithiro".to_owned(), msg)));
        assert_eq!(PoseMsg::unrelay(&[5, b'a']), None);
    }

    #[test]
    fn slim_skins_are_told_by_their_empty_arm_columns() {
        let mut wide = vec![255u8; 64 * 64 * 4];
        assert!(!slim_skin(64, 64, &wide));
        for y in 20..32 {
            for x in 54..56 {
                wide[(y * 64 + x) * 4 + 3] = 0;
            }
        }
        assert!(slim_skin(64, 64, &wide));
        assert!(!slim_skin(64, 32, &wide[..64 * 32 * 4]), "an old 64x32 skin has no such arm");
    }
}
