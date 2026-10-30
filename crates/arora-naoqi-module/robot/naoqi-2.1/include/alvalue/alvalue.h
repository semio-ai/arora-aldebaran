// What the DCM bridge uses of the C++ SDK of NAOqi 2.1: see ../../README.md.
#ifndef ARORA_NAOQI21_ALVALUE_ALVALUE_H
#define ARORA_NAOQI21_ALVALUE_ALVALUE_H

#include <string>

namespace AL {

// The values of NAOqi's calls: a type and a word, a pointer to the array or string it holds.
class ALValue {
 public:
  ALValue();
  ALValue(const ALValue& other);
  ALValue(const std::string& value);
  virtual ~ALValue();

  ALValue& operator=(const ALValue& other);
  ALValue& operator=(const int& value);
  ALValue& operator=(const float& value);

  // The element `index` of an array.
  ALValue& operator[](int index);
  // Makes it an array of `size` elements.
  void arraySetSize(int size);

 private:
  int fType;
  union {
    void* pointer;
    int integer;
    float real;
  } fValue;
};

}  // namespace AL

#endif
