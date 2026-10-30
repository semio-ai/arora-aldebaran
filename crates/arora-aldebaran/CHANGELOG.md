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
- On arora 16, building with Semio Studio is the opt-in: the runner built with the `studio`
  feature connects to Studio on every run unless it passes `--no-studio`, enrolling a
  device that is not enrolled first (by a code entered in Studio, or
  `STUDIO_ENROLLMENT_TOKEN`). `arora-naoqi`'s `--studio` and `ARORA_STUDIO` are gone; to
  migrate, drop them, and build with `studio` a runner that should connect.
- The runner takes `--device <NAME>`, `--device-dir <PATH>` and `--bridge-port <PORT>`
  (`ARORA_DEVICE`, `ARORA_DEVICE_DIR`, `ARORA_BRIDGE_PORT`): the device the run is, so
  several run on one machine. A device's Studio credential is kept under its name in
  Semio's device store.
- On exit, the robot goes to rest: the NAOqi HAL's `rest_on_exit`, on by default.

### Changed

- On Arora 17 and `arora-behavior` 10 (were 16 and 9), with the other dependencies updated
  to their latest compatible versions.
