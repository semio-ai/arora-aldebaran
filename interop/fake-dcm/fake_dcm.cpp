// A fake NAOqi process for the tests of arora-naoqi-module: loads the library the way NAOqi
// loads the libraries of its autoload.ini (`_createModule(broker)`), with a broker of the stub
// of the NAOqi C++ SDK of libqi-rs-vibe (interop/naoqi-sdk-stub) holding the memory of a NAO,
// then runs the loop of its DCM every 10 ms. The commands the plugins send are applied to the
// memory: a position actuator moves its joint's sensor there at once.
//
// Usage: fake-dcm <library> <seconds>
// Prints the modules, then, once the loop ends, `alias <name> <count> <last values>` for each
// alias commanded, then destroys the modules (as NAOqi does when it stops) and calls
// `_closeModule`.
#include <alcommon/albroker.h>
#include <alcommon/albrokermanager.h>
#include <alcommon/almodule.h>
#include <alproxies/almemoryproxy.h>
#include <alproxies/dcmproxy.h>
#include <dlfcn.h>
#include <cstdio>
#include <cstdlib>
#include <map>
#include <string>
#include <unistd.h>

typedef int (*CreateModule)(boost::shared_ptr<AL::ALBroker>);
typedef int (*CloseModule)();

static const char* const kJoints[] = {
    "HeadYaw", "HeadPitch", "LShoulderPitch", "LShoulderRoll", "LElbowYaw", "LElbowRoll",
    "LWristYaw", "LHand", "LHipYawPitch", "LHipRoll", "LHipPitch", "LKneePitch", "LAnklePitch",
    "LAnkleRoll", "RHipRoll", "RHipPitch", "RKneePitch", "RAnklePitch", "RAnkleRoll",
    "RShoulderPitch", "RShoulderRoll", "RElbowYaw", "RElbowRoll", "RWristYaw", "RHand"};

static std::string device(const std::string& path) { return "Device/SubDeviceList/" + path + "/Value"; }

static void populate(std::map<std::string, float>& memory) {
  for (size_t i = 0; i < sizeof kJoints / sizeof *kJoints; ++i) {
    const std::string joint = kJoints[i];
    memory[device(joint + "/Position/Sensor")] = 0.0f;
    memory[device(joint + "/Position/Actuator")] = 0.0f;
    memory[device(joint + "/Hardness/Actuator")] = 0.0f;
    memory[device(joint + "/Temperature/Sensor")] = 30.0f;
    memory[device(joint + "/ElectricCurrent/Sensor")] = 0.01f;
  }
  // The second hip yaw-pitch of NAO is the first's motor: a sensor without actuator.
  memory[device("RHipYawPitch/Position/Sensor")] = 0.0f;
  memory[device("Battery/Charge/Sensor")] = 0.8f;
  memory[device("Battery/Current/Sensor")] = -0.3f;
  memory[device("InertialSensor/AngleX/Sensor")] = 0.01f;
  const char* touches[] = {"Head/Touch/Front", "Head/Touch/Middle", "Head/Touch/Rear",
                           "ChestBoard/Button", "LFoot/Bumper/Left", "LFoot/Bumper/Right"};
  for (size_t i = 0; i < sizeof touches / sizeof *touches; ++i)
    memory[device(std::string(touches[i]) + "/Sensor")] = 0.0f;
  const char* colors[] = {"Red", "Green", "Blue"};
  for (int c = 0; c < 3; ++c) {
    memory[device(std::string("ChestBoard/Led/") + colors[c] + "/Actuator")] = 0.0f;
    for (int side = 0; side < 2; ++side)
      for (int angle = 0; angle < 360; angle += 45) {
        char path[96];
        std::snprintf(path, sizeof path, "Face/Led/%s/%s/%dDeg/Actuator", colors[c],
                      side ? "Right" : "Left", angle);
        memory[device(path)] = 0.0f;
      }
  }
}

