// What the DCM bridge uses of the C++ SDK of NAOqi 2.1: see ../../README.md.
#ifndef ARORA_NAOQI21_ALCOMMON_ALPROXY_H
#define ARORA_NAOQI21_ALCOMMON_ALPROXY_H

#include <alcommon/almodule.h>

#include <boost/enable_shared_from_this.hpp>
#include <boost/shared_ptr.hpp>

namespace AL {

// A proxy to a module, which NAOqi gives through a shared pointer.
class ALProxy : public boost::enable_shared_from_this<ALProxy> {
 public:
  virtual ~ALProxy();

  // The module itself, when it is in this process.
  boost::shared_ptr<ALModule> getModule() const;

 private:
  ALProxy();
  ALProxy(const ALProxy&);
};

}  // namespace AL

#endif
