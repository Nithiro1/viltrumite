//! Draws a player in a pose (design/poses): the vanilla player model, its parts turned to the
//! pose's angles, with the player's own skin. Only while a pose is on (or blending out of one);
//! otherwise the game draws the player, armour, held items and cape included.
//!
//! The skin: the one the server named for the player (`skins.rs`), else, for this window's own
//! player, the launcher's skin of an offline name, else the default; each tried in turn, since a
//! skin still downloading isn't a texture yet.
use crate::poses::{ActTrack, Angles, Base, PoseMsg, PoseTrack, Situation, target};
use crate::skins::{OFFLINE_SKIN, SkinMsg};
use mod_api::{BlockEntityDrawContext, ClientHost, Flow, ModelPart, RenderPlayerPreEvent};
use std::collections::HashMap;

const WIDE: &str = "minecraft:player#main";
const SLIM: &str = "minecraft:player_slim#main";
const WIDE_DEFAULT: &str = "entity/player/wide/steve";
const SLIM_DEFAULT: &str = "entity/player/slim/alex";
/// Players are drawn at 15/16 (`PlayerRenderer.scale`).
const PLAYER_SCALE: f32 = 0.9375;
/// Where the body turns when it lies down: its middle, in blocks above the feet.
const BODY_MIDDLE: f32 = 0.9;
/// How long to wait before baking the models again after a failure, in ticks.
const BAKE_AGAIN: f32 = 200.0;

/// What another player's client told about their pose.
#[derive(Clone, Copy, Debug, Default)]
struct Remote {
    base: Option<Base>,
    holding: bool,
    act: ActTrack,
}

/// The poses of every player in view, kept by the window's client.
#[derive(Default)]
pub struct Poser {
    /// Window ticks, the clock of the poses.
    pub clock: f32,
    /// This player's own moves.
    pub own_act: ActTrack,
    remote: HashMap<String, Remote>,
    tracks: HashMap<String, PoseTrack>,
    /// Each player's skin by name, as the server told it: texture and slim.
    skins: HashMap<String, (String, bool)>,
    /// The wide and slim models, baked once; a failed bake is tried again later.
    models: Option<(ModelPart, Option<ModelPart>)>,
    bake_tried: Option<f32>,
    /// What was already said in the log, so it is said once.
    logged_skin: HashMap<String, String>,
    logged_failure: bool,
}

impl Poser {
    /// Another player's news, from the server's relay.
    pub fn heard(&mut self, name: String, msg: PoseMsg) {
        let now = self.clock;
        let remote = self.remote.entry(name).or_default();
        remote.base = msg.base;
        remote.holding = msg.holding;
        if let Some(act) = msg.act {
            remote.act.start(act, now);
        }
    }

    /// A player's skin, from the server.
    pub fn skin(&mut self, msg: SkinMsg) {
        self.skins.insert(msg.name, (msg.texture, msg.slim));
    }

    /// Back to nothing (another world).
    pub fn forget(&mut self) {
        self.remote.clear();
        self.tracks.clear();
        self.skins.clear();
    }

    /// The models, baked the first time and again a while after a failure.
    fn models(&mut self, host: &dyn ClientHost) -> Option<&(ModelPart, Option<ModelPart>)> {
        if self.models.is_none() && self.bake_tried.is_none_or(|t| self.clock - t >= BAKE_AGAIN) {
            self.bake_tried = Some(self.clock);
            match host.bake_layer(WIDE) {
                Some(wide) => {
                    let slim = host.bake_layer(SLIM);
                    log::info!("Viltrumite: player models baked (slim: {})", slim.is_some());
                    self.models = Some((wide, slim));
                }
                None => log::warn!("Viltrumite: no player model `{WIDE}`: poses are off for now"),
            }
        }
        self.models.as_ref()
    }

    /// `render_player_pre`: `own` is this player's flight pose and hold, used when the player drawn
    /// is the window's own. Cancel when it drew the player itself.
    pub fn render(&mut self, host: &dyn ClientHost, event: &mut RenderPlayerPreEvent<'_>, own: (Option<Base>, bool)) -> Flow {
        let mob = event.player;
        let who = event.who;
        let time = self.clock + mob.partial;
        let (base, holding, act) = if who.local {
            (own.0, own.1, self.own_act.current(time))
        } else {
            match self.remote.get(who.name) {
                Some(r) => (r.base, r.holding, r.act.current(time)),
                None => (None, false, None),
            }
        };
        let situation = Situation { base, holding, act, pitch: mob.pitch, time };
        let track = self.tracks.entry(who.name.to_owned()).or_default();
        let Some(angles) = track.frame(target(&situation), time) else {
            return Flow::Continue;
        };

        let told = self.skins.get(who.name).cloned();
        let slim = told.as_ref().is_some_and(|t| t.1);
        let mut candidates: Vec<&str> = Vec::with_capacity(3);
        if let Some((texture, _)) = &told {
            candidates.push(texture);
        }
        if who.local {
            candidates.push(OFFLINE_SKIN);
        }
        candidates.push(if slim { SLIM_DEFAULT } else { WIDE_DEFAULT });

        let Some((wide, slim_model)) = self.models(host) else {
            return Flow::Continue;
        };
        let model = match (slim, slim_model) {
            (true, Some(slim_model)) => slim_model,
            _ => wide,
        };
        let mut posed = model.clone();
        pose_parts(&mut posed, &angles, mob.pitch, mob.head_yaw);
        match draw_posed(event.draw, &posed, &angles, &candidates) {
            Some(skin) => {
                if self.logged_skin.get(who.name).map(String::as_str) != Some(skin) {
                    log::info!("Viltrumite: drawing {} in poses (skin `{skin}`, slim {slim})", who.name);
                    self.logged_skin.insert(who.name.to_owned(), skin.to_owned());
                }
                Flow::Cancel
            }
            None => {
                if !std::mem::replace(&mut self.logged_failure, true) {
                    log::warn!("Viltrumite: none of the skins {candidates:?} is a texture: poses are off");
                }
                Flow::Continue
            }
        }
    }
}

