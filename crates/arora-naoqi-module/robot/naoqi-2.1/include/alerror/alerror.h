// What the DCM bridge uses of the C++ SDK of NAOqi 2.1: see ../../README.md.
#ifndef ARORA_NAOQI21_ALERROR_ALERROR_H
#define ARORA_NAOQI21_ALERROR_ALERROR_H

#include <stdexcept>
#include <string>

namespace AL {

// The exception the modules of NAOqi throw.
class ALError : public std::runtime_error {
 public:
  ALError(const ALError& other);
  virtual ~ALError() throw();

 private:
  ALError();
};

}  // namespace AL

#endif
