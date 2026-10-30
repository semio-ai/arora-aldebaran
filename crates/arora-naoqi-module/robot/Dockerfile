# The C++ compiler of the bridge to the DCM, for robots: GCC 4.8 and its libstdc++ (Ubuntu
# 14.04), as old as the libstdc++ of NAOqi 2.1 needs, with 32-bit support; its Boost (1.54)
# serves SDKs without one, such as the stub of the tests.
FROM ubuntu:14.04
# `apt-get update` fails on the image's ESM source, which the packages below do not need.
RUN apt-get update; DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      g++-multilib binutils libboost-dev && apt-get clean
