#pragma once

#include <string>
#include <vector>

namespace dream::gpu {

/// Starts recording every image the dynamic loader maps from now on, including images that
/// are unloaded again before the final check.
void watchLoadedImages();

/// Images whose file name marks them as part of the Slang compiler, seen at any point since
/// `watchLoadedImages` or loaded right now.
std::vector<std::string> slangCompilerImages();

/// Every image currently mapped, for the report.
std::vector<std::string> loadedImages();

} // namespace dream::gpu