// Applies a command `[alias, update, "time-separate", 0, [time], [[value]...]]` to the memory.
static void apply(AL::DCMProxy& dcm, AL::ALMemoryProxy& memory, const AL::ALValue& command) {
  const std::string name = command[0];
  for (size_t a = 0; a < dcm.fAliases.size(); ++a) {
    if (std::string(dcm.fAliases[a][0]) != name) continue;
    const AL::ALValue& keys = dcm.fAliases[a][1];
    for (size_t k = 0; k < keys.getSize() && k < command[5].getSize(); ++k) {
      const std::string key = keys[k];
      const float value = command[5][k][0];
      memory.fData[key] = value;
      const std::string actuator = "/Position/Actuator/Value";
      if (key.size() > actuator.size() &&
          key.compare(key.size() - actuator.size(), actuator.size(), actuator) == 0)
        memory.fData[key.substr(0, key.size() - actuator.size()) + "/Position/Sensor/Value"] = value;
    }
  }
}

int main(int argc, char** argv) {
  if (argc < 3) {
    std::fprintf(stderr, "usage: %s <library> <seconds>\n", argv[0]);
    return 2;
  }
  const int cycles = std::atoi(argv[2]) * 100;

  boost::shared_ptr<AL::ALBrokerManager> manager(new AL::ALBrokerManager);
  boost::shared_ptr<AL::ALBroker> broker(new AL::ALBroker);
  broker->fBrokerManager = manager;
  broker->fDcm.reset(new AL::DCMProxy);
  broker->fMemory.reset(new AL::ALMemoryProxy);
  populate(broker->fMemory->fData);
  boost::shared_ptr<AL::ALModuleCore> dcm(new AL::ALModuleCore("DCM"));
  broker->fModules["DCM"] = dcm;

  void* handle = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
  if (!handle) {
    std::fprintf(stderr, "dlopen failed: %s\n", dlerror());
    return 1;
  }
  CreateModule create = reinterpret_cast<CreateModule>(dlsym(handle, "_createModule"));
  CloseModule close = reinterpret_cast<CloseModule>(dlsym(handle, "_closeModule"));
  if (!create || !close || create(broker) != 0) {
    std::fprintf(stderr, "_createModule failed\n");
    return 1;
  }
  std::printf("modules:");
  for (std::map<std::string, boost::shared_ptr<AL::ALModuleCore> >::iterator it = broker->fModules.begin();
       it != broker->fModules.end(); ++it)
    std::printf(" %s", it->first.c_str());
  std::printf("\naliases %d\n", static_cast<int>(broker->fDcm->fAliases.size()));
  std::fflush(stdout);

  size_t applied = 0;
  for (int i = 0; i < cycles; ++i) {
    dcm->stubPreProcess();
    for (; applied < broker->fDcm->fCommands.size(); ++applied)
      apply(*broker->fDcm, *broker->fMemory, broker->fDcm->fCommands[applied]);
    broker->fDcm->fTime += 10;
    dcm->stubPostProcess();
    usleep(10000);
  }

  std::map<std::string, std::pair<int, std::string> > summary;
  for (size_t i = 0; i < broker->fDcm->fCommands.size(); ++i) {
    std::pair<int, std::string>& entry = summary[std::string(broker->fDcm->fCommands[i][0])];
    entry.first += 1;
    entry.second = broker->fDcm->fCommands[i][5].toString();
  }
  for (std::map<std::string, std::pair<int, std::string> >::iterator it = summary.begin();
       it != summary.end(); ++it)
    std::printf("alias %s %d %s\n", it->first.c_str(), it->second.first, it->second.second.c_str());
  std::printf("commands %d\n", static_cast<int>(broker->fDcm->fCommands.size()));
  std::fflush(stdout);

  // NAOqi stopping: its modules are destroyed, then the library is closed.
  std::map<std::string, boost::shared_ptr<AL::ALModuleCore> > modules;
  modules.swap(broker->fModules);
  modules.clear();
  std::printf("destroyed\n_closeModule %d\n", close());
  std::fflush(stdout);
  return 0;
}
