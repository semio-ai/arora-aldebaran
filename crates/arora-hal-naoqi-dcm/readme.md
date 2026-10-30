# arora-hal-naoqi-dcm

The DCM HAL: NAO and Pepper as Arora HALs from inside NAOqi, paced by the loop of the DCM.

Its two parts share a `DcmLink`:

- the DCM plugin (`naoqi::plugin`, for a `#[qi::dcm]` function of a `#[qi::module]` of
  [`libqi-vibe`](https://crates.io/crates/libqi-vibe)), in the loop of the DCM: it reads the
  sensors in `ALMemory` every cycle and sends what changed to the HAL's feeds, and sends the
  latest commands to the actuators through DCM aliases, each joint stepping toward its target
  at most at `joint_speed_limit`;
- `DcmHal`, the `arora_hal::Hal` a device runs on, at its own pace: it reads the feeds, hands
  its commands to the plugin, and calls NAOqi's services for what is not a key (its voice,
  Autonomous Life, the rest postures, the sonars, the description), with
  [`arora-hal-naoqi`](../arora-hal-naoqi)'s `services`.

The keys are those of the Arora robot standard, as for `arora-hal-naoqi`. The library NAOqi
loads, which runs a device on this HAL, is [`arora-naoqi-module`](../arora-naoqi-module); the
[repository's README](../../README.md#inside-naoqi-on-the-loop-of-the-dcm) documents both.
