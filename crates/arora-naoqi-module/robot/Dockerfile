# The C++ compiler of the bridge to the DCM, for robots: GCC 4.8 and its libstdc++ (Ubuntu
# 14.04), with 32-bit support: an old compiler, whose code needs no more of the robot's
# libstdc++ (GCC 4.5 on NAOqi 2.1) than it has, which build.sh checks; its Boost (1.54)
# serves SDKs without one, such as the stub of the tests.
FROM ubuntu:14.04
# `apt-get update` fails on the image's ESM source, which the packages below do not need.
RUN apt-get update; DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      g++-multilib binutils libboost-dev && apt-get clean
