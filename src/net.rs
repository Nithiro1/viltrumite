//! The mod's messages between client and server: raw play payloads, each a few bytes in the
//! formats `moves.rs` and `poses.rs` define (and test). Handlers reach the mod's state through the
//! statics in `lib.rs`.
use crate::{CLIENT, SERVER};
use mod_api::{
    ClientHost, IPayloadContext, IPayloadHandler, PayloadBytes, PayloadHandleError, PayloadId, PlayerRef,
    Registrar, ServerHost,
};
use std::sync::Arc;

/// Server to client: one byte, 1 with powers, 0 without.
pub const POWER: &str = "viltrumite:power";
/// Server to client: one byte, 1 while holding a mob.
pub const HELD: &str = "viltrumite:held";
/// Client to server: a move (`moves::Move`).
pub const MOVE: &str = "viltrumite:move";
/// Client to server: this player's pose news (`poses::PoseMsg`).
pub const POSE: &str = "viltrumite:pose";
/// Server to clients: another player's name and pose news.
pub const POSE_OF: &str = "viltrumite:pose_of";
/// Server to clients: a player's skin texture (`skins::SkinMsg`).
pub const SKIN: &str = "viltrumite:skin";
/// Server to client: one byte, 1: this player hasn't chosen yet in this world, show the dialog.
pub const ASK: &str = "viltrumite:ask";
/// Client to server: the dialog's answer, one byte, 1 to be a Viltrumite, 0 to stay human.
pub const CHOOSE: &str = "viltrumite:choose";

fn id(name: &str) -> PayloadId {
    PayloadId(name.to_owned())
}

pub fn register(r: &mut dyn Registrar) {
    let mut payloads = r.payload_handlers();
    let mut view = payloads.registrar("1");
    view.play_to_client(id(POWER), Some(Arc::new(ToClient(POWER))));
    view.play_to_client(id(HELD), Some(Arc::new(ToClient(HELD))));
    view.play_to_client(id(POSE_OF), Some(Arc::new(ToClient(POSE_OF))));
    view.play_to_client(id(SKIN), Some(Arc::new(ToClient(SKIN))));
    view.play_to_client(id(ASK), Some(Arc::new(ToClient(ASK))));
    view.play_to_server(id(CHOOSE), Arc::new(ToServer(CHOOSE)));
    view.play_to_server(id(MOVE), Arc::new(ToServer(MOVE)));
    view.play_to_server(id(POSE), Arc::new(ToServer(POSE)));
}

/// Sends `bytes` on channel `name` to `player`'s client.
pub fn to_player(host: &mut dyn ServerHost, player: PlayerRef, name: &str, bytes: Vec<u8>) {
    if let Err(error) = player.send_payload(host, &id(name), PayloadBytes(bytes)) {
        log::debug!("Viltrumite: {name} didn't go to a player: {error:?}");
    }
}

/// Sends `bytes` on channel `name` to the server.
pub fn to_server(host: &mut dyn ClientHost, name: &str, bytes: &[u8]) {
    if let Err(error) = host.send_payload_to_server(&id(name), PayloadBytes(bytes.to_vec())) {
        log::debug!("Viltrumite: {name} didn't go to the server: {error:?}");
    }
}

/// What a client sent, handed to the server's state.
struct ToServer(&'static str);

impl IPayloadHandler for ToServer {
    fn handle(&self, payload: PayloadBytes, context: &mut dyn IPayloadContext) -> Result<(), PayloadHandleError> {
        let Ok(player) = context.player() else {
            return Ok(());
        };
        let Ok(host) = context.server_host() else {
            return Ok(());
        };
        match self.0 {
            MOVE => SERVER.moved(host, player, &payload.0),
            POSE => SERVER.relay_pose(host, player, &payload.0),
            CHOOSE => SERVER.chose(host, player, &payload.0),
            _ => {}
        }
        Ok(())
    }
}

/// What the server sent, handed to the client's state.
struct ToClient(&'static str);

impl IPayloadHandler for ToClient {
    fn handle(&self, payload: PayloadBytes, _context: &mut dyn IPayloadContext) -> Result<(), PayloadHandleError> {
        CLIENT.message(self.0, &payload.0);
        Ok(())
    }
}
