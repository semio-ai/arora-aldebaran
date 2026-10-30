// What the DCM bridge uses of the C++ SDK of NAOqi 2.1: see ../../README.md.
#ifndef ARORA_NAOQI21_ALCOMMON_ALMODULE_H
#define ARORA_NAOQI21_ALCOMMON_ALMODULE_H

#include <alcommon/almodulecore.h>

#include <boost/pointer_cast.hpp>
#include <boost/shared_ptr.hpp>
#include <string>

// NAOqi 2.1 has the callbacks of its modules on Boost.Signals, which Boost 1.55 deprecates.
#ifndef BOOST_SIGNALS_NO_DEPRECATION_WARNING
#define BOOST_SIGNALS_NO_DEPRECATION_WARNING
#endif
#include <boost/signal.hpp>

namespace AL {

class ALBroker;

namespace detail {

// The callbacks of a module before and after each cycle of its loop: the DCM's are those of
// the DCM plugins.
class ALProcessSignals {
 public:
  typedef boost::signal<void()> ProcessSignal;
  typedef ProcessSignal::slot_function_type ProcessSignalSlot;
  typedef boost::signals::connect_position ProcessSignalPosition;
  typedef boost::signals::connection ProcessSignalConnection;

  ALProcessSignals() {}
  virtual ~ALProcessSignals() {}

  ProcessSignalConnection atPreProcess(ProcessSignalSlot slot,
                                       ProcessSignalPosition position = boost::signals::at_back) {
    return fPreProcess.connect(slot, position);
  }
  ProcessSignalConnection atPostProcess(ProcessSignalSlot slot,
                                        ProcessSignalPosition position = boost::signals::at_back) {
    return fPostProcess.connect(slot, position);
  }

 protected:
  ProcessSignal fPreProcess;
  ProcessSignal fPostProcess;
};

}  // namespace detail

// A module of NAOqi written in C++.
class ALModule : public ALModuleCore, public detail::ALProcessSignals {
 public:
  // Creates the module `T`, constructed with the broker and `argument`, registers it, then
  // initializes it: one whose initialization throws an `ALError` exits, and the error goes on.
  template <class T, typename A>
  static boost::shared_ptr<T> createModule(boost::shared_ptr<ALBroker> broker, A argument) {
    boost::shared_ptr<T> module(new T(broker, argument));
    module->initModule();
    try {
      boost::static_pointer_cast<ALModule>(module)->init();
    } catch (const ALError& error) {
      module->exit();
      throw(error);
    }
    return module;
  }

  ALModule(boost::shared_ptr<ALBroker> broker, const std::string& name);
  virtual ~ALModule();

  virtual std::string httpGet();
  virtual void stop(const int& task);
  virtual void exit();
  virtual void init() {}
};

}  // namespace AL

#endif
