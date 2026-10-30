// The layout of the classes of NAOqi the DCM bridge uses, for check-abi.sh: each probe_ function
// returns a size or an offset, which the script reads in its code. Compiled, never run.
#include <alcommon/albroker.h>
#include <alcommon/albrokermanager.h>
#include <alcommon/almodule.h>
#include <alerror/alerror.h>
#include <alproxies/almemoryproxy.h>
#include <alproxies/dcmproxy.h>
#include <alvalue/alvalue.h>

#include <cstddef>

namespace {

struct Signals : AL::detail::ALProcessSignals {
  static std::size_t pre() { return offsetof(Signals, fPreProcess); }
  static std::size_t post() { return offsetof(Signals, fPostProcess); }
};

// Where the members of a module written on ALModule start.
struct Module : AL::ALModule {
  Module() : AL::ALModule(boost::shared_ptr<AL::ALBroker>(), "") {}
  char member;
};

template <class Base, class Derived>
std::size_t base() {
  Derived* derived = reinterpret_cast<Derived*>(0x1000);
  return reinterpret_cast<char*>(static_cast<Base*>(derived)) - reinterpret_cast<char*>(derived);
}

}  // namespace

#define PROBE(name, value) \
  extern "C" std::size_t probe_##name() { return value; }

PROBE(module_size, sizeof(AL::ALModule))
PROBE(module_align, __alignof__(AL::ALModule))
PROBE(module_members, offsetof(Module, member))
PROBE(module_core, (base<AL::ALModuleCore, AL::ALModule>()))
PROBE(module_dynamic_object, (base<qi::DynamicObject, AL::ALModule>()))
PROBE(module_shared_from_this, (base<boost::enable_shared_from_this<AL::ALModuleCore>, AL::ALModule>()))
PROBE(module_signals, (base<AL::detail::ALProcessSignals, AL::ALModule>()))
PROBE(module_core_size, sizeof(AL::ALModuleCore))
PROBE(dynamic_object_size, sizeof(qi::DynamicObject))
PROBE(signals_size, sizeof(AL::detail::ALProcessSignals))
PROBE(signals_pre, Signals::pre())
PROBE(signals_post, Signals::post())
PROBE(broker_size, sizeof(AL::ALBroker))
PROBE(broker_shared_from_this, (base<boost::enable_shared_from_this<AL::ALBroker>, AL::ALBroker>()))
PROBE(broker_manager, offsetof(AL::ALBroker, fBrokerManager))
PROBE(proxy_shared_from_this, (base<boost::enable_shared_from_this<AL::ALProxy>, AL::ALProxy>()))
PROBE(dcm_proxy_size, sizeof(AL::DCMProxy))
PROBE(memory_proxy_size, sizeof(AL::ALMemoryProxy))
PROBE(value_size, sizeof(AL::ALValue))
PROBE(value_align, __alignof__(AL::ALValue))
PROBE(error_size, sizeof(AL::ALError))
