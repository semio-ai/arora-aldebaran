// What the DCM bridge uses of the C++ SDK of NAOqi 2.1: see ../../README.md.
#ifndef ARORA_NAOQI21_ALPROXIES_DCMPROXY_H
#define ARORA_NAOQI21_ALPROXIES_DCMPROXY_H

#include <alvalue/alvalue.h>

#include <boost/shared_ptr.hpp>

namespace AL {

class ALProxy;

namespace detail {

// The calls of the DCM that return at once, through the proxy.
class DCMProxyPostHandler {
 private:
  boost::shared_ptr<ALProxy> fProxy;
};

}  // namespace detail

// The proxy to the DCM.
class DCMProxy {
 public:
  DCMProxy(boost::shared_ptr<ALProxy> proxy);

  ALValue createAlias(const ALValue& alias);
  // The time of the DCM, in ms, plus `offset`.
  int getTime(const int& offset);
  void setAlias(const ALValue& commands);

 private:
  boost::shared_ptr<ALProxy> fProxy;

 public:
  detail::DCMProxyPostHandler post;
};

}  // namespace AL

#endif
