//! The respawn screen replayed against BDS 1.26.51: every death of
//! `testdata/bds-1.26.51-respawn.json` set up as `respawn.txt` had it, the
//! `Respawn` packets our server sends compared with BDS's.

use glam::{DVec3, IVec3};
use jolyne::valentine::types::{ActorRuntimeId, EnumsPlayerRespawnState, Vec3};
use jolyne::valentine::{McpePacketData, RespawnPacket};
use serde::Deserialize;

use crate::entity::PLAYER_EYE_HEIGHT;
use crate::entity::components::Position;
use crate::entity::player_life::authority::PlayerMotion;
use crate::entity::player_life::respawn::{PlayerSpawnPoint, standing_in};
use crate::server::game::capture_replay::{Replay, SESSION};

const FIXTURE: &str = include_str!("../testdata/bds-1.26.51-respawn.json");

#[derive(Debug, Deserialize)]
struct Fixture {
    deaths: Vec<Death>,
}

#[derive(Debug, Deserialize)]
struct Death {
    line: u32,
    feet: [f64; 3],
    point: Option<[i32; 3]>,
    blocked: bool,
    immediate: bool,
    respawns: Vec<Sent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
struct Sent {
    state: State,
    position: [f32; 3],
}

/// The `Respawn` states the server sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
enum State {
    Searchingforspawn,
    Readytospawn,
}

impl State {
    fn of(state: EnumsPlayerRespawnState) -> Option<Self> {
        match state {
            EnumsPlayerRespawnState::Searchingforspawn => Some(Self::Searchingforspawn),
            EnumsPlayerRespawnState::Readytospawn => Some(Self::Readytospawn),
            _ => None,
        }
    }
}

fn sent(packets: &[McpePacketData]) -> Vec<Sent> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            McpePacketData::RespawnPacket(respawn) => Some(Sent {
                state: State::of(respawn.state)?,
                position: [respawn.position.x, respawn.position.y, respawn.position.z],
            }),
            _ => None,
        })
        .collect()
}

#[test]
fn the_respawn_screen_answers_like_bds() {
    let fixture: Fixture = serde_json::from_str(FIXTURE).expect("the respawn fixture parses");
    let mut checked = 0;
    for death in &fixture.deaths {
        let mut h = Replay::new();
        let player = h.player;
        h.world()
            .resource_mut::<crate::world::rules::GameRules>()
            .do_immediate_respawn = death.immediate;
        let eye = DVec3::from_array(death.feet) + DVec3::Y * PLAYER_EYE_HEIGHT;
        h.world().get_mut::<Position>(player).unwrap().0 = eye;
        if let Some(mut motion) = h.world().get_mut::<PlayerMotion>(player) {
            motion.placed(eye);
        }
        if let Some(point) = death.point {
            let block = IVec3::from_array(point);
            h.world()
                .entity_mut(player)
                .insert(PlayerSpawnPoint::at(standing_in(block)));
            if death.blocked {
                h.set(block, "minecraft:stone");
                h.set(block + IVec3::Y, "minecraft:stone");
            }
        }
        h.drain();
        crate::entity::player_life::kill_player(h.world(), player);
        h.tick();
        let at_death = sent(&h.drain());
        assert_eq!(
            at_death,
            death.respawns[..1],
            "line {} at the death",
            death.line
        );
        h.route(
            RespawnPacket {
                position: Vec3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                state: EnumsPlayerRespawnState::Clientreadytospawn,
                player_runtime_id: ActorRuntimeId {
                    actor_runtime_id: SESSION,
                },
            }
            .into(),
        );
        h.tick();
        let answer = sent(&h.drain());
        assert_eq!(answer.len(), 2, "line {}", death.line);
        assert_eq!(
            answer[0], death.respawns[1],
            "line {} searching",
            death.line
        );
        // BDS spreads world spawns around the spawn (`spawnRadius`); only a
        // player's own point is compared.
        if death.point.is_some() {
            assert_eq!(answer[1], death.respawns[2], "line {} ready", death.line);
        } else {
            assert_eq!(answer[1].state, State::Readytospawn, "line {}", death.line);
        }
        assert!(
            h.world()
                .get::<crate::entity::player_life::Dead>(player)
                .is_none()
        );
        checked += 1;
    }
    assert_eq!(checked, 6);
}
