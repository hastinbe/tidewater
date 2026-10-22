//! The lobby protocol between two machines in one process, over loopback TCP, each on its own thread, through the
//! program's public API only (`net::room`): the creator opens a room, a joiner enters it, they change races and talk,
//! the creator starts, and both get the same game. No game files are needed.

use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use tidewater_client::net::proto::{Identity, Msg, RowState};
use tidewater_client::net::room::{self, GuestEvent, HostRoom, RowChoice, Texts};
use tidewater_client::net::tcp::Inbound;
use tidewater_sim::setup::race;

const WAIT: Duration = Duration::from_secs(15);

fn room(enhanced: bool, identity: Identity) -> HostRoom {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut view = room::new_view(enhanced, "Host", race::HUMAN);
    view.map_name = "Maps\\synthetic.pud".into();
    view.map_hash = 77;
    view.map_players = 4;
    HostRoom::new(listener, identity, Texts::default(), view).unwrap()
}

#[test]
fn two_machines_fill_a_room_talk_and_start_the_same_game() {
    let mut host = room(true, Identity::ours(5));
    let addr = format!("127.0.0.1:{}", host.port());
    // The joiner's machine: connect, accept, then play the room until the game starts.
    let (said, heard) = mpsc::channel();
    let joiner = thread::spawn(move || {
        let hs = room::handshake(&addr, Identity::ours(5), "Ann").unwrap();
        assert_eq!(
            hs.config.map_hash, 77,
            "the joiner is shown the creator's map before it accepts"
        );
        let mut guest = hs.accept().unwrap();
        guest.set_race(race::ORC);
        guest.say("good luck");
        let end = Instant::now() + WAIT;
        while Instant::now() < end {
            for e in guest.poll() {
                match e {
                    GuestEvent::View => {
                        if guest.view.as_ref().is_some_and(|v| v.rows[0].say == "and you") {
                            let _ = said.send(());
                        }
                    }
                    GuestEvent::Go(start) => return Some(start),
                    GuestEvent::Ended(why) => panic!("the room ended: {why}"),
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        None
    });
    // The creator's machine.
    let end = Instant::now() + WAIT;
    while Instant::now() < end && !(host.view().rows[1].race == race::ORC && host.view().rows[1].say == "good luck") {
        host.poll();
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(host.view().rows[1].state, RowState::Human);
    assert_eq!(host.view().rows[1].name, "Ann");
    host.say("and you");
    host.set_row(2, RowChoice::Computer);
    let end = Instant::now() + WAIT;
    while Instant::now() < end && heard.try_recv().is_err() {
        host.poll();
        thread::sleep(Duration::from_millis(5));
    }
    assert!(host.can_start().is_ok());
    let mine = host.start(4242).unwrap();
    let theirs = joiner.join().unwrap().expect("the joiner is sent the game");
    assert_eq!((mine.me, theirs.me), (0, 1));
    assert_eq!(mine.config.seed, 4242);
    assert_eq!(mine.config.lobby.humans, theirs.config.lobby.humans);
    assert_eq!(mine.config.lobby.computers, [2]);
    assert_eq!(mine.config.lobby.humans[1], (1, race::ORC));
    // The game's wires carry a turn from one to the other.
    let (mut a, mut b) = (mine.wire, theirs.wire);
    a.send(Msg::Turn {
        slot: 0,
        seq: 1,
        cmd: None,
    });
    let end = Instant::now() + WAIT;
    let mut got = false;
    while !got && Instant::now() < end {
        got = b.poll().iter().any(|i| matches!(i, Inbound::Msg { from: 0, .. }));
        thread::sleep(Duration::from_millis(5));
    }
    assert!(got);
}

#[test]
fn a_joiner_whose_game_data_differs_is_refused_with_the_reason() {
    let mut host = room(true, Identity::ours(1));
    let addr = format!("127.0.0.1:{}", host.port());
    let t = thread::spawn(move || room::handshake(&addr, Identity::ours(2), "Ann"));
    let end = Instant::now() + WAIT;
    while Instant::now() < end && !t.is_finished() {
        host.poll();
        thread::sleep(Duration::from_millis(5));
    }
    let err = t.join().unwrap().unwrap_err();
    assert!(format!("{err:#}").contains("game data"), "{err:#}");
    assert_eq!(host.view().humans(), 1);
}
