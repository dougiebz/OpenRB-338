# The Audio Unit (AUv2) of OpenRB-338: free-audio's clap-wrapper around the CLAP plugin that
# crates/orb-vst builds. scripts/package-plugin-macos.sh runs this; macOS only.
#
#   cmake -S crates/orb-vst/au -B BUILD -DOPENRB_CLAP=/abs/path/OpenRB-338.clap
#   cmake --build BUILD --config Release
#
# The result is BUILD/OpenRB-338.component with the .clap copied inside it, so the component is
# all a host needs. clap-wrapper is MIT; it downloads the CLAP SDK (MIT) and Apple's
# AudioUnitSDK (Apache 2.0) at configure time. Nothing of them is committed here.
cmake_minimum_required(VERSION 3.21)

set(CMAKE_OSX_DEPLOYMENT_TARGET "11.0" CACHE STRING "as scripts/build.sh")
project(OpenRB338AudioUnit VERSION 0.1.0 LANGUAGES C CXX OBJC OBJCXX)

if (NOT APPLE)
    message(FATAL_ERROR "the Audio Unit builds on macOS only")
endif ()
if (NOT DEFINED OPENRB_CLAP OR NOT EXISTS "${OPENRB_CLAP}")
    message(FATAL_ERROR "pass -DOPENRB_CLAP=/abs/path/OpenRB-338.clap (the built CLAP bundle)")
endif ()

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CLAP_WRAPPER_DOWNLOAD_DEPENDENCIES ON CACHE BOOL "fetch the CLAP SDK and the AudioUnitSDK")
set(CLAP_WRAPPER_BUILD_AUV2 ON CACHE BOOL "the AUv2 wrapper")
set(CLAP_WRAPPER_BUILD_TESTS OFF CACHE BOOL "")

include(FetchContent)
FetchContent_Declare(
    clap-wrapper
    GIT_REPOSITORY https://github.com/free-audio/clap-wrapper.git
    # v0.17.0
    GIT_TAG 806bba0a5f792e3966b99c1561ab9952d3316d7c
)
FetchContent_MakeAvailable(clap-wrapper)

add_library(openrb338_auv2 MODULE)
# The four-character codes name the unit to every host (and to `auval -v aumu Rb38 Orb3`):
# they must never change once a project has been saved with the unit.
target_add_auv2_wrapper(
    TARGET openrb338_auv2
    OUTPUT_NAME "OpenRB-338"
    BUNDLE_IDENTIFIER "net.emphased.openrb338.auv2"
    BUNDLE_VERSION "${PROJECT_VERSION}"
    MANUFACTURER_NAME "OpenRB-338"
    MANUFACTURER_CODE "Orb3"
    SUBTYPE_CODE "Rb38"
    INSTRUMENT_TYPE "aumu"
    MACOS_EMBEDDED_CLAP_LOCATION "${OPENRB_CLAP}"
)
