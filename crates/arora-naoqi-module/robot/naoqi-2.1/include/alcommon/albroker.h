// What the DCM bridge uses of the C++ SDK of NAOqi 2.1: see ../../README.md.
#ifndef ARORA_NAOQI21_ALCOMMON_ALBROKER_H
#define ARORA_NAOQI21_ALCOMMON_ALBROKER_H

#include <alcommon/alproxy.h>

#include <boost/enable_shared_from_this.hpp>
#include <boost/noncopyable.hpp>
#include <boost/shared_ptr.hpp>
#include <boost/weak_ptr.hpp>
#include <string>

namespace AL {

class ALBrokerManager;

// The broker of a process of NAOqi, which gives the proxies to its modules.
class ALBroker : public boost::enable_shared_from_this<ALBroker>, public boost::noncopyable {
 public:
  virtual ~ALBroker();

  boost::shared_ptr<ALProxy> getProxy(const std::string& name, int deprecated = 0);

  // The proxy `T` (DCMProxy, ALMemoryProxy...) to the module `name`, created here.
  template <typename T>
  boost::shared_ptr<T> getSpecialisedProxy(const std::string& name = std::string()) {
    return boost::shared_ptr<T>(new T(getProxy(name)));
  }
// As in NAOqi 2.1, the proxies of the DCM and ALMemory.
#define getMemoryProxy() getSpecialisedProxy<AL::ALMemoryProxy>("ALMemory")
#define getDcmProxy() getSpecialisedProxy<AL::DCMProxy>("DCM")

  boost::weak_ptr<ALBrokerManager> fBrokerManager;

 private:
  ALBroker();
  void* fPrivate;
};

}  // namespace AL

#endif
