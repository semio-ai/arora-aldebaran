//! Aldebaran robots (NAO, Pepper) as Arora devices.
//!
//! The `arora-aldebaran` runner drives the robot through one HAL, chosen when it is built:
//! [`robot`] connects to the robot with it. The rest does not depend on the HAL: the
//! behavior trees ([`READY_TREE`], [`TOUCH_TREE`]) and their [`gestures`] name the keys of
//! the Arora robot standard, which every HAL maps, so they run on any of them.
//!
//! See the [repository's README](https://github.com/semio-ai/arora-aldebaran#readme).

pub mod gestures;
pub mod robot;

use arora_types::data::{Key, State, StateChange};
use arora_types::value::Value;

/// The default behavior of the runner, a Groot tree: while [`COMMAND_READY`] holds a
/// request (any non-empty text), the robot says "Arora is ready" as its hands play a
/// gesture, then hands the request back (unsets the key). Its leaves are `Say` (the HAL's
/// voice), [`gestures`] and the native `IsStringSet` and `UnsetString`.
pub const READY_TREE: &str = include_str!("../trees/ready.groot.xml");

/// A tree to try a robot with: the default behavior on request, and a greeting when the
/// front of the head is touched, once per touch.
pub const TOUCH_TREE: &str = include_str!("../trees/touch.groot.xml");

/// The key that requests the default behavior: any non-empty text plays it once.
pub const COMMAND_READY: &str = "command.ready";

/// The text the runner writes to [`COMMAND_READY`] to request the default behavior.
pub const READY_REQUEST: &str = "ready";

/// The joints the default behavior's gesture moves, by their standard ids.
pub const READY_TREE_JOINTS: [&str; 4] = ["l_wrist_yaw", "r_wrist_yaw", "l_hand", "r_hand"];

/// The rest shape of the say contract's `viseme`, which the trees bind to `speech.viseme`.
pub const SILENCE_VISEME: &str = "sil";

/// The keys the default behavior's leaves write, seeded so that they bind: a tree's
/// out-parameter binds to a key that exists. Joint targets start where the robot is
/// (`state`, its HAL's [`read_all`](arora_hal::Hal::read_all)), so that writing them
/// moves nothing. [`COMMAND_READY`] starts requested: the behavior plays at start.
pub fn ready_tree_initial_state(state: &State) -> StateChange {
    let measured = |joint: &str, component: &str| {
        state
            .get(&Key::from(format!("{joint}.{component}")))
            .cloned()
            .flatten()
            .unwrap_or(Value::F64(0.0))
    };
    let mut change = StateChange::set(COMMAND_READY, Value::String(READY_REQUEST.to_string()));
    change.set.insert(
        Key::from("speech.viseme"),
        Some(Value::String(SILENCE_VISEME.to_string())),
    );
    // The standard joint keys: `<joint>.position` and `.stiffness` measured,
    // `.target_position` and `.target_stiffness` commanded.
    for joint in READY_TREE_JOINTS {
        change.set.insert(
            Key::from(format!("{joint}.target_position")),
            Some(measured(joint, "position")),
        );
        change.set.insert(
            Key::from(format!("{joint}.target_stiffness")),
            Some(measured(joint, "stiffness")),
        );
    }
    change
}
