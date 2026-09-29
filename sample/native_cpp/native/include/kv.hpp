#pragma once
#include <functional>
#include <map>
#include <optional>
#include <stdexcept>
#include <string>
#include <vector>

namespace geom {
struct Point { double x; double y; };
}

namespace kv {
struct Entry {
    std::string key;
    std::string value;
    const std::string& name() const { return key; }
    std::string_view val() const { return value; }
    ~Entry();
};

class Store {
public:
    explicit Store(const std::string& path);
    ~Store();
    void put(const std::string& key, const std::string& value);
    std::optional<std::string> get(const std::string& key) const;
    Entry* find(const std::string& key);
    Entry snapshot(const std::string& key) const;
    void compact();
    void on_change(std::function<void(const std::string&)> f);
    template <class T> T get_as(const std::string& key) const;
    int count() const;
    int count(const std::string& prefix) const;
    int scaled(int factor = 10) const;
    long sum(const std::vector<int>& xs) const;
    double norm(geom::Point p) const;
    void origin(geom::Point& out) const;
    static Store* open_default();
    static std::string version();
private:
    std::string path_;
    std::map<std::string, Entry> entries_;
    std::vector<std::function<void(const std::string&)>> listeners_;
};

template <> int Store::get_as<int>(const std::string& key) const;
std::string greet(const std::string& who);
}
