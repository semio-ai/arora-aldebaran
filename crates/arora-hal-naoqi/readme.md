# arora-hal-naoqi

NAO and Pepper robots as Arora devices, over their NAOqi middleware.

The HAL talks to the robot with [`libqi-vibe`](https://crates.io/crates/libqi-vibe), a Rust
implementation of the `qi` framework: no C++ SDK. Its `arora-naoqi` runner (feature
`runner`) runs the robot as a device, from a computer over the network or on the robot
itself: the [repository's readme](../../README.md) shows how to
[try it on a NAO](../../README.md#try-it-on-a-nao),
[deploy it on the robot](../../README.md#deploy-it-on-the-robot) and connect it to
[Semio Studio](../../README.md#semio-studio). This readme is the HAL's reference.

- [The default behavior](#the-default-behavior)
- [Keys](#keys)
- [Behavior leaves](#behavior-leaves)
- [NAOqi 2.1 and older robots](#naoqi-21-and-older-robots)

## The default behavior

[`trees/ready.groot.xml`](trees/ready.groot.xml), built into the runner:

```text
Fallback
├── Sequence
│   ├── IsStringSet command.ready     (a request is pending)
│   ├── Parallel
│   │   ├── HandsReady      wrists ±20° twice, hands open and close three times, over 4 s
│   │   └── Fallback
│   │       ├── Say "Arora is ready"
│   │       └── Succeed     (a robot that cannot speak still moves)
│   ├── EndSpeech                     (the episode ends: the next request
│   ├── EndGestures                    speaks and moves anew)
│   └── UnsetString command.ready     (the request is handed back)
└── Succeed                 (rest)
```

`IsStringSet` and `UnsetString` are native behavior-tree nodes. The runner starts with a
request in `command.ready` (any non-empty text). The `r` key writes a new one; so does any
bridge client, Studio included: the runner opens `command.ready` to remote writers. The gesture stiffens the four joints it moves (0.6) and relaxes them once over.

## Keys

The keys are those of an Arora robot standard ([`standard/arora-robot.json`](standard/arora-robot.json)):
an interface, as data, of the keys a robot exposes to Arora with their kind, type, unit and
range, independent of NAOqi. The standard is local to this HAL until Arora has one; the HAL
is its mapping onto NAOqi, and a test checks the two agree.

Joints are named: the NAOqi joint names in snake case (`HeadYaw` → `head_yaw`,
`LShoulderPitch` → `l_shoulder_pitch`), unless the configuration overrides them. A robot
model whose joints carry generated ids (a GLB from Studio) maps those ids to these names on
its side.

An id is a key's entity: the HAL refuses a configuration that maps a joint to an empty id,
to an id with a `.`, or two joints to the same id.

| Key | Direction | NAOqi source or target |
|---|---|---|
| `<joint>.position` (rad; a hand's opening, 0 to 1), `.velocity` (per second, derived), `.stiffness`, `.temperature` (°C), `.current` (A) | sensed | `ALMemory` `Device/SubDeviceList/<Joint>/…` |
| `battery.charge` (0..1), `battery.current` (A), `battery.charging` | sensed | `ALMemory` battery keys |
| `imu.angle.{x,y}`, `imu.gyroscope.{x,y,z}`, `imu.accelerometer.{x,y,z}` | sensed | `ALMemory` inertial sensor keys |
| `sonar.{left,right}.distance` (m) | sensed | `ALMemory` ultrasound keys |
| `touch.head.{front,middle,rear}`, `touch.hand.{left,right}.{back,left,right}`, `bumper.{left,right}`, `chest_button` | sensed (events) | `ALMemory` tactile events |
| `<joint>.target_position` (rad; a hand's opening, 0 to 1) | commanded | `ALMotion.setAngles` |
| `<joint>.target_stiffness` (0..1) | commanded | `ALMotion.setStiffnesses` |
| `led.<group>.color` (`0xRRGGBB`), `led.<group>.intensity` | commanded | `ALLeds.fadeRGB`, `ALLeds.setIntensity` |
| `velocity.{x,y}`, `rotation.z` (normalized -1..1) | commanded | `ALMotion.moveToward` |

Numeric sensor keys are sampled from `ALMemory` at `sensor_period_ms` (50 ms by default)
in one `getListData` call, and only changed values are reported. Touch keys are driven
by `ALMemory` events.

The sonars measure only while someone subscribes to `ALSonar`: with sonar sensing on
(`sensors.sonar`, the default), the HAL does, and unsubscribes when it goes.

Commands reach the robot in the order they are written. When the HAL goes (the runner
stops), a base that may be moving is stopped (`ALMotion.moveToward(0, 0, 0)`), and commands
still queued are not sent.

The HAL receives and publishes keys; it sets no key meta. Which keys remote writers may
change is the device's to say, as with the other HALs: the runner opens
`command.ready` only.

## Behavior leaves

What a robot does over time is a behavior tree's; the HAL gives trees two host modules:

- **`Say`** ([`say`](src/say.rs)): the speech contract of Arora's trees
  (`say(text, voice, viseme)`, under the ids its other providers, `say-silent` and Piper's,
  share) in the robot's voice, `ALTextToSpeech`. `Running` while the
  robot speaks, then `Success` while ticked; a new text, or the leaf halted for 250 ms,
  stops the sentence. `EndSpeech` ends the episode: it stops the sentence under way and
  forgets the said ones, so the same text is said again when asked again. The runner
  installs the robot's voice with `say::install(hal.voice())`.
- **`HandsReady`** ([`gestures`](src/gestures.rs)): the default behavior's gesture, timed
  joint targets and stiffness on the wrists and hands, paced by the tree's `arora/dt`.
  `EndGestures` ends the episode, so the next `HandsReady` plays from its start.

The default tree also uses the native `IsStringSet` and `UnsetString` nodes.

## NAOqi 2.1 and older robots

`libqi-vibe` is validated against libqi 4 (NAOqi 2.5 and later) and against `naoqi-sim`, its
simulated NAOqi. NAOqi 2.1 is older, and this HAL has not run on it yet:

- **The connection.** NAOqi 2.1's libqi may predate the capability exchange and the
  authentication that `libqi-vibe` opens a session with. If the runner cannot connect,
  the logs at `RUST_LOG=debug` show where the handshake stops: please report them.
- **The kernel.** NAO's NAOqi 2.1 system runs Linux 2.6.33; Rust's standard library
  targets 3.2 and later. The static musl binary is expected to run, untested. If it does
  not start on the robot, [run it from a computer](../../README.md#2-run-it-from-your-computer)
  instead.
- **Joints and services.** The HAL discovers the joints with `ALMotion.getBodyNames` and
  uses `ALMotion.setAngles`, `setStiffnesses`, `ALTextToSpeech.say` and `stopAll`, all
  present in NAOqi 2.1.
