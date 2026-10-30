# Changelog

All notable changes to `arora-hal-naoqi-dcm`. The format follows
[Keep a Changelog](https://keepachangelog.com/); versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- The crate: the DCM HAL. Its plugin (`naoqi::plugin`) runs in the loop of the DCM: it reads
  the sensors in place in `ALMemory` every cycle and sends what changed to the HAL's feeds,
  and sends the HAL's latest commands (joint targets and stiffness, LED groups) through DCM
  aliases, each joint stepping toward its target at most at `joint_speed_limit`. `DcmHal`
  is the `arora_hal::Hal` a device runs on: it puts the robot to rest when it starts (and does
  not start when the robot does not rest) and when it goes (`rest_at_start`, `rest_on_exit`), turns Autonomous Life off, subscribes to the
  sonars, and gives the robot's voice. `DcmRobotConfig` configures it. The plugin probes the
  joints, sensors and LEDs the robot has, from NAO's and Pepper's.

### Changed

- The dependencies are updated to their latest compatible versions.
