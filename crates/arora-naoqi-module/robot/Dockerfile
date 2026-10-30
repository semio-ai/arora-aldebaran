# The C++ compiler of the bridge to the DCM, for robots: GCC 4.8 and its libstdc++ (Ubuntu
# 14.04), with 32-bit support: an old compiler, whose code needs no more of the robot's
# libstdc++ (GCC 4.5 on NAOqi 2.1) than it has, which build.sh checks; and the Boost of NAOqi
# 2.1 (1.55), for the declarations of naoqi-2.1/.
FROM ubuntu:14.04
# `apt-get update` fails on the image's ESM source, which the packages below do not need.
RUN apt-get update; DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      g++-multilib binutils libboost1.55-dev && apt-get clean
