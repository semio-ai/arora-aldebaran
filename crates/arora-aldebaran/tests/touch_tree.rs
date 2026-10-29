//! `trees/touch.groot.xml` on the Arora runtime against an in-process fake NAOqi: a touch of
//! the front of the head greets once, the greeting plays to its end once started, and the
//! next touch greets again.
// The device against the fake NAOqi: the `naoqi` HAL.
#![cfg(feature = "naoqi")]

use std::time::Duration;

use arora::{Arora, HostModule};
use arora_aldebaran::gestures::{naoqi_gestures, HANDS_READY_DURATION};
use arora_aldebaran::{ready_tree_initial_state, robot, COMMAND_READY, TOUCH_TREE};
use arora_simple_data_store::SimpleDataStore;
use arora_types::data::{DataStore, Key};
use fake_naoqi::FakeNaoqi;

const STEP: Duration = Duration::from_millis(20);

#[tokio::test(flavor = "multi_thread")]
async fn a_touch_of_the_head_greets() {
    let robot = FakeNaoqi::start().await;
    let connected = robot::connect(&robot.url, None)
        .await
        .expect("connect the robot");
    let store = SimpleDataStore::new();
    let mut seed = ready_tree_initial_state(&connected.hal.read_all().await.unwrap());
    // Start at rest: no request, so only a touch plays.
    seed.set.remove(&Key::from(COMMAND_READY));
    store.write(seed).unwrap();
    let mut device = connected
        .install(Arora::builder())
        .with_data_store(Box::new(store.clone()))
        .with_host_module(HostModule::of::<naoqi_gestures::Module>())
        .with_groot(TOUCH_TREE)
        .build()
        .expect("the touch tree resolves against the device's leaves");

    let step = |device: &mut Arora, steps: usize| {
        for _ in 0..steps {
            device.step(STEP).expect("step");
            std::thread::sleep(Duration::from_millis(2));
        }
    };
    step(&mut device, 20);
    assert!(robot.said.lock().unwrap().is_empty(), "untouched, it rests");

    let greetings = |robot: &FakeNaoqi| {
        robot
            .said
            .lock()
            .unwrap()
            .iter()
            .filter(|text| *text == "Hello, you touched my head")
            .count()
    };
    let last_hand_stiffness = |robot: &FakeNaoqi| {
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
    let gesture_steps = (HANDS_READY_DURATION.as_millis() / STEP.as_millis()) as usize + 50;

    // The greeting arms once the tree has seen the head untouched: the HAL reads the sensor
    // at start, in its own time, so the test steps until then.
    let armed = || {
        store
            .read(&[Key::from("touch_tree.armed")])
            .pop()
            .flatten()
            .is_some()
    };
    for _ in 0..200 {
        step(&mut device, 1);
        if armed() {
            break;
        }
    }
    assert!(armed(), "untouched, the greeting arms");

    // A tap: touched, then released while the gesture plays.
    robot.raise_event("FrontTactilTouched", 1.0);
    for _ in 0..200 {
        step(&mut device, 1);
        if last_hand_stiffness(&robot) == Some(0.6) {
            break;
        }
    }
    assert_eq!(last_hand_stiffness(&robot), Some(0.6), "the gesture plays");
    robot.raise_event("FrontTactilTouched", 0.0);
    // Released, the greeting plays to its end: said, and the hands relaxed.
    step(&mut device, gesture_steps);
    let relaxed_by = std::time::Instant::now() + Duration::from_secs(5);
    while last_hand_stiffness(&robot) != Some(0.0) && std::time::Instant::now() < relaxed_by {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        greetings(&robot),
        1,
        "a tap greets: behavior error {:?}, said {:?}",
        device
            .behavior_error()
            .borrow()
            .as_ref()
            .map(|e| e.to_string()),
        robot.said.lock().unwrap()
    );
    assert_eq!(
        last_hand_stiffness(&robot),
        Some(0.0),
        "the hands are relaxed once over"
    );

    // Released, it rests.
    step(&mut device, 20);
    assert_eq!(greetings(&robot), 1, "untouched, it rests");

    // A head held past a greeting greets once.
    robot.raise_event("FrontTactilTouched", 1.0);
    let held_by = std::time::Instant::now() + Duration::from_secs(5);
    while greetings(&robot) < 2 && std::time::Instant::now() < held_by {
        step(&mut device, 5);
    }
    assert_eq!(greetings(&robot), 2, "a held touch greets");
    step(&mut device, 2 * gesture_steps);
    assert_eq!(
        greetings(&robot),
        2,
        "held past the greeting, it does not greet again"
    );
    // Released: the tree re-arms once it sees the release. The events reach the tree in their
    // own time, so the test steps until it sees each one acted on rather than counting steps.
    robot.raise_event("FrontTactilTouched", 0.0);
    for _ in 0..200 {
        step(&mut device, 1);
        if armed() {
            break;
        }
    }
    assert!(armed(), "a release re-arms the greeting");

    // The next touch greets again.
    robot.raise_event("FrontTactilTouched", 1.0);
    for _ in 0..200 {
        step(&mut device, 1);
        if last_hand_stiffness(&robot) == Some(0.6) {
            break;
        }
    }
    assert_eq!(
        last_hand_stiffness(&robot),
        Some(0.6),
        "the gesture plays again"
    );
    robot.raise_event("FrontTactilTouched", 0.0);
    for _ in 0..2 * gesture_steps {
        step(&mut device, 1);
        if greetings(&robot) == 3 {
            break;
        }
    }
    assert_eq!(greetings(&robot), 3, "the next touch greets again");
}
