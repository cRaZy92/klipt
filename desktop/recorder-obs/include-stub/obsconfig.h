/* Stand-in for the obsconfig.h that OBS's CMake build generates. A source checkout of
 * libobs (what scripts/fetch-obs-windows.ps1 provides) doesn't contain it. These macros
 * only describe OBS's own install paths, which klipt-recorder never uses: it passes every
 * module path explicitly. build.rs puts this directory last on the include path, so a real
 * obsconfig.h from installed OBS headers takes precedence. */
#pragma once
#define OBS_DATA_PATH "../../data"
#define OBS_PLUGIN_PATH "../../obs-plugins/64bit"
#define OBS_PLUGIN_DESTINATION "obs-plugins/64bit"
#define OBS_INSTALL_PREFIX ""
#define OBS_RELEASE_CANDIDATE 0
#define OBS_BETA 0
