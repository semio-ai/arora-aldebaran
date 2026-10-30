// What the DCM bridge uses of the C++ SDK of NAOqi 2.1: see ../../README.md.
#ifndef ARORA_NAOQI21_ALCOMMON_ALBROKERMANAGER_H
#define ARORA_NAOQI21_ALCOMMON_ALBROKERMANAGER_H

#include <boost/shared_ptr.hpp>

namespace AL {

class ALBroker;

// The brokers of a process: a library NAOqi loads shares NAOqi's.
class ALBrokerManager {
 public:
  virtual ~ALBrokerManager();

  static boost::shared_ptr<ALBrokerManager> getInstance();
  static boost::shared_ptr<ALBrokerManager> setInstance(boost::shared_ptr<ALBrokerManager> manager);
  void addBroker(boost::shared_ptr<ALBroker> broker);

 private:
  ALBrokerManager();
};

}  // namespace AL

#endif
