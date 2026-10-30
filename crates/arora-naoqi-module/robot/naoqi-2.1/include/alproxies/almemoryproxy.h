// What the DCM bridge uses of the C++ SDK of NAOqi 2.1: see ../../README.md.
#ifndef ARORA_NAOQI21_ALPROXIES_ALMEMORYPROXY_H
#define ARORA_NAOQI21_ALPROXIES_ALMEMORYPROXY_H

#include <alvalue/alvalue.h>

#include <boost/shared_ptr.hpp>
#include <string>

namespace AL {

class ALProxy;

namespace detail {

// The calls of ALMemory that return at once, through the proxy.
class ALMemoryProxyPostHandler {
 private:
  boost::shared_ptr<ALProxy> fProxy;
};

}  // namespace detail

// The proxy to ALMemory.
class ALMemoryProxy {
 public:
  ALMemoryProxy(boost::shared_ptr<ALProxy> proxy);

  // Where the value of `key` is, which the modules of the process read and write directly.
  void* getDataPtr(const std::string& key);

 private:
  boost::shared_ptr<ALProxy> fProxy;

 public:
  detail::ALMemoryProxyPostHandler post;
};

}  // namespace AL

#endif
