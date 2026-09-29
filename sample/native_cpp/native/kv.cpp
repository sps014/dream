#include "kv.hpp"
#include <cmath>
#include <cstdio>

namespace kv {
Entry::~Entry() {}
Store::Store(const std::string& path) : path_(path) {}
Store::~Store() { std::printf("[c++] ~Store(%s)\n", path_.c_str()); std::fflush(stdout); }
void Store::put(const std::string& k, const std::string& v) {
    entries_[k] = Entry{k, v};
    for (auto& l : listeners_) l(k);
}
std::optional<std::string> Store::get(const std::string& k) const {
    auto it = entries_.find(k);
    if (it == entries_.end()) return std::nullopt;
    return it->second.value;
}
Entry* Store::find(const std::string& k) {
    auto it = entries_.find(k);
    return it == entries_.end() ? nullptr : &it->second;
}
Entry Store::snapshot(const std::string& k) const { return entries_.at(k); }
void Store::compact() {
    if (entries_.empty()) throw std::runtime_error("nothing to compact");
}
void Store::on_change(std::function<void(const std::string&)> f) { listeners_.push_back(std::move(f)); }
template <> int Store::get_as<int>(const std::string& k) const { return std::stoi(entries_.at(k).value); }
int Store::count() const { return (int)entries_.size(); }
int Store::count(const std::string& p) const {
    int n = 0;
    for (auto& [k, _] : entries_) n += k.rfind(p, 0) == 0;
    return n;
}
int Store::scaled(int f) const { return count() * f; }
long Store::sum(const std::vector<int>& xs) const { long s = 0; for (int x : xs) s += x; return s; }
double Store::norm(geom::Point p) const { return std::sqrt(p.x * p.x + p.y * p.y); }
void Store::origin(geom::Point& out) const { out.x = 1.5; out.y = -2.0; }
Store* Store::open_default() { return new Store("default"); }
std::string Store::version() { return "kv 1.0"; }
std::string greet(const std::string& who) { return "hello, " + who; }
}
