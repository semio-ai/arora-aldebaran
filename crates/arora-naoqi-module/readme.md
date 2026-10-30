# arora-naoqi-module

An Aldebaran robot (NAO, Pepper) as an Arora device, inside NAOqi: `libarora_naoqi.so`, the
library NAOqi loads from its `autoload.ini`. Its DCM plugin, `AroraDcm`, runs the
[DCM HAL](../arora-hal-naoqi-dcm) in the loop of the DCM, and starts the device on a thread of
its own: the default behavior of [`arora-aldebaran`](../arora-aldebaran), on the DCM HAL,
serving its bridges. The service `Arora` reports what it does (`qicli call Arora.status`).

- `robot/build.sh [i686|x86_64]` builds the library for a robot, and its package, against
  `robot/naoqi-2.1/` (what the bridge uses of NAOqi 2.1's C++ SDK, nothing to download), or
  against the SDK `NAOQI_SDK` names;
- `arora_naoqi.json` configures the device (`config::ModuleConfig`);
- `tests/fake_dcm.rs` runs it in a fake NAOqi (`interop/fake-dcm`);
- [`robot-test-plan.md`](robot-test-plan.md) is the plan to try it on a robot.

The [repository's README](../../README.md#inside-naoqi-on-the-loop-of-the-dcm) documents it.
