//! Sleeping replayed against BDS 1.26.51: every bed click of the committed
//! fixture (`testdata/bds-1.26.51-sleeping.json`) set up as the script had
//! it and answered by our server, then the stand-up spots, the nights and
//! the actor data.

use bevy_ecs::prelude::*;
use glam::{DVec3, IVec3};
use jolyne::valentine::types::{
    BlockPos, DataItemEntry, DataItemEntryPayload, EnumsHandSlot,
    EnumsItemUseInventoryTransactionActionType, EnumsItemUseInventoryTransactionTriggerType,
    EnumsPlayerActionType, EnumsSpawnPositionType, InventoryTransactionPacketTransaction,
    ItemUseInventoryTransaction, TextPacketBody, Vec3,
};
use jolyne::valentine::{
    InventoryTransactionPacket, McpePacket, McpePacketData, PlayerActionPacket,
};
use serde::Deserialize;
use tokio::sync::mpsc::Receiver;

use super::*;
use crate::entity::components::GameMode;
use crate::entity::mob::{SpawnCause, spawn_mob};
use crate::registry::BlockRegistry;
use crate::server::game::PlayerSpawnData;
use crate::server::game::capture_replay::Replay;
use crate::world::rules::{Difficulty, WorldTime};
use crate::world::weather::WeatherLevels;
use unastar_data::game_rules::GameRuleValue;

const FIXTURE: &str = include_str!("../testdata/bds-1.26.51-sleeping.json");

#[derive(Debug, Deserialize)]
struct Fixture {
    bed: BedFixture,
    clicks: Vec<Click>,
    wakes: Vec<Wake>,
    nights: Vec<Night>,
    actor_data: Vec<ActorRows>,
}

