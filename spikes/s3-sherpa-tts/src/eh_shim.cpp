// Compiled with g++ against libstdc++: throws and catches C++ exceptions so the
// combo binary proves libstdc++-built code still unwinds correctly when
// libc++abi (pulled in for libneedle) provides __cxa_throw/__gxx_personality_v0.
#include <stdexcept>
#include <string>
#include <vector>
extern "C" int eh_shim_libstdcxx(int n) {
    int caught = 0;
    for (int i = 0; i < n; ++i) {
        try {
            std::vector<int> v(3);
            (void)v.at(10 + i);  // throws std::out_of_range
        } catch (const std::out_of_range& e) {
            if (std::string(e.what()).size() > 0) ++caught;
        }
        try {
            throw std::runtime_error("x");
        } catch (const std::exception&) {
            ++caught;
        }
    }
    return caught;
}
