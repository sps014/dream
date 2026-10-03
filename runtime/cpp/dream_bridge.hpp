// Support code for Dream's generated `@cpp` shims (`shim.cpp`). Each shim function converts its C
// arguments with the adapters below, calls the C++ API, and converts the result with a
// `dream::out_*` helper chosen by the Dream declaration; the helper inspects the real C++ return
// type, so ownership and string shapes follow the header rather than the declaration.
#pragma once

#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <exception>
#include <functional>
#include <memory>
#include <optional>
#include <string>
#include <string_view>
#include <type_traits>
#include <utility>
#include <vector>
#if __has_include(<span>)
#include <span>
#endif

extern "C" void dream_callback_retain(void* obj);
extern "C" void dream_callback_release(void* obj);

namespace dream {

template <class> inline constexpr bool dependent_false = false;

template <class T> struct is_optional : std::false_type {};
template <class T> struct is_optional<std::optional<T>> : std::true_type {};
template <class T> struct is_unique_ptr : std::false_type {};
template <class T, class D> struct is_unique_ptr<std::unique_ptr<T, D>> : std::true_type {};

template <class T> using bare = std::remove_cv_t<std::remove_reference_t<T>>;

inline std::string& last_error_buf() {
    static thread_local std::string s;
    return s;
}

inline const char* last_error() { return last_error_buf().c_str(); }

[[noreturn]] inline void fail(const char* site, const char* what) {
    std::fprintf(stderr, "panic: %s: %s\n", site, what);
    std::fflush(stderr);
    std::abort();
}

// A member whose Dream declaration is not `Result<T, string>`: an exception is a panic.
template <class F> decltype(auto) trap(const char* site, F&& f) {
#if defined(__cpp_exceptions)
    try {
        return f();
    } catch (const std::exception& e) {
        fail(site, e.what());
    } catch (...) {
        fail(site, "unknown C++ exception");
    }
#else
    (void)site;
    return f();
#endif
}

// A `Result<T, string>` member: an exception sets `*failed` and the message for `last_error`.
template <class R, class F> R attempt(int32_t* failed, R fallback, F&& f) {
#if defined(__cpp_exceptions)
    try {
        return f();
    } catch (const std::exception& e) {
        last_error_buf() = e.what();
    } catch (...) {
        last_error_buf() = "unknown C++ exception";
    }
    *failed = 1;
    return fallback;
#else
    (void)failed;
    (void)fallback;
    return f();
#endif
}

// ---- results --------------------------------------------------------------------------------

// Dream copies a returned string before the next call on this thread.
inline const char* keep(std::string s) {
    static thread_local std::string buf;
    buf = std::move(s);
    return buf.c_str();
}

template <class F> const char* out_str(F&& f) {
    using R = decltype(f());
    if constexpr (std::is_convertible_v<R, const char*>) {
        const char* p = f();
        return p ? keep(p) : nullptr;
    } else if constexpr (std::is_constructible_v<std::string, R>) {
        return keep(std::string(f()));
    } else {
        static_assert(dependent_false<R>,
                      "a `string` @cpp member must return std::string, std::string_view, or const char*");
    }
}

template <class F> const char* out_opt_str(F&& f) {
    using R = bare<decltype(f())>;
    if constexpr (is_optional<R>::value) {
        auto o = f();
        return o ? keep(std::string(*o)) : nullptr;
    } else {
        return out_str(std::forward<F>(f));
    }
}

// `void` C++ members are declared `Result<bool, string>` and yield `Ok(true)`.
template <class C, class F> C out_scalar(F&& f) {
    using R = decltype(f());
    if constexpr (std::is_void_v<R>) {
        f();
        return C(1);
    } else {
        return static_cast<C>(f());
    }
}

template <class F> void* out_ptr(F&& f) {
    using R = bare<decltype(f())>;
    static_assert(std::is_pointer_v<R>, "a `CPtr` @cpp member must return a pointer");
    return const_cast<void*>(static_cast<const volatile void*>(f()));
}

// By value, `std::optional<T>`, or `std::unique_ptr<T>`: owned. `T*`: borrowed unless `@owned`.
// `T&`: borrowed.
template <class T, bool Owned, bool Optional, class F>
void* out_obj(const char* site, F&& f, int32_t* owned) {
    using R = decltype(f());
    using D = bare<R>;
    void* p = nullptr;
    if constexpr (std::is_pointer_v<D>) {
        p = const_cast<void*>(static_cast<const void*>(static_cast<const T*>(f())));
        *owned = Owned ? 1 : 0;
    } else if constexpr (std::is_lvalue_reference_v<R>) {
        p = const_cast<void*>(static_cast<const void*>(&static_cast<const T&>(f())));
        *owned = 0;
    } else if constexpr (is_unique_ptr<D>::value) {
        p = static_cast<T*>(f().release());
        *owned = 1;
    } else if constexpr (is_optional<D>::value) {
        auto o = f();
        if (o) {
            p = new T(std::move(*o));
        }
        *owned = 1;
    } else {
        p = new T(f());
        *owned = 1;
    }
    if constexpr (!Optional) {
        if (!p) {
            fail(site, "returned null where the Dream declaration is not Option");
        }
    }
    return p;
}

// ---- arguments ------------------------------------------------------------------------------

struct any_ptr {
    void* p;
    template <class T> operator T*() const { return static_cast<T*>(p); }
};

struct opt_str {
    const char* p;
    operator const char*() const { return p; }
    operator std::optional<std::string>() const {
        return p ? std::optional<std::string>(p) : std::nullopt;
    }
    operator std::optional<std::string_view>() const {
        return p ? std::optional<std::string_view>(p) : std::nullopt;
    }
};

template <class T> struct obj {
    void* p;
    operator T&() const { return *static_cast<T*>(p); }
    operator T*() const { return static_cast<T*>(p); }
};

template <class T> struct slice {
    const T* p;
    int32_t n;
    operator std::vector<T>() const { return std::vector<T>(p, p + n); }
#if defined(__cpp_lib_span)
    operator std::span<const T>() const { return {p, static_cast<size_t>(n)}; }
    operator std::span<T>() const { return {const_cast<T*>(p), static_cast<size_t>(n)}; }
#endif
};

// ---- callbacks ------------------------------------------------------------------------------

// Holds the Dream `NativeCallback` for as long as any copy of the `std::function` lives.
struct callback_ref {
    void* obj;
    explicit callback_ref(void* o) : obj(o) { dream_callback_retain(obj); }
    ~callback_ref() { dream_callback_release(obj); }
    callback_ref(const callback_ref&) = delete;
    callback_ref& operator=(const callback_ref&) = delete;
};

template <class A> struct to_c {
    A v;
    template <class X> to_c(X&& x) : v(static_cast<A>(std::forward<X>(x))) {}
    A get() const { return v; }
};

template <> struct to_c<const char*> {
    std::optional<std::string> s;
    to_c(const char* p) : s(p ? std::optional<std::string>(p) : std::nullopt) {}
    template <class X,
              std::enable_if_t<std::is_constructible_v<std::string, X> &&
                                   !std::is_convertible_v<X, const char*>,
                               int> = 0>
    to_c(X&& x) : s(std::string(std::forward<X>(x))) {}
    template <class X> to_c(const std::optional<X>& o) : s(o ? std::optional<std::string>(std::string(*o)) : std::nullopt) {}
    const char* get() const { return s ? s->c_str() : nullptr; }
};

template <> struct to_c<void*> {
    void* v;
    template <class X> to_c(X* p) : v(const_cast<void*>(static_cast<const volatile void*>(p))) {}
    to_c(std::nullptr_t) : v(nullptr) {}
    void* get() const { return v; }
};

template <class R, class... A> struct callback {
    R (*fn)(void*, A...);
    std::shared_ptr<callback_ref> ref;

    callback(void* f, void* user_data)
        : fn(reinterpret_cast<R (*)(void*, A...)>(f)), ref(std::make_shared<callback_ref>(user_data)) {}

    template <class... X> auto operator()(X&&... x) const {
        static_assert(sizeof...(X) == sizeof...(A),
                      "the C++ callback's arity differs from the Dream `fun` type");
        void* ud = ref->obj;
        if constexpr (std::is_void_v<R>) {
            fn(ud, to_c<A>(std::forward<X>(x)).get()...);
        } else if constexpr (std::is_same_v<R, void*>) {
            return any_ptr{fn(ud, to_c<A>(std::forward<X>(x)).get()...)};
        } else {
            return fn(ud, to_c<A>(std::forward<X>(x)).get()...);
        }
    }
};

}  // namespace dream