/// Turns the model's parts to `angles` (radians in the model, degrees here), with the head on the
/// player's look. Skin layers that are separate parts (not children) follow their part.
pub fn pose_parts(model: &mut ModelPart, angles: &Angles, pitch: f32, head_yaw: f32) {
    model.reset_all_poses();
    let head = [pitch + angles.head, head_yaw, 0.0];
    let parts: [(&str, &str, [f32; 3]); 5] = [
        ("head", "hat", head),
        ("right_arm", "right_sleeve", angles.right_arm),
        ("left_arm", "left_sleeve", angles.left_arm),
        ("right_leg", "right_pants", [angles.right_leg, 0.0, 0.0]),
        ("left_leg", "left_pants", [angles.left_leg, 0.0, 0.0]),
    ];
    for (part, layer, [x, y, z]) in parts {
        let turn = |p: &mut ModelPart| {
            p.x_rot = x.to_radians();
            p.y_rot = y.to_radians();
            p.z_rot = z.to_radians();
        };
        if let Some(p) = find_mut(model, part) {
            turn(p);
        }
        // A layer at the root (older layouts) turns with its part; one inside it already does.
        if let Some(p) = model.get_child_mut(layer) {
            turn(p);
        }
    }
}

/// The part named `name`, wherever it is in the tree.
fn find_mut<'a>(part: &'a mut ModelPart, name: &str) -> Option<&'a mut ModelPart> {
    if part.get_child(name).is_some() {
        return part.get_child_mut(name);
    }
    for (_, child) in part.children_mut() {
        if let Some(found) = find_mut(child, name) {
            return Some(found);
        }
    }
    None
}

/// Draws the posed model in the player's frame (at the feet, turned with the body): the bob, the
/// body leaning about its middle, then the model's own frame as the game's renderer sets it up.
/// The first of `skins` that is a texture wins; None when none is.
fn draw_posed<'s>(draw: &mut dyn BlockEntityDrawContext, model: &ModelPart, angles: &Angles, skins: &[&'s str]) -> Option<&'s str> {
    draw.push();
    draw.translate([0.0, angles.bob / 16.0, 0.0]);
    draw.rotate_about([1.0, 0.0, 0.0], angles.tilt, [0.0, BODY_MIDDLE, 0.0]);
    draw.scale([1.0, -1.0, -1.0]);
    draw.scale([PLAYER_SCALE; 3]);
    draw.translate([0.0, -1.501, 0.0]);
    let drawn = skins.iter().copied().find(|skin| draw.model_part(model, skin, 0xFFFFFF));
    draw.pop();
    drawn
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::poses::NEUTRAL;
    use mod_api::PartPose;

    fn part(children: Vec<(&str, ModelPart)>) -> ModelPart {
        let children = children.into_iter().map(|(n, c)| (n.to_owned(), c)).collect();
        ModelPart::new(Vec::new(), children, PartPose::default())
    }

    #[test]
    fn parts_turn_to_the_pose_and_root_layers_follow() {
        // Arms with their sleeves inside, a hat at the root (both layouts).
        let mut model = part(vec![
            ("head", part(vec![])),
            ("hat", part(vec![])),
            ("body", part(vec![])),
            ("right_arm", part(vec![("right_sleeve", part(vec![]))])),
            ("left_arm", part(vec![])),
            ("right_leg", part(vec![])),
            ("left_leg", part(vec![])),
        ]);
        let angles = Angles { head: -85.0, right_arm: [-175.0, 0.0, 6.0], right_leg: 3.0, ..NEUTRAL };
        pose_parts(&mut model, &angles, 10.0, 20.0);
        let head = model.get_child("head").unwrap();
        assert!((head.x_rot - (-75.0f32).to_radians()).abs() < 1e-6);
        assert!((head.y_rot - 20.0f32.to_radians()).abs() < 1e-6);
        assert_eq!(model.get_child("hat").unwrap().x_rot, head.x_rot, "a root hat follows the head");
        let arm = model.get_child("right_arm").unwrap();
        assert!((arm.x_rot - (-175.0f32).to_radians()).abs() < 1e-6);
        assert!((arm.z_rot - 6.0f32.to_radians()).abs() < 1e-6);
        assert_eq!(arm.get_child("right_sleeve").unwrap().x_rot, 0.0, "a sleeve inside the arm turns with it, not twice");
    }

    #[test]
    fn a_nested_part_is_found() {
        let mut model = part(vec![("body", part(vec![("left_arm", part(vec![]))]))]);
        assert!(find_mut(&mut model, "left_arm").is_some());
        assert!(find_mut(&mut model, "tail").is_none());
    }
}
