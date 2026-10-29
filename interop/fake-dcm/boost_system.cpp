// The two functions of Boost.System (before 1.69, where it is not header-only) that the bridge
// to the DCM uses, for the 32-bit fake-dcm: in NAOqi, they are those of the libboost_system its
// libraries load.
#include <boost/system/error_code.hpp>
#include <string>

namespace {
struct Category : boost::system::error_category {
  const char* name() const BOOST_SYSTEM_NOEXCEPT { return "fake-dcm"; }
  std::string message(int) const { return "fake-dcm"; }
};
}  // namespace

namespace boost {
namespace system {
const error_category& system_category() BOOST_SYSTEM_NOEXCEPT {
  static Category category;
  return category;
}
const error_category& generic_category() BOOST_SYSTEM_NOEXCEPT {
  static Category category;
  return category;
}
}  // namespace system
}  // namespace boost
