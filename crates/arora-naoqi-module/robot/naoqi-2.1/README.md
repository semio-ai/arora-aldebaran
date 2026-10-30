# What the DCM bridge uses of NAOqi 2.1

The DCM bridge (`cpp/naoqi_shim.cpp` of libqi-vibe) is C++ built against the C++ SDK of the
robot's NAOqi: it creates an `ALModule`, takes the proxies of the DCM and `ALMemory`, and
connects to the callbacks of the DCM. `include/` declares that, and only that, with the ABI of
NAOqi 2.1, so that `build.sh` needs no SDK. The SDK's headers are Aldebaran's, and not ours to
copy: these declarations are written here.

The bridge's code must be the code it would be with the SDK. NAOqi's code runs on the objects
the bridge creates, and the bridge's code on NAOqi's, so the declarations match the SDK in:

- the layout of the classes: their bases and where they are, their sizes, the members the
  bridge reaches (`fBrokerManager` of `ALBroker`, the signals of `ALProcessSignals`). The
  private members are pointer-sized words, as many as the SDK's take;
- the virtual functions of `ALModule`, its bases included (libqi's `qi::DynamicObject`), in the
  order of the SDK's vtable, with the exact types of their parameters: a module written on
  `ALModule` has the same vtable;
- the names, parameters and return types of the functions the bridge calls, which NAOqi's
  libraries define;
- the inline code the bridge instantiates: `ALModule::createModule`,
  `ALBroker::getSpecialisedProxy`, `ALProcessSignals::atPreProcess`;
- Boost 1.55 (NAOqi 2.1's, the image's), for Boost.Signals, `boost::shared_ptr` and
  `enable_shared_from_this`: they are in the layouts and in the inline code.

`../check-abi.sh <cross toolchain of NAOqi 2.1>` checks it, in 32 and 64 bits:

- `abi-probe.cpp` compiled against the SDK and against these declarations gives the same 21
  sizes and offsets;
- the bridge compiled against either (without optimization, so that each function is its own)
  needs no symbol more with these declarations and defines no function, vtable or typeinfo
  more, and the ones both define have the same code and data, but for where the local data of
  the object is. With the SDK, the object has more: the templates of libqi its headers
  instantiate, which the bridge does not use.

It passed on 2026-10-30 against `ctc-linux64-atom-2.1.4.13` (SHA-256 `6b72eb98…`, see the
readme): 519 functions, vtables and typeinfos the same in 32 bits, 517 in 64 bits. It fails on
any of a changed order of virtual functions, a private member less, a member removed from a
proxy, a wrong return type or a wrong parameter type. Run it again when the bridge changes
(a new version of libqi-vibe): a bridge using more of the SDK needs more declarations here.

These declarations are NAOqi 2.1's: another NAOqi has its own ABI (its compiler, its Boost, its
libqi, whose `qi::DynamicObject` is under every `ALModule`), and needs its own declarations,
checked against its own cross toolchain.
