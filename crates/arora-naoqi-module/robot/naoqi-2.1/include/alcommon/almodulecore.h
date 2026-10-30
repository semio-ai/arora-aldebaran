// What the DCM bridge uses of the C++ SDK of NAOqi 2.1: see ../../README.md.
#ifndef ARORA_NAOQI21_ALCOMMON_ALMODULECORE_H
#define ARORA_NAOQI21_ALCOMMON_ALMODULECORE_H

#include <alerror/alerror.h>
#include <alvalue/alvalue.h>

#include <boost/enable_shared_from_this.hpp>
#include <boost/noncopyable.hpp>
#include <boost/shared_ptr.hpp>
#include <stdint.h>
#include <string>

namespace qi {

// The types of libqi 2.1 in the virtual functions of NAOqi's modules, by name only: a module
// written here inherits these functions, and calls none.
struct Empty;
template <typename T>
class Object;
typedef Object<Empty> AnyObject;
template <typename T>
class Future;
class AnyReference;
class AnyValue;
class GenericFunctionParameters;
class MetaObject;
class Signature;
class SignalSubscriber;
enum MetaCallType {};
typedef uint64_t SignalLink;

// The object of libqi 2.1 under NAOqi's modules.
class DynamicObject {
 public:
  virtual ~DynamicObject();
  virtual void setMetaObject(const MetaObject& metaObject);
  virtual Future<AnyReference> metaCall(AnyObject context, unsigned int method,
                                        const GenericFunctionParameters& parameters,
                                        MetaCallType callType, Signature returnSignature);
  virtual void metaPost(AnyObject context, unsigned int event,
                        const GenericFunctionParameters& parameters);
  virtual Future<SignalLink> metaConnect(unsigned int event, const SignalSubscriber& subscriber);
  virtual Future<void> metaDisconnect(SignalLink link);
  virtual Future<AnyValue> metaProperty(unsigned int property);
  virtual Future<void> metaSetProperty(unsigned int property, AnyValue value);

 private:
  DynamicObject();
  // Its private part, a shared pointer.
  void* fPrivate[2];
};

}  // namespace qi

namespace AL {

class ALBroker;
class ALMethodInfo;

// The base of NAOqi's modules.
class ALModuleCore : public boost::enable_shared_from_this<ALModuleCore>,
                     public boost::noncopyable,
                     public qi::DynamicObject {
 public:
  ALModuleCore(boost::shared_ptr<ALBroker> broker, const std::string& name);
  virtual ~ALModuleCore();

  // Registers the module in its broker.
  void initModule();
  boost::shared_ptr<ALBroker> getParentBroker() const;
  void setModuleDescription(const std::string& description);

  virtual ALMethodInfo* execute(const std::string& method, const ALValue& parameters,
                                ALValue& result);
  virtual std::string version();
  virtual void exit();
  virtual std::string httpGet();
  virtual qi::Future<qi::AnyReference> metaCall(qi::AnyObject context, unsigned int method,
                                                const qi::GenericFunctionParameters& parameters,
                                                qi::MetaCallType callType,
                                                qi::Signature returnSignature);

 protected:
  virtual void init() {}

 private:
  // Its methods, their descriptions and its private part.
  void* fPrivate[21];
};

}  // namespace AL

#endif
