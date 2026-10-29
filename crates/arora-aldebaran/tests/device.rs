//! The arora-aldebaran device end to end against an in-process fake NAOqi: the NAOqi HAL, the
//! default tree and its leaves (Say, HandsReady, the native IsStringSet and UnsetString) on
//! the Arora runtime.
// The device against the fake NAOqi: the `naoqi` HAL.
#![cfg(feature = "naoqi")]

use std::time::Duration;

use arora::{Arora, HostModule};
use arora_aldebaran::gestures::{naoqi_gestures, HANDS_READY_DURATION};
use arora_aldebaran::{ready_tree_initial_state, robot, COMMAND_READY, READY_REQUEST, READY_TREE};
use arora_hal_naoqi::say;
use arora_simple_data_store::SimpleDataStore;
use arora_types::data::{DataStore, Key, StateChange};
use arora_types::value::Value;
use fake_naoqi::FakeNaoqi;

const STEP: Duration = Duration::from_millis(20);

fn request(store: &SimpleDataStore) -> Option<Value> {
    store.read(&[Key::from(COMMAND_READY)]).pop().flatten()
}

/// The default tree says "Arora is ready" while the hands play their gesture, then hands
/// the request back; requested again, after a pause or at once, it plays again.
#[tokio::test(flavor = "multi_thread")]
async fn the_ready_tree_speaks_moves_the_hands_and_plays_again_on_request() {
    let robot = FakeNaoqi::start().await;
    let connected = robot::connect(&robot.url, None)
        .await
        .expect("connect the robot");
    let store = SimpleDataStore::new();
    store
        .write(ready_tree_initial_state(
            &connected.hal.read_all().await.unwrap(),
        ))
        .unwrap();
    let ready = || StateChange::set(COMMAND_READY, Value::String(READY_REQUEST.to_string()));
    let mut device = connected
        .install(Arora::builder())
        .with_data_store(Box::new(store.clone()))
        .with_host_module(HostModule::of::<naoqi_gestures::Module>())
        .with_groot(READY_TREE)
        .build()
        .expect("the default tree resolves against the device's leaves");

    // Lockstep: tree time advances a step per iteration, the robot answers meanwhile.
    let play = |device: &mut Arora| {
        let steps = HANDS_READY_DURATION.as_millis() / STEP.as_millis() + 50;
        for _ in 0..steps {
            device.step(STEP).expect("step");
            std::thread::sleep(Duration::from_millis(2));
            if request(&store).is_none() {
                return;
            }
        }
        panic!(
            "the ready behavior did not hand the request back: behavior error {:?}, \
             command.ready {:?}, said {:?}, l_hand target {:?}",
            device
                .behavior_error()
                .borrow()
                .as_ref()
                .map(|e| e.to_string()),
            request(&store),
            robot.said.lock().unwrap(),
            store.read(&[Key::from("l_hand.target_position")]),
        );
    };
    play(&mut device);

    assert_eq!(
        robot.said.lock().unwrap().as_slice(),
        ["Arora is ready".to_string()]
    );
    let moved: Vec<String> = robot
        .set_angles
        .lock()
        .unwrap()
        .iter()
        .flat_map(|(names, _, _)| names.clone())
        .collect();
    for joint in ["LWristYaw", "RWristYaw", "LHand", "RHand"] {
        assert!(moved.iter().any(|name| name == joint), "{joint} moved");
    }
    // The relaxing stiffness leaves in the step that hands the request back, through the
    // HAL's queued writes: it reaches the robot shortly after the store reads `idle`.
    let last_hand_stiffness = || {
        robot
            .set_stiffnesses
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find_map(|(names, values)| {
                names
                    .iter()
                    .position(|name| name == "LHand")
                    .map(|index| values[index])
            })
    };
    let relaxed_by = std::time::Instant::now() + Duration::from_secs(5);
    while last_hand_stiffness() != Some(0.0) && std::time::Instant::now() < relaxed_by {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(last_hand_stiffness().is_some(), "the hand was stiffened");
    assert_eq!(
        last_hand_stiffness(),
        Some(0.0),
        "the hands are relaxed once over"
    );

    // Requested again, after a pause: it speaks again.
    std::thread::sleep(say::IDLE_STOP + Duration::from_millis(50));
    store.write(ready()).unwrap();
    play(&mut device);
    assert_eq!(robot.said.lock().unwrap().len(), 2);

    // Requested again at once: the episode ended, so it speaks anew.
    store.write(ready()).unwrap();
    play(&mut device);
    assert_eq!(robot.said.lock().unwrap().len(), 3);
}
