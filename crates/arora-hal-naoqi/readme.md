# arora-hal-naoqi

The NAOqi HAL: NAO and Pepper robots as Arora HALs, over their NAOqi middleware, through
[`libqi-vibe`](https://crates.io/crates/libqi-vibe), a Rust implementation of the `qi`
framework (no C++ SDK).

`NaoqiHal::new(NaoqiRobotConfig)` connects to the robot and gives an `arora_hal::Hal`: joint
states, battery, inertial unit, sonars and touch sensors flow in as keys; joint targets,
stiffness, LEDs and base velocity flow out to `ALMotion` and `ALLeds`. Its voice
(`NaoqiHal::voice`, installed with `say::install`) speaks the trees' `Say` leaves with
`ALTextToSpeech`. The keys are those of the Arora robot standard
([`standard/arora-robot.json`](standard/arora-robot.json)), which `keys` maps NAOqi onto.

The device runner that runs a robot with this HAL is
[`arora-aldebaran`](../arora-aldebaran), built with its `naoqi` feature (the default). The
[repository's README](../../README.md) documents both:

- [The NAOqi HAL](../../README.md#the-naoqi-hal): its keys, what it does when it goes, its
  [configuration](../../README.md#configuration), and
  [NAOqi 2.1 and older robots](../../README.md#naoqi-21-and-older-robots);
- [Leaves](../../README.md#leaves): `Say` and `EndSpeech`.

`examples/naoqi_probe.rs` connects to a robot and prints what the HAL reads:
`cargo run -p arora-hal-naoqi --example naoqi_probe -- tcp://<robot_name>.local:9559`.
