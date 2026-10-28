//! `trees/touch.groot.xml` on the Arora runtime against an in-process fake NAOqi: a touch of
//! the front of the head greets.

#[allow(dead_code)]
mod fake_naoqi;

use std::time::Duration;

use arora::{Arora, HostModule};
use arora_hal::Hal;
use arora_hal_naoqi::gestures::naoqi_gestures;
use arora_hal_naoqi::say::{self, naoqi_say};
use arora_hal_naoqi::{ready_tree_initial_state, NaoqiHal, NaoqiRobotConfig, COMMAND_READY};
use arora_simple_data_store::SimpleDataStore;
use arora_types::data::{DataStore, Key};
use fake_naoqi::FakeNaoqi;

const TOUCH_TREE: &str = include_str!("../trees/touch.groot.xml");
const STEP: Duration = Duration::from_millis(20);

#[tokio::test(flavor = "multi_thread")]
async fn a_touch_of_the_head_greets() {
    let robot = FakeNaoqi::start().await;
    let hal = NaoqiHal::new(NaoqiRobotConfig::with_url(&robot.url))
        .await
        .expect("connect the HAL");
    say::install(hal.voice());
    let store = SimpleDataStore::new();
    let mut seed = ready_tree_initial_state(&hal.read_all().await.unwrap());
    // Start at rest: no request, so only a touch plays.
    seed.set.remove(&Key::from(COMMAND_READY));
    store.write(seed).unwrap();
    let mut device = Arora::builder()
        .with_data_store(Box::new(store.clone()))
        .with_hal(Box::new(hal))
        .with_host_module(HostModule::of::<naoqi_say::Module>())
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

    robot.raise_event("FrontTactilTouched", 1.0);
    let greeted = |robot: &FakeNaoqi| {
        robot
            .said
            .lock()
            .unwrap()
            .iter()
            .any(|text| text == "Hello, you touched my head")
    };
    for _ in 0..100 {
        step(&mut device, 5);
        if greeted(&robot) {
            break;
        }
    }
    assert!(
        greeted(&robot),
        "a touch greets: behavior error {:?}, said {:?}",
        device
            .behavior_error()
            .borrow()
            .as_ref()
            .map(|e| e.to_string()),
        robot.said.lock().unwrap()
    );
    let moved = robot
        .set_angles
        .lock()
        .unwrap()
        .iter()
        .any(|(names, _, _)| names.iter().any(|name| name == "LHand"));
    assert!(moved, "the hands play the gesture");
}