#[derive(Debug, Deserialize)]
struct BedFixture {
    foot: [i32; 3],
    head: [i32; 3],
    direction: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
struct BedAt {
    head: [i32; 3],
    direction: i32,
}

#[derive(Debug, Clone, Deserialize)]
struct Click {
    script: String,
    bed: BedAt,
    line: u32,
    clock: i64,
    daylight_cycle: bool,
    rain: f32,
    lightning: f32,
    players: usize,
    percentage: i32,
    feet: [f64; 3],
    clicked: [i32; 3],
    zombie: Option<[f64; 3]>,
    walls: Vec<[i32; 3]>,
    block_on_head: Option<FixtureBlock>,
    mode: GameMode,
    occupied_by_other: bool,
    answer: Answer,
}

/// A block the fixture names, resolved to its default state when the
/// fixture loads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
struct FixtureBlock(u32);

impl TryFrom<String> for FixtureBlock {
    type Error = String;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        BlockRegistry::vanilla()
            .default_state_id_by_name(&name)
            .map(Self)
            .ok_or_else(|| format!("unknown block {name}"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct Answer {
    spawn: Option<Spawn>,
    messages: Vec<String>,
    event: Option<[i32; 2]>,
    slept: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
struct Spawn {
    block: [i32; 3],
    bed: [i32; 3],
}

/// What woke a sleeper in the capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WakeKind {
    /// `PlayerAction` StopSleeping.
    Left,
    /// The night was over.
    Morning,
    /// A command (`/time set`, `/damage`).
    Command,
}

#[derive(Debug, Deserialize)]
struct Wake {
    script: String,
    line: u32,
    cause: WakeKind,
    command: Option<String>,
    stood_eye: Option<[f32; 3]>,
    event: Option<[i32; 2]>,
}

#[derive(Debug, Deserialize)]
struct Night {
    script: String,
    line: u32,
    slept_ticks: u64,
    clock_after: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct ActorRows {
    script: String,
    line: u32,
    event: String,
    rows: Vec<serde_json::Value>,
}

fn fixture() -> Fixture {
    serde_json::from_str(FIXTURE).expect("the sleeping fixture parses")
}

fn ivec(at: [i32; 3]) -> IVec3 {
    IVec3::from_array(at)
}

/// The head of the scripts' bed A, which every [`Bedroom`] has.
fn fixture_head() -> [i32; 3] {
    [2, -60, 0]
}

/// The scripted world: the BDS arena with the bed, the clock stopped at
/// the clicks' times, no natural spawning.
struct Bedroom {
    replay: Replay,
    /// The awake second player of the two-player scripts.
    guest: Option<(Entity, Receiver<McpePacket>)>,
}

impl Bedroom {
    fn new(fixture: &Fixture) -> Self {
        let mut replay = Replay::with_difficulty(Difficulty::Easy);
        replay.world().resource_mut::<GameRules>().do_mob_spawning = false;
        for x in -12..=24 {
            for z in -12..=14 {
                replay.set(IVec3::new(x, -61, z), "minecraft:stone");
            }
        }
        let mut room = Self {
            replay,
            guest: None,
        };
        room.place_bed(ivec(fixture.bed.head), fixture.bed.direction, false);
        room.replay.tick();
        room.replay.drain();
        room
    }

    fn world(&mut self) -> &mut World {
        self.replay.world()
    }

    /// A bed with its head at `head` lying along `direction`.
    fn place_bed(&mut self, head: IVec3, direction: i32, occupied: bool) {
        let registry = BlockRegistry::vanilla();
        let bed = registry.default_state_id_by_name("minecraft:bed").unwrap();
        let entry = registry.get_by_runtime_id(bed).unwrap();
        let with = |state: u32, name: &str, value: StateValue| {
            entry.with_state_value(state, name, value).unwrap()
        };
        let base = with(bed, DIRECTION, StateValue::Int(direction));
        let foot_state = with(base, HEAD_PIECE, StateValue::Byte(0));
        let head_state = with(
            with(base, HEAD_PIECE, StateValue::Byte(1)),
            OCCUPIED,
            StateValue::Byte(u8::from(occupied)),
        );
        let foot = head - Horizontal::from_legacy(direction).step();
        for (pos, state) in [(foot, foot_state), (head, head_state)] {
            crate::server::game::blocks::write_block(self.world(), pos, state, false);
        }
    }

    fn join_guest(&mut self) {
        if self.guest.is_some() {
            return;
        }
        let (outbound_tx, rx) = tokio::sync::mpsc::channel(1 << 16);
        let guest = self
            .replay
            .server
            .spawn_player(PlayerSpawnData {
                session_id: 8,
                display_name: "BDSObserver".into(),
                xuid: None,
                uuid: Some("00000000-0000-0000-0000-000000000008".into()),
                runtime_id: 8,
                position: DVec3::new(0.5, -60.0 + PLAYER_EYE_HEIGHT, 6.5),
                outbound_tx,
                saved_data: None,
                chunk_radius: 2,
            })
            .expect("valid guest spawn");
        *self.world().get_mut::<GameMode>(guest).unwrap() = GameMode::Survival;
        self.guest = Some((guest, rx));
    }

    fn put(&mut self, feet: DVec3) {
        let player = self.replay.player;
        let eye = feet + DVec3::Y * PLAYER_EYE_HEIGHT;
        self.world().get_mut::<Position>(player).unwrap().0 = eye;
        if let Some(mut motion) = self.world().get_mut::<PlayerMotion>(player) {
            motion.placed(eye);
        }
    }

    fn route(&mut self, packet: impl Into<McpePacket>) {
        self.replay.route(packet.into());
    }

    fn click(&mut self, pos: IVec3) {
        let player = self.replay.player;
        let feet = feet_of(self.world(), player).unwrap();
        let wire = feet + DVec3::Y * PLAYER_REPORTED_EYE_HEIGHT;
        let network = crate::server::game::types::block_network_ids(self.world());
        let block = network.to_wire(self.world().block_at(pos).unwrap());
        self.route(InventoryTransactionPacket {
            transaction: InventoryTransactionPacketTransaction::ItemUseInventoryTransaction(
                Box::new(ItemUseInventoryTransaction {
                    action_type: EnumsItemUseInventoryTransactionActionType::Place,
                    trigger_type: EnumsItemUseInventoryTransactionTriggerType::Playerinput,
                    position: BlockPos {
                        x: pos.x,
                        y: pos.y,
                        z: pos.z,
                    },
                    face: 1,
                    slot: 0,
                    hand: EnumsHandSlot::Mainhand,
                    from_position: Vec3 {
                        x: wire.x as f32,
                        y: wire.y as f32,
                        z: wire.z as f32,
                    },
                    click_position: Vec3 {
                        x: 0.5,
                        y: 1.0,
                        z: 0.5,
                    },
                    target_block_id: block,
                    ..Default::default()
                }),
            ),
            ..Default::default()
        });
    }

    fn leave_bed(&mut self) {
        self.route(PlayerActionPacket {
            player_runtime_id: jolyne::valentine::types::ActorRuntimeId {
                actor_runtime_id: crate::server::game::capture_replay::SESSION,
            },
            action: EnumsPlayerActionType::Stopsleeping,
            face: -1,
            ..Default::default()
        });
        self.replay.tick();
    }

    fn sleeping(&mut self) -> bool {
        let player = self.replay.player;
        self.world().get::<Sleeping>(player).is_some()
    }

    /// Set the scene of `click` up as its script had it.
    fn set_up(&mut self, click: &Click) {
        if click.players == 2 {
            self.join_guest();
        }
        let tick = self.world().resource::<TickCounter>().current;
        let player = self.replay.player;
        let world = self.world();
        world.resource_mut::<WorldTime>().ticks = click.clock;
        let mut rules = world.resource_mut::<GameRules>();
        rules.do_daylight_cycle = click.daylight_cycle;
        rules
            .set_value(
                crate::world::rules::PLAYERS_SLEEPING_PERCENTAGE,
                GameRuleValue::Int(click.percentage),
            )
            .unwrap();
        world.insert_resource(WeatherLevels::holding(
            tick + 1,
            click.rain,
            click.lightning,
        ));
        *world.get_mut::<GameMode>(player).unwrap() = click.mode;
        self.put(DVec3::from_array(click.feet));
        for wall in &click.walls {
            self.replay.set(ivec(*wall), "minecraft:stone");
        }
        if let Some(FixtureBlock(block)) = click.block_on_head {
            let above = IVec3::new(2, -59, 0);
            crate::server::game::blocks::write_block(self.world(), above, block, false);
        }
        if click.bed.head != fixture_head() {
            let bed = click.bed;
            self.place_bed(ivec(bed.head), bed.direction, click.occupied_by_other);
        }
        if let Some(zombie) = click.zombie {
            spawn_mob(
                self.world(),
                "minecraft:zombie",
                DVec3::from_array(zombie),
                0.0,
                SpawnCause::Spawned,
            )
            .unwrap();
        }
        self.replay.drain();
    }

    /// Undo `click`'s scene.
    fn tear_down(&mut self, click: &Click) {
        if self.sleeping() {
            self.leave_bed();
        }
        for wall in &click.walls {
            self.replay.set(ivec(*wall), "minecraft:air");
        }
        if click.block_on_head.is_some() {
            self.replay.set(IVec3::new(2, -59, 0), "minecraft:air");
        }
        if click.bed.head != fixture_head() {
            let head = ivec(click.bed.head);
            let foot = head - Horizontal::from_legacy(click.bed.direction).step();
            self.replay.set(head, "minecraft:air");
            self.replay.set(foot, "minecraft:air");
        }
        let mut mobs = self.world().query_filtered::<Entity, With<MobKind>>();
        let mobs: Vec<Entity> = mobs.iter(self.world()).collect();
        for mob in mobs {
            self.world().despawn(mob);
        }
        self.replay.tick();
        self.replay.drain();
    }
}

/// What our server answered to a click.
fn answer(packets: &[McpePacketData], slept: bool) -> Answer {
    let mut answer = Answer {
        spawn: None,
        messages: Vec::new(),
        event: None,
        slept,
    };
    for packet in packets {
        match packet {
            McpePacketData::SetSpawnPositionPacket(spawn)
                if spawn.spawn_position_type == EnumsSpawnPositionType::Playerrespawn =>
            {
                let pos = |at: &BlockPos| [at.x, at.y, at.z];
                answer.spawn = Some(Spawn {
                    block: pos(&spawn.block_position),
                    bed: pos(&spawn.spawn_block_pos),
                });
            }
            McpePacketData::TextPacket(text) => {
                if let TextPacketBody::MessageOnly(body) = &text.body {
                    let message: serde_json::Value =
                        serde_json::from_str(body.message.to_string().trim_end()).unwrap();
                    for part in message["rawtext"].as_array().unwrap() {
                        if let Some(key) = part["translate"]
                            .as_str()
                            .and_then(|key| key.strip_prefix("tile.bed."))
                        {
                            answer.messages.push(key.to_owned());
                        }
                    }
                }
            }
            McpePacketData::LevelEventGenericPacket(event)
                if event.event_id == roster::LEVEL_EVENT_SLEEPING_PLAYERS
                    && answer.event.is_none() =>
            {
                answer.event = Some(sleeping_counts(event));
            }
            _ => {}
        }
    }
    answer
}

/// (needed, sleeping) of a sleeping-players event, decoded.
fn sleeping_counts(event: &jolyne::valentine::LevelEventGenericPacket) -> [i32; 2] {
    let counts = roster::SleepingPlayers::try_from(event).expect("a sleeping-players event");
    [counts.needed, counts.sleeping]
}

#[test]
fn every_bed_click_is_answered_like_bds() {
    let fixture = fixture();
    let mut scripts: Vec<&str> = fixture.clicks.iter().map(|c| c.script.as_str()).collect();
    scripts.dedup();
    let mut mismatches = Vec::new();
    for script in scripts {
        let mut room = Bedroom::new(&fixture);
        for click in fixture.clicks.iter().filter(|c| c.script == script) {
            room.set_up(click);
            room.click(ivec(click.clicked));
            room.replay.tick();
            let packets = room.replay.drain();
            let slept = room.sleeping();
            let ours = answer(&packets, slept);
            if ours != click.answer {
                mismatches.push(format!(
                    "{} line {}: BDS {:?}, ours {:?}",
                    click.script, click.line, click.answer, ours
                ));
            }
            room.tear_down(click);
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
fn sleepers_stand_up_where_bds_stands_them() {
    let fixture = fixture();
    let mut checked = 0;
    for wake in fixture.wakes.iter().filter(|w| w.cause == WakeKind::Left) {
        let Some(stood) = wake.stood_eye else {
            continue;
        };
        let click = fixture
            .clicks
            .iter()
            .find(|c| c.script == wake.script && c.line == wake.line)
            .unwrap();
        let mut room = Bedroom::new(&fixture);
        room.set_up(click);
        room.click(ivec(click.clicked));
        room.replay.tick();
        assert!(room.sleeping(), "{} line {}", wake.script, wake.line);
        room.replay.drain();
        room.leave_bed();
        let packets = room.replay.drain();
        let player = room.replay.player;
        let eye = room.world().get::<Position>(player).unwrap().0;
        // The column BDS stood the player in; its height is one tick of
        // BDS's movement simulation from there.
        assert_eq!(
            [eye.x as f32, eye.z as f32],
            [stood[0], stood[2]],
            "{} line {}",
            wake.script,
            wake.line
        );
        assert_eq!(
            answer(&packets, false).event,
            wake.event,
            "{} line {}",
            wake.script,
            wake.line
        );
        checked += 1;
    }
    assert!(checked >= 8, "only {checked} stand-ups checked");
}

#[test]
fn the_night_passes_after_a_hundred_ticks_of_sleep() {
    let fixture = fixture();
    for night in &fixture.nights {
        let click = fixture
            .clicks
            .iter()
            .find(|c| c.script == night.script && c.line == night.line)
            .unwrap();
        // The day came by `/time set` (`a_day_or_damage_wakes_a_sleeper`).
        if night.slept_ticks < 99 {
            continue;
        }
        if click.players != 1 && click.percentage == 100 {
            // Both players lay down (the guest ten ticks later): covered by
            // `everyone_must_sleep_unless_the_percentage_says_otherwise`.
            continue;
        }
        let mut room = Bedroom::new(&fixture);
        room.set_up(click);
        room.click(ivec(click.clicked));
        room.replay.tick();
        assert!(room.sleeping());
        let mut ticks = 0;
        let mut packets = Vec::new();
        while room.sleeping() {
            room.replay.tick();
            ticks += 1;
            packets = room.replay.drain();
            assert!(
                ticks <= 200,
                "{} line {} never woke",
                night.script,
                night.line
            );
        }
        // BDS's count is in client inputs: 100 or 101.
        assert!(
            (night.slept_ticks - 1..=night.slept_ticks).contains(&ticks),
            "{} line {}: slept {ticks}, BDS {}",
            night.script,
            night.line,
            night.slept_ticks
        );
        let clocks: Vec<i32> = packets
            .iter()
            .filter_map(|packet| match packet {
                McpePacketData::SyncWorldClocksPacket(sync) => match &sync.data {
                    jolyne::valentine::types::SyncWorldClocksPacketData::SyncStateData(state) => {
                        Some(state.clock_data[0].time)
                    }
                    _ => None,
                },
                _ => None,
            })
            .collect();
        match night.clock_after {
            Some(clock) => assert_eq!(clocks.last().copied(), Some(clock as i32)),
            None => {
                // The clock stopped: it stays where it was.
                assert!(clocks.iter().all(|&time| i64::from(time) == click.clock));
                assert_eq!(room.world().resource::<WorldTime>().ticks, click.clock);
            }
        }
        assert_eq!(
            answer(&packets, false).event,
            None,
            "no event at the morning"
        );
    }
}

#[test]
fn everyone_must_sleep_unless_the_percentage_says_otherwise() {
    assert_eq!(sleepers_needed(1, 100), 1);
    assert_eq!(sleepers_needed(2, 100), 2);
    assert_eq!(sleepers_needed(2, 50), 1);
    assert_eq!(sleepers_needed(3, 50), 2);
    assert_eq!(sleepers_needed(2, 0), 1);
    assert_eq!(GameRules::default().players_sleeping_percentage, 100);
    // sleeping-2 line 192: the player lies down, the guest ten ticks later;
    // the morning comes 100 ticks after the guest.
    let fixture = fixture();
    let click = fixture
        .clicks
        .iter()
        .find(|c| c.script == "sleeping-2" && c.line == 192)
        .unwrap();
    let mut room = Bedroom::new(&fixture);
    room.set_up(click);
    room.place_bed(IVec3::new(2, -60, 4), 3, false);
    room.click(ivec(click.clicked));
    room.replay.tick();
    assert!(room.sleeping());
    for _ in 0..10 {
        room.replay.tick();
    }
    let (guest, _) = room.guest.as_ref().map(|(g, _)| (*g, ())).unwrap();
    super::used(room.world(), guest, IVec3::new(2, -60, 4));
    assert!(room.world().get::<Sleeping>(guest).is_some());
    let mut ticks = 0;
    while room.sleeping() {
        room.replay.tick();
        ticks += 1;
        assert!(ticks <= 200);
    }
    // The guest's click is applied between two ticks here, so one more
    // tick passes than after a routed click.
    assert!(
        (99..=101).contains(&ticks),
        "woke {ticks} ticks after the guest"
    );
}

/// The guest lies down in bed B `after` ticks after the player; returns
/// how many ticks after the player's click the player woke.
fn two_sleepers(fixture: &Fixture, click: &Click, after: u32) -> u32 {
    let mut room = Bedroom::new(fixture);
    room.set_up(click);
    room.place_bed(IVec3::new(2, -60, 4), 3, false);
    room.click(ivec(click.clicked));
    room.replay.tick();
    assert!(room.sleeping());
    let mut ticks = 0;
    for _ in 0..after {
        room.replay.tick();
        ticks += 1;
    }
    let guest = room.guest.as_ref().map(|(guest, _)| *guest).unwrap();
    super::used(room.world(), guest, IVec3::new(2, -60, 4));
    assert!(room.world().get::<Sleeping>(guest).is_some());
    while room.sleeping() {
        room.replay.tick();
        ticks += 1;
        assert!(ticks <= 300);
    }
    ticks
}

#[test]
fn as_many_sleepers_as_needed_must_have_slept_a_hundred_ticks() {
    let fixture = fixture();
    // sleeping-5 line 172: one of two needed, the guest 50 ticks after the
    // player: the morning 100 ticks after the player (BDS 101 inputs).
    let click = fixture
        .clicks
        .iter()
        .find(|c| c.script == "sleeping-5" && c.line == 172)
        .unwrap();
    let night = fixture
        .nights
        .iter()
        .find(|n| n.script == "sleeping-5" && n.line == 172)
        .unwrap();
    let ticks = two_sleepers(&fixture, click, 50);
    assert!(
        (night.slept_ticks - 1..=night.slept_ticks).contains(&u64::from(ticks)),
        "slept {ticks}, BDS {}",
        night.slept_ticks
    );
}

#[test]
fn a_day_or_damage_wakes_a_sleeper() {
    let fixture = fixture();
    for wake in fixture
        .wakes
        .iter()
        .filter(|w| w.cause == WakeKind::Command)
    {
        let command = wake.command.as_deref().unwrap();
        let click = fixture
            .clicks
            .iter()
            .find(|c| c.script == wake.script && c.line == wake.line)
            .unwrap();
        let mut room = Bedroom::new(&fixture);
        room.set_up(click);
        room.click(ivec(click.clicked));
        room.replay.tick();
        assert!(room.sleeping(), "{command}");
        room.replay.drain();
        let player = room.replay.player;
        match command {
            "/time set 1000" => room.world().resource_mut::<WorldTime>().ticks = 1000,
            "/damage @s 1" => {
                crate::entity::player_life::hurt_player(
                    room.world(),
                    player,
                    &crate::entity::mob::combat::Hit {
                        damage: 1.0,
                        cause: unastar_data::actor_components::body::ActorDamageCause::None,
                        attacker: None,
                        by_player: false,
                        knockback_from: None,
                        knockback_power: crate::entity::mob::components::KnockbackPower::DEFAULT,
                    },
                );
            }
            other => panic!("unexpected command {other}"),
        }
        room.replay.tick();
        assert!(!room.sleeping(), "{command} woke the sleeper");
        let packets = room.replay.drain();
        assert_eq!(answer(&packets, false).event, wake.event, "{command}");
        let eye = room.world().get::<Position>(player).unwrap().0;
        let stood = wake.stood_eye.unwrap();
        assert_eq!(
            [eye.x as f32, eye.z as f32],
            [stood[0], stood[2]],
            "{command}"
        );
    }
}

#[test]
fn a_sleeper_who_leaves_frees_its_bed_and_is_saved_beside_it() {
    let fixture = fixture();
    let click = fixture
        .clicks
        .iter()
        .find(|c| c.script == "sleeping-2" && c.line == 192)
        .unwrap();
    let mut room = Bedroom::new(&fixture);
    room.set_up(click);
    room.click(ivec(click.clicked));
    room.replay.tick();
    assert!(room.sleeping());
    let player = room.replay.player;
    let (_, record) = crate::server::game::player_record(room.world(), player).unwrap();
    // Saved where it would stand up, not inside the bed.
    assert_eq!(
        record.position,
        [2.5, -60.0 + WAKE_RISE + PLAYER_EYE_HEIGHT, 1.5]
    );
    room.replay
        .server
        .despawn_player(crate::server::game::capture_replay::SESSION);
    let head = ivec(fixture.bed.head);
    assert_eq!(is_bed_head(room.world(), head), Some(true));
    let block = bed_block(room.world()).unwrap();
    let bed = bed_at(room.world(), block, head).unwrap();
    assert!(!bed.occupied, "the bed is free again");
    // The guest is told nobody sleeps.
    let (_, rx) = room.guest.as_mut().unwrap();
    let packets: Vec<McpePacketData> =
        std::iter::from_fn(|| rx.try_recv().ok().map(|packet| packet.data)).collect();
    let events: Vec<[i32; 2]> = packets
        .iter()
        .filter_map(|packet| match packet {
            McpePacketData::LevelEventGenericPacket(event)
                if event.event_id == roster::LEVEL_EVENT_SLEEPING_PLAYERS =>
            {
                Some(sleeping_counts(event))
            }
            _ => None,
        })
        .collect();
    assert_eq!(events.last(), Some(&[1, 0]));
}

#[test]
fn lying_down_and_waking_send_the_actor_data_bds_sends() {
    let fixture = fixture();
    for event in ["lie_down", "wake"] {
        let expected = fixture
            .actor_data
            .iter()
            .find(|rows| rows.script == "sleeping" && rows.event == event)
            .unwrap();
        let click = fixture
            .clicks
            .iter()
            .find(|c| c.script == "sleeping" && c.line == 212)
            .unwrap();
        let mut room = Bedroom::new(&fixture);
        room.set_up(click);
        room.click(ivec(click.clicked));
        room.replay.tick();
        let mut packets = room.replay.drain();
        if event == "wake" {
            room.leave_bed();
            packets = room.replay.drain();
        }
        let player_id = crate::server::game::capture_replay::SESSION;
        let rows = packets
            .iter()
            .find_map(|packet| match packet {
                McpePacketData::SetActorDataPacket(data)
                    if data.target_runtime_id.actor_runtime_id == player_id
                        && data.actor_data.data.iter().any(|row| {
                            row.id == jolyne::valentine::actor_data_i_ds::PLAYER_FLAGS
                        }) =>
                {
                    Some(
                        data.actor_data
                            .data
                            .iter()
                            .map(row_json)
                            .collect::<Vec<_>>(),
                    )
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("no {event} actor data"));
        let expected: Vec<serde_json::Value> = expected.rows.iter().map(in_f32).collect();
        assert_eq!(
            rows,
            expected,
            "{event} (BDS line {})",
            expected_line(&fixture, event)
        );
    }
}

fn expected_line(fixture: &Fixture, event: &str) -> u32 {
    fixture
        .actor_data
        .iter()
        .find(|rows| rows.script == "sleeping" && rows.event == event)
        .map_or(0, |rows| rows.line)
}

/// A fixture row with its floats as the wire's `f32`s.
fn in_f32(row: &serde_json::Value) -> serde_json::Value {
    match row {
        serde_json::Value::Number(n) if n.is_f64() => {
            serde_json::json!(n.as_f64().unwrap() as f32 as f64)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(in_f32).collect())
        }
        other => other.clone(),
    }
}

fn row_json(row: &DataItemEntry) -> serde_json::Value {
    let (kind, value) = match &row.payload {
        DataItemEntryPayload::DataItemBytePayload(p) => ("byte", serde_json::json!(p.value)),
        DataItemEntryPayload::DataItemInt64Payload(p) => ("int64", serde_json::json!(p.value)),
        DataItemEntryPayload::DataItemPosPayload(p) => {
            ("pos", serde_json::json!([p.value.x, p.value.y, p.value.z]))
        }
        DataItemEntryPayload::DataItemVec3Payload(p) => {
            ("vec3", serde_json::json!([p.value.x, p.value.y, p.value.z]))
        }
        other => panic!("unexpected row {other:?}"),
    };
    serde_json::json!([row.id, kind, value])
}

#[test]
fn a_broken_bed_wakes_its_sleeper_and_no_longer_respawns_it() {
    let fixture = fixture();
    let click = fixture
        .clicks
        .iter()
        .find(|c| c.script == "sleeping" && c.line == 692)
        .unwrap();
    let mut room = Bedroom::new(&fixture);
    room.set_up(click);
    room.click(ivec(click.clicked));
    room.replay.tick();
    assert!(room.sleeping());
    room.replay.set(ivec(fixture.bed.foot), "minecraft:air");
    room.replay.set(ivec(fixture.bed.head), "minecraft:air");
    room.replay.tick();
    assert!(!room.sleeping());
    let player = room.replay.player;
    let point = *room.world().get::<PlayerSpawnPoint>(player).unwrap();
    assert_eq!(point.bed, Some(ivec(fixture.bed.head)));
    room.replay.drain();
    // sleeping-2 line 992: death with the bed gone respawns at the world
    // spawn with `tile.bed.notValid`.
    crate::entity::player_life::kill_player(room.world(), player);
    room.replay.tick();
    super::super::respawn::respawn(room.world(), player);
    let packets = room.replay.drain();
    assert!(room.world().get::<PlayerSpawnPoint>(player).is_none());
    assert!(packets.iter().any(|packet| matches!(packet,
        McpePacketData::TextPacket(text)
            if matches!(&text.body, TextPacketBody::MessageOnly(body)
                if body.message =="{\"rawtext\":[{\"translate\":\"tile.bed.notValid\"}]}\n"))));
}

#[test]
fn a_bed_respawn_point_brings_the_player_back_in_its_block() {
    // sleeping-3 line 182: the point set from (-1.0, -60, 0.5); BDS
    // respawned the player at (-0.5, -60, 0.5).
    let fixture = fixture();
    let click = fixture
        .clicks
        .iter()
        .find(|c| c.script == "sleeping-3" && c.line == 182)
        .unwrap();
    let mut room = Bedroom::new(&fixture);
    room.set_up(click);
    room.click(ivec(click.clicked));
    room.replay.tick();
    room.leave_bed();
    let player = room.replay.player;
    crate::entity::player_life::kill_player(room.world(), player);
    room.replay.tick();
    super::super::respawn::respawn(room.world(), player);
    let eye = room.world().get::<Position>(player).unwrap().0;
    assert_eq!(eye, DVec3::new(-0.5, -60.0 + PLAYER_EYE_HEIGHT, 0.5));
}
