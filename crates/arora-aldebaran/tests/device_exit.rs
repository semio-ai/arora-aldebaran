//! The arora-aldebaran device stopped while the default tree's gesture plays, against an
//! in-process fake NAOqi. Its own test binary: the gesture's clock is the process's, and
//! would race `device.rs`.
// The device against the fake NAOqi: the `naoqi` HAL.
#![cfg(feature = "naoqi")]

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use arora::{Arora, HostModule};
use arora_aldebaran::gestures::{naoqi_gestures, GESTURE_STIFFNESS};
use arora_aldebaran::{ready_tree_initial_state, robot, READY_TREE};
use arora_simple_data_store::SimpleDataStore;
use arora_types::data::DataStore;
use fake_naoqi::FakeNaoqi;

const STEP: Duration = Duration::from_millis(20);
const GESTURE_JOINTS: [&str; 4] = ["LWristYaw", "RWristYaw", "LHand", "RHand"];

/// The device stops (Ctrl-C, `q`) in the middle of `HandsReady`: the tree no longer ticks
/// the gesture to its end, so it cannot relax the four joints it holds; the HAL puts the
/// robot to rest, which relaxes them.
#[tokio::test(flavor = "multi_thread")]
async fn stopping_mid_gesture_puts_the_robot_to_rest() {
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
    let mut device = connected
        .install(Arora::builder())
        .with_data_store(Box::new(store))
        .with_host_module(HostModule::of::<naoqi_gestures::Module>())
        .with_groot(READY_TREE)
        .build()
        .expect("the default tree resolves against the device's leaves");

    let last_stiffness = |joint: &str| {
        robot
            .set_stiffnesses
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find_map(|(names, values)| {
                names
                    .iter()
                    .position(|name| name == joint)
                    .map(|index| values[index])
            })
    };
    // A second into the four of the gesture, its stiffness has reached the robot.
    for _ in 0..50 {
        device.step(STEP).expect("step");
        std::thread::sleep(Duration::from_millis(2));
    }
    let stiffened_by = Instant::now() + Duration::from_secs(5);
    while GESTURE_JOINTS
        .iter()
        .any(|joint| last_stiffness(joint) != Some(GESTURE_STIFFNESS as f32))
        && Instant::now() < stiffened_by
    {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    for joint in GESTURE_JOINTS {
        assert_eq!(
            last_stiffness(joint),
            Some(GESTURE_STIFFNESS as f32),
            "{joint} is held by the gesture"
        );
    }
    assert_eq!(robot.rested.load(Ordering::SeqCst), 0);

    drop(device);
    assert_eq!(
        robot.rested.load(Ordering::SeqCst),
        1,
        "dropping the device drops the HAL, which puts the robot to rest"
    );
}
