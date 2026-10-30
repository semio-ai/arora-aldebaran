# Changelog

All notable changes to `arora-aldebaran`. The format follows
[Keep a Changelog](https://keepachangelog.com/); versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- The crate: the device runner of Aldebaran robots, `arora-aldebaran`, which was
  `arora-naoqi` in `arora-hal-naoqi` (feature `runner`). It drives the robot through the HAL
  its build selects: `naoqi` (the default), `arora-hal-naoqi`; a build without one is
  refused. `robot::connect` connects the build's HAL from what names the robot, and
  `Robot::install` gives the device its HAL and its leaves.
- From `arora-hal-naoqi`: the trees (`READY_TREE`, and `TOUCH_TREE`, a greeting when the
  head is touched), `ready_tree_initial_state` and the gesture leaves (`HandsReady`,
  `EndGestures`), which name the keys of the Arora robot standard only.
- `EndGestures`: a leaf that ends a gesture episode, so the next `HandsReady` plays from its
  start; the default tree calls it, with `EndSpeech`, before handing the request back.
- Building with Semio Studio is the opt-in: the runner built with the `studio` feature
  serves the device's Studio bridge on every run unless it passes `--no-studio`.
  `arora-naoqi`'s `--studio` and `ARORA_STUDIO` are gone; to migrate, drop them, and build
  with `studio` a runner that should connect.
- The runner takes `--device <NAME>`, `--device-dir <PATH>` and `--bridge-port <PORT>`
  (`ARORA_DEVICE`, `ARORA_DEVICE_DIR`, `ARORA_BRIDGE_PORT`): the device the run is, so
  several run on one machine. A device's Studio credential is kept under its name in
  Semio's device store.
- On exit, the robot goes to rest: the NAOqi HAL's `rest_on_exit`, on by default.

### Changed

- **Breaking:** on arora 18 (was 16): `Robot::install` takes and returns arora 18's
  `AroraBuilder`. The runner built with `studio` runs the robot at once, with the device's
  Studio bridge from its start, and a device that is not enrolled with Studio enrolls while
  it runs (`StudioHandle::enroll_when_needed`): by the token in `STUDIO_ENROLLMENT_TOKEN`,
  or by a code once a person accepts to enroll. The terminal UI asks whether to enroll, the
  device's name and its model family, shows the code in its prompt line, then the device's
  Studio record and id in its header; headless, the code is logged. A refused enrollment
  fails the run, and so does a Studio bridge that halts, such as on a credential of another
  Firebase project than the run's, the error naming the credentials directory to delete;
  Studio out of reach no longer fails the run. To migrate: a caller of `Robot::install`
  builds against arora 18; a script that waited for the run to enroll before relying on
  Studio waits for the device to show in Studio instead; `--no-studio` and the credential
  store are unchanged.
- On `arora-behavior` 10 (was 9), with the other dependencies updated to their latest
  compatible versions.
